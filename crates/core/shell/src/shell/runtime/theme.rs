use super::super::*;

const THEME_RELOAD_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
const SHELL_SETTINGS_RELOAD_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(500);

fn profile_shell_theme_object(
    settings: &mut serde_json::Value,
) -> &mut serde_json::Map<String, serde_json::Value> {
    let object = settings
        .as_object_mut()
        .expect("profile shell settings must be an object");
    if !object
        .get("theme")
        .is_some_and(serde_json::Value::is_object)
    {
        object.insert("theme".into(), serde_json::json!({}));
    }
    object
        .get_mut("theme")
        .and_then(serde_json::Value::as_object_mut)
        .expect("theme settings object was just initialized")
}

fn set_profile_theme_selection(settings: &mut serde_json::Value, theme_id: &str) {
    if !settings.is_object() {
        *settings = serde_json::json!({});
    }
    let theme = profile_shell_theme_object(settings);
    theme.insert("active".into(), serde_json::Value::String(theme_id.into()));
    theme.remove("mode");
}

fn set_profile_theme_mode(settings: &mut serde_json::Value, mode: &str) {
    if !settings.is_object() {
        *settings = serde_json::json!({});
    }
    let theme = profile_shell_theme_object(settings);
    theme.insert("mode".into(), serde_json::Value::String(mode.into()));
}

/// A validated settings candidate that has not crossed its durable commit
/// boundary yet. Theme and locale preparation consume this value before any
/// live shell state changes, so a rejected candidate cannot leave persistence
/// and runtime state divergent.
pub(in crate::shell) struct ControlPlaneSettingsCandidate {
    pub(in crate::shell) store: SettingsStore,
    pub(in crate::shell) shared_revision: u64,
    pub(in crate::shell) profile_commit: Option<(
        mesh_core_module::package::ProfilePaths,
        String,
        mesh_core_module::package::ShellProfile,
        u64,
    )>,
}

pub(in crate::shell) struct ControlPlaneSettingsCommit {
    pub(in crate::shell) store: SettingsStore,
    pub(in crate::shell) revision: DurableControlPlaneRevision,
}

fn theme_source_fingerprint(shell: &Shell) -> Result<u64, mesh_core_theme::ThemeError> {
    let graph = shell.installed_module_graph.as_ref().ok_or_else(|| {
        mesh_core_theme::ThemeError::NotFound(shell.settings.theme.active.clone())
    })?;
    let descriptor = graph
        .theme_catalog()
        .get(&shell.settings.theme.active)
        .ok_or_else(|| {
            mesh_core_theme::ThemeError::NotFound(shell.settings.theme.active.clone())
        })?;
    let mode = selected_theme_mode(&shell.settings, descriptor)?;
    let source = descriptor.mode(&mode).ok_or_else(|| {
        mesh_core_theme::ThemeError::Composition(format!(
            "theme '{}' has no mode '{mode}'",
            descriptor.id
        ))
    })?;
    source
        .source
        .fingerprint()
        .map_err(|error| mesh_core_theme::ThemeError::Source {
            path: source.source.candidate_path(),
            message: error.to_string(),
        })
}

fn theme_preview_palette(theme: &mesh_core_theme::Theme) -> serde_json::Value {
    let color = |name: &str, fallback: &str| {
        theme
            .token(name)
            .map(ToString::to_string)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| fallback.to_string())
    };
    let surface = color("color.surface", "transparent");
    let on_surface = color("color.on-surface", "currentcolor");

    serde_json::json!({
        "surface": surface,
        "surface_container_low": color("color.surface-container-low", &surface),
        "surface_container_high": color("color.surface-container-high", &surface),
        "primary": color("color.primary", &on_surface),
        "outline_variant": color("color.outline-variant", &on_surface),
        "on_surface": on_surface,
    })
}

fn system_resources_json(settings: &ShellSettings) -> serde_json::Value {
    let resources = mesh_core_resources::system_resource_catalog();
    serde_json::json!({
        "active_icon_theme": settings.icons.default_pack,
        "active_font_family": settings.fonts.ui_family,
        "icon_themes": resources.icon_themes.iter().filter(|theme| !theme.hidden).map(|theme| serde_json::json!({
            "id": theme.id,
            "name": theme.name,
            "path": theme.path,
            "inherits": theme.inherits,
        })).collect::<Vec<_>>(),
        "font_families": resources.font_families.iter().map(|family| serde_json::json!({
            "name": family.name,
            "face_count": family.face_count,
            "monospace": family.monospace,
        })).collect::<Vec<_>>(),
    })
}

impl Shell {
    /// Prepare a shared or active-profile settings candidate. The closure
    /// receives the shared candidate when no profile is active, otherwise it
    /// receives the sparse profile override that owns the write.
    pub(in crate::shell) fn prepare_control_plane_settings<F>(
        &self,
        mutate: F,
    ) -> Result<ControlPlaneSettingsCandidate, ShellRunError>
    where
        F: FnOnce(
            &mut SettingsStore,
            Option<&mut mesh_core_module::package::ShellProfile>,
        ) -> Result<(), ShellRunError>,
    {
        let shared_path = self.settings_watch.path.clone();
        let mut shared = SettingsStore::load_from(&shared_path)
            .map_err(|error| ShellRunError::Package(format!("failed to load settings: {error}")))?;
        let shared_revision = shared.revision();
        let mut profile_commit = if let Some(profile_id) = self.active_profile_id.clone() {
            let paths = mesh_core_module::package::ProfilePaths::from_root_graph(
                &self.installed_module_graph_path(),
            )
            .map_err(|error| ShellRunError::Package(error.to_string()))?;
            let profile = paths
                .load(&profile_id)
                .map_err(|error| ShellRunError::Package(error.to_string()))?;
            Some((paths, profile_id, profile))
        } else {
            None
        };

        mutate(
            &mut shared,
            profile_commit.as_mut().map(|(_, _, profile)| profile),
        )?;

        let (mut store, profile_commit) = if let Some((paths, profile_id, profile)) = profile_commit
        {
            let store =
                super::super::discovery::effective_profile_settings(shared.clone(), Some(&profile))
                    .map_err(|error| ShellRunError::Package(error.to_string()))?;
            let expected_revision = profile.revision;
            (store, Some((paths, profile_id, profile, expected_revision)))
        } else {
            (shared, None)
        };

        if let Some(graph) = self.installed_module_graph.as_ref() {
            super::super::discovery::register_graph_settings_schemas(&mut store, graph)
                .map_err(|error| ShellRunError::Package(error.to_string()))?;
        }

        Ok(ControlPlaneSettingsCandidate {
            store,
            shared_revision,
            profile_commit,
        })
    }

    /// Commit a prepared candidate through the owning file's revision check.
    /// No live state is changed until the atomic persistence operation returns.
    pub(in crate::shell) fn commit_control_plane_settings(
        &mut self,
        candidate: ControlPlaneSettingsCandidate,
    ) -> Result<ControlPlaneSettingsCommit, ShellRunError> {
        let ControlPlaneSettingsCandidate {
            mut store,
            shared_revision,
            profile_commit,
        } = candidate;
        let shared_path = self.settings_watch.path.clone();
        let shared = SettingsStore::load_from(&shared_path).map_err(|error| {
            ShellRunError::Package(format!("failed to reload settings: {error}"))
        })?;
        shared
            .check_revision(shared_revision)
            .map_err(|error| ShellRunError::Package(error.to_string()))?;

        let revision = if let Some((paths, profile_id, profile, expected_revision)) = profile_commit
        {
            let committed = paths
                .save_if_revision(&profile_id, &profile, expected_revision)
                .map_err(|error| ShellRunError::Package(error.to_string()))?;
            DurableControlPlaneRevision::new(shared_revision, Some(committed.revision))
        } else {
            store
                .save_if_revision(shared_revision)
                .map_err(|error| ShellRunError::Package(error.to_string()))?;
            DurableControlPlaneRevision::new(store.revision(), None)
        };

        self.settings_watch.modified_at = std::fs::metadata(&shared_path)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        Ok(ControlPlaneSettingsCommit { store, revision })
    }

    /// Publish the effective settings snapshot through the ordinary interface
    /// path. Consumers observe one revisioned value and never need the
    /// settings-file path or a raw component namespace injected by the shell.
    pub(in crate::shell) fn sync_settings_service_state(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let namespaces = self.settings_store.to_value();
        let revision = self.control_plane_revision.as_string();
        let payload = serde_json::json!({
            "revision": revision.clone(),
            "durable_revision": revision,
            "namespaces": namespaces,
        });
        self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.settings".into(),
            source_module: self.active_service_provider_or("mesh.settings", "@mesh/shell"),
            payload,
        })
    }

    /// Publish one committed control-plane snapshot in a fixed order. All
    /// three runtime snapshots are installed before component callbacks run;
    /// callbacks and service observers then see settings, theme, and locale in
    /// that order and receive effects in the same batch.
    pub(in crate::shell) fn commit_control_plane_batch(
        &mut self,
        commit: ControlPlaneSettingsCommit,
        theme: Option<(ThemeEngine, ThemeWatchState)>,
        locale: Option<LocaleEngine>,
        theme_effect: bool,
        locale_effect: bool,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        self.settings_store = Arc::new(commit.store);
        self.settings =
            mesh_core_config::resolve_shell_locale_settings(self.settings_store.shell());
        self.control_plane_revision = commit.revision;

        if let Some((theme, watch)) = theme {
            self.install_prepared_theme(PreparedThemeState {
                engine: theme,
                watch,
            });
        }
        if let Some(locale) = locale {
            self.locale = locale;
        }

        mesh_core_icon::set_default_shell_pack(self.settings.icons.default_pack.clone());
        mesh_core_render::set_blur_quality(blur_quality_from_settings(&self.settings.render.blur));

        let mut requests = VecDeque::new();
        // This is intentionally the first effect phase: settings-dependent
        // component state is ready before the appearance and locale signals.
        self.apply_settings_to_components()?;
        requests.extend(self.sync_settings_service_state()?);
        if theme_effect {
            requests.extend(self.mark_components_theme_changed()?);
            requests.extend(self.sync_theme_service_state()?);
        }
        if locale_effect {
            requests.extend(self.mark_components_locale_changed()?);
            requests.extend(self.sync_locale_service_state()?);
        }
        Ok(requests)
    }

    /// Install a fully prepared theme candidate and refresh the managed watch
    /// set at the same in-memory commit boundary. Preparation is the only
    /// fallible CSS/source phase; this operation only swaps owned snapshots.
    pub(in crate::shell) fn install_prepared_theme(&mut self, candidate: PreparedThemeState) {
        let PreparedThemeState { engine, mut watch } = candidate;
        watch.revision = engine.active_snapshot().revision;
        self.theme = engine;
        self.theme_watch = watch;
        self.reconcile_file_watcher();
    }

    /// Commit a theme candidate used by hot reload. The candidate has already
    /// been parsed, composed, and fingerprinted, so a failure in notification
    /// handling retains the exact previous renderer snapshot and watch state.
    fn commit_prepared_theme(
        &mut self,
        candidate: PreparedThemeState,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let previous_theme = self.theme.clone();
        let previous_watch = self.theme_watch.clone();
        self.install_prepared_theme(candidate);
        let mut requests = match self.mark_components_theme_changed() {
            Ok(requests) => requests,
            Err(error) => {
                self.theme = previous_theme;
                self.theme_watch = previous_watch;
                self.reconcile_file_watcher();
                self.record_theme_reload_failure(&error);
                return Ok(VecDeque::new());
            }
        };
        match self.sync_theme_service_state() {
            Ok(effects) => requests.extend(effects),
            Err(error) => {
                self.theme = previous_theme;
                self.theme_watch = previous_watch;
                self.reconcile_file_watcher();
                self.record_theme_reload_failure(&error);
                return Ok(VecDeque::new());
            }
        }
        Ok(requests)
    }

    pub(in crate::shell) fn reload_theme_if_changed(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let now = std::time::Instant::now();
        if now < self.next_theme_reload_check {
            return Ok(VecDeque::new());
        }
        // Keep polling even when inotify is active. Theme selection can move
        // the source into a directory that the startup watcher did not know;
        // the fingerprint makes this poll cheap and content-sensitive.
        self.next_theme_reload_check = now + THEME_RELOAD_POLL_INTERVAL;

        let has_authorized_theme = self.installed_module_graph.as_ref().is_some_and(|graph| {
            graph
                .theme_catalog()
                .get(&self.settings.theme.active)
                .is_some()
        });
        if !has_authorized_theme {
            return Ok(VecDeque::new());
        }

        let fingerprint = match theme_source_fingerprint(self) {
            Ok(fingerprint) => fingerprint,
            Err(error) => {
                self.record_theme_reload_failure(&error);
                return Ok(VecDeque::new());
            }
        };

        let selected_mode = self
            .installed_module_graph
            .as_ref()
            .and_then(|graph| graph.theme_catalog().get(&self.settings.theme.active))
            .map(|descriptor| selected_theme_mode(&self.settings, descriptor))
            .transpose();
        let selected_mode = match selected_mode {
            Ok(mode) => mode,
            Err(error) => {
                self.record_theme_reload_failure(&error);
                return Ok(VecDeque::new());
            }
        };

        let modified_at = std::fs::metadata(&self.theme_watch.path)
            .ok()
            .and_then(|metadata| metadata.modified().ok());

        if self.theme_watch.fingerprint == Some(fingerprint)
            && self.theme_watch.modified_at == modified_at
            && self.theme_watch.mode == selected_mode
        {
            return Ok(VecDeque::new());
        }

        let Some(graph) = self.installed_module_graph.as_ref() else {
            return Ok(VecDeque::new());
        };
        let mut candidate = match prepare_theme_state_for_graph(&self.settings, graph) {
            Ok(Some(candidate)) => candidate,
            Ok(None) => return Ok(VecDeque::new()),
            Err(error) => {
                self.record_theme_reload_failure(&error);
                return Ok(VecDeque::new());
            }
        };
        candidate.engine.update_active(|theme| {
            apply_font_family(theme, self.settings.fonts.ui_family.as_deref());
            crate::shell::discovery::apply_font_registry_tokens(theme, &self.font_registry);
        });
        candidate.watch.revision = candidate.engine.active_snapshot().revision;
        tracing::info!(
            "reloaded active theme '{}' from {}",
            candidate.engine.active().id,
            candidate.watch.path.display()
        );
        self.commit_prepared_theme(candidate)
    }

    fn record_theme_reload_failure(&mut self, error: &dyn std::fmt::Display) {
        self.diagnostics.record_lifecycle_error(
            "@mesh/shell",
            "theme_reload_rejected",
            error.to_string(),
        );
        tracing::warn!("retaining last-known-good theme after reload failure: {error}");
    }

    pub(in crate::shell) fn mark_components_theme_changed(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let snapshot = self.theme.active_snapshot().clone();
        for component_index in 0..self.components.len() {
            if self.component_is_quarantined(component_index) {
                continue;
            }
            match self.components[component_index].component.theme_changed() {
                Ok(()) => self.components[component_index].parent.force_full_present = true,
                Err(error) => {
                    self.contain_component_failure(component_index, "theme", &error);
                }
            }
        }
        // Broadcast event so script-side subscribers can react. Painter-level
        // invalidation is handled by `theme_changed()` above; the event is
        // additive — components that opt in via `handle_core_event` can use it
        // for non-visual derived state (e.g. icon name based on dark mode).
        // Effects the subscribers emit in response are returned to the caller
        // rather than dropped.
        self.broadcast_core_event(CoreEvent::ThemeChanged { snapshot })
    }

    pub(in crate::shell) fn apply_set_theme(
        &mut self,
        theme_id: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let Some(graph) = self
            .installed_module_graph
            .as_ref()
            .filter(|graph| graph.theme_catalog().get(theme_id).is_some())
            .cloned()
        else {
            tracing::warn!(
                "cannot select theme '{theme_id}': it is not a graph-authorized catalog identity"
            );
            return Ok(VecDeque::new());
        };
        let theme_id = theme_id.to_owned();
        let settings_candidate = self.prepare_control_plane_settings(|shared, profile| {
            if let Some(profile) = profile {
                let target = profile
                    .settings
                    .entry(mesh_core_config::SHELL_NAMESPACE.to_string())
                    .or_insert_with(|| serde_json::json!({}));
                set_profile_theme_selection(target, &theme_id);
            } else {
                shared.merge_namespace(
                    mesh_core_config::SHELL_NAMESPACE,
                    &serde_json::json!({ "theme": { "active": theme_id } }),
                );
                shared.unset_namespace_field(mesh_core_config::SHELL_NAMESPACE, "theme", "mode");
            }
            Ok(())
        })?;
        let candidate_settings =
            mesh_core_config::resolve_shell_locale_settings(settings_candidate.store.shell());
        let prepared_theme = match prepare_theme_state_for_graph(&candidate_settings, &graph) {
            Ok(Some(candidate)) => candidate,
            Ok(None) => return Ok(VecDeque::new()),
            Err(error) => {
                tracing::warn!("cannot compose theme '{theme_id}': {error}");
                return Ok(VecDeque::new());
            }
        };
        let mut theme = prepared_theme.engine;
        theme.update_active(|active| {
            crate::shell::discovery::apply_font_registry_tokens(active, &self.font_registry);
        });
        let commit = self.commit_control_plane_settings(settings_candidate)?;
        tracing::info!("active theme changed to '{theme_id}'");
        self.commit_control_plane_batch(
            commit,
            Some((theme, prepared_theme.watch)),
            None,
            true,
            false,
        )
    }

    pub(in crate::shell) fn apply_set_theme_mode(
        &mut self,
        mode: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mode = mode.trim();
        if mode.is_empty() {
            tracing::warn!("ignoring empty theme mode request");
            return Ok(VecDeque::new());
        }
        let Some(graph) = self.installed_module_graph.as_ref().cloned() else {
            tracing::warn!("cannot select theme mode without an installed module graph");
            return Ok(VecDeque::new());
        };
        let settings_candidate = self.prepare_control_plane_settings(|shared, profile| {
            if let Some(profile) = profile {
                let target = profile
                    .settings
                    .entry(mesh_core_config::SHELL_NAMESPACE.to_string())
                    .or_insert_with(|| serde_json::json!({}));
                set_profile_theme_mode(target, mode);
            } else {
                shared.merge_namespace(
                    mesh_core_config::SHELL_NAMESPACE,
                    &serde_json::json!({ "theme": { "mode": mode } }),
                );
            }
            Ok(())
        })?;
        let candidate_settings =
            mesh_core_config::resolve_shell_locale_settings(settings_candidate.store.shell());
        let prepared_theme = match prepare_theme_state_for_graph(&candidate_settings, &graph) {
            Ok(Some(candidate)) => candidate,
            Ok(None) => return Ok(VecDeque::new()),
            Err(error) => {
                tracing::warn!("cannot select theme mode '{mode}': {error}");
                return Ok(VecDeque::new());
            }
        };
        let mut theme = prepared_theme.engine;
        theme.update_active(|active| {
            crate::shell::discovery::apply_font_registry_tokens(active, &self.font_registry);
        });
        let commit = self.commit_control_plane_settings(settings_candidate)?;
        tracing::info!("active theme mode changed to '{mode}'");
        self.commit_control_plane_batch(
            commit,
            Some((theme, prepared_theme.watch)),
            None,
            true,
            false,
        )
    }

    pub(in crate::shell) fn apply_set_icon_theme(
        &mut self,
        theme_id: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let theme_id = theme_id.trim();
        let available_host_theme = mesh_core_resources::system_resource_catalog()
            .icon_themes
            .iter()
            .any(|theme| theme.id == theme_id && !theme.hidden);
        let available_resource_pack = self
            .resource_explanation_snapshot()
            .icons
            .available
            .iter()
            .any(|candidate| candidate == theme_id);
        let available = available_host_theme || available_resource_pack;
        if !available {
            tracing::warn!("cannot select unavailable icon resource '{theme_id}'");
            return Ok(VecDeque::new());
        }
        let theme_id = theme_id.to_owned();
        let candidate = self.prepare_control_plane_settings(|shared, profile| {
            let patch = serde_json::json!({ "icons": { "default_pack": theme_id } });
            if let Some(profile) = profile {
                let target = profile
                    .settings
                    .entry(mesh_core_config::SHELL_NAMESPACE.to_string())
                    .or_insert_with(|| serde_json::json!({}));
                mesh_core_config::merge_json(target, &patch);
            } else {
                shared.merge_namespace(mesh_core_config::SHELL_NAMESPACE, &patch);
            }
            Ok(())
        })?;
        let commit = self.commit_control_plane_settings(candidate)?;
        self.commit_control_plane_batch(commit, None, None, true, false)
    }

    pub(in crate::shell) fn apply_set_font_family(
        &mut self,
        family: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let family = family.trim();
        let available = mesh_core_resources::system_resource_catalog()
            .font_families
            .iter()
            .any(|candidate| candidate.name == family);
        if !available {
            tracing::warn!("cannot select unavailable system font family '{family}'");
            return Ok(VecDeque::new());
        }
        let family = family.to_owned();
        let candidate = self.prepare_control_plane_settings(|shared, profile| {
            let patch = serde_json::json!({ "fonts": { "ui_family": family } });
            if let Some(profile) = profile {
                let target = profile
                    .settings
                    .entry(mesh_core_config::SHELL_NAMESPACE.to_string())
                    .or_insert_with(|| serde_json::json!({}));
                mesh_core_config::merge_json(target, &patch);
            } else {
                shared.merge_namespace(mesh_core_config::SHELL_NAMESPACE, &patch);
            }
            Ok(())
        })?;
        let mut theme = self.theme.clone();
        theme.update_active(|active| apply_font_family(active, Some(&family)));
        let mut watch = self.theme_watch.clone();
        watch.revision = theme.active_snapshot().revision;
        let commit = self.commit_control_plane_settings(candidate)?;
        self.commit_control_plane_batch(commit, Some((theme, watch)), None, true, false)
    }

    /// Build the only service state allowed to describe the rendered theme.
    /// Provider mirrors can carry no independent theme identity, mode, token,
    /// or color-scheme facts; those all come from the committed snapshot.
    pub(in crate::shell) fn authoritative_theme_service_payload(&self) -> serde_json::Value {
        let snapshot = self.theme.active_snapshot();
        let theme_modes = |theme: &mesh_core_theme::Theme| {
            self.installed_module_graph
                .as_ref()
                .and_then(|graph| graph.theme_catalog().get(&theme.id))
                .map(|descriptor| {
                    descriptor
                        .modes
                        .values()
                        .map(|mode| {
                            serde_json::json!({
                                "name": mode.name,
                                "color_scheme": mode.metadata.color_scheme,
                                "contrast": mode.metadata.contrast,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| {
                    vec![serde_json::json!({
                        "name": theme.metadata().mode,
                        "color_scheme": theme.metadata().color_scheme,
                        "contrast": theme.metadata().contrast,
                    })]
                })
        };
        let theme_entry = |theme: &mesh_core_theme::Theme| {
            serde_json::json!({
                "id": theme.id,
                "label": theme.name,
                "modes": theme_modes(theme),
                "palette": theme_preview_palette(theme),
            })
        };
        let mut themes = self
            .theme
            .available_themes()
            .iter()
            .map(theme_entry)
            .collect::<Vec<_>>();
        if !themes
            .iter()
            .any(|theme| theme["id"].as_str() == Some(snapshot.id.as_str()))
        {
            themes.push(theme_entry(self.theme.active()));
        }
        themes.sort_by(|left, right| {
            left["id"]
                .as_str()
                .unwrap_or_default()
                .cmp(right["id"].as_str().unwrap_or_default())
        });
        let available = themes
            .iter()
            .filter_map(|theme| theme["id"].as_str())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        serde_json::json!({
            "current": snapshot.id.clone(),
            "theme_id": snapshot.id.clone(),
            "mode": snapshot.mode.clone(),
            "mode_policy": self.settings.theme.mode_policy.clone(),
            "modes": theme_modes(self.theme.active()),
            "color_scheme": snapshot.color_scheme.clone(),
            "contrast": snapshot.contrast.clone(),
            "tokens": snapshot.tokens.clone(),
            "provenance": snapshot.provenance.clone(),
            "revision": format!("{:016x}", snapshot.revision),
            "durable_revision": self.control_plane_revision.as_string(),
            "fingerprint": self.theme_watch.fingerprint,
            "is_dark": snapshot.color_scheme.eq_ignore_ascii_case("dark"),
            "themes": themes,
            "available": available,
            "system_resources": system_resources_json(&self.settings),
        })
    }

    pub(in crate::shell) fn sync_theme_service_state(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let snapshot = self.theme.active_snapshot().clone();
        let previous_snapshot = self.last_published_theme_snapshot.clone();
        let payload = self.authoritative_theme_service_payload();
        let modes = payload["modes"].clone();
        let durable_revision = self.control_plane_revision.as_string();
        // The renderer owns this snapshot; a provider mirror is delivered
        // through the normal `mesh.theme` state channel below, not an
        // internal `"set-current"` command the contract never declared. A
        // module implementing `mesh.theme` would otherwise receive a
        // command outside its own contract's method set.
        let source_module = self.active_service_provider_or(
            "mesh.theme",
            self.core_service_providers
                .provider_id("mesh.theme")
                .unwrap_or("@mesh/shell"),
        );
        if let Err(error) = mesh_core_backend::validate_command_payload(&payload) {
            tracing::warn!("theme service snapshot exceeded backend command JSON budget: {error}");
        }
        let mut requests = self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.theme".into(),
            source_module: source_module.into(),
            payload,
        })?;

        if let Some(previous_snapshot) = previous_snapshot.as_ref()
            && previous_snapshot != &snapshot
        {
            let changed_token_names = snapshot.changed_token_names(previous_snapshot);
            let changed_tokens = changed_token_names
                .iter()
                .map(|name| {
                    serde_json::json!({
                        "name": name,
                        "value": snapshot.tokens.get(name),
                        "provenance": snapshot.provenance.get(name),
                    })
                })
                .collect::<Vec<_>>();
            let revision = format!("{:016x}", snapshot.revision);
            requests.extend(self.broadcast_shell_interface_event(
                "mesh.theme",
                "ThemeChanged",
                serde_json::json!({
                    "theme_id": snapshot.id.clone(),
                    "mode": snapshot.mode.clone(),
                    "mode_policy": self.settings.theme.mode_policy.clone(),
                    "modes": modes,
                    "color_scheme": snapshot.color_scheme.clone(),
                    "contrast": snapshot.contrast.clone(),
                    "revision": revision.clone(),
                    "durable_revision": durable_revision.clone(),
                    "tokens": snapshot.tokens.clone(),
                    "provenance": snapshot.provenance.clone(),
                    "changed_tokens": changed_tokens,
                }),
            )?);
            if self.has_interface_event_observers("mesh.theme", "TokenChanged") {
                for name in changed_token_names {
                    requests.extend(self.broadcast_shell_interface_event(
                        "mesh.theme",
                        "TokenChanged",
                        serde_json::json!({
                            "theme_id": snapshot.id.clone(),
                            "mode": snapshot.mode.clone(),
                            "name": name,
                            "value": snapshot.tokens.get(&name),
                            "provenance": snapshot.provenance.get(&name),
                            "revision": revision.clone(),
                            "durable_revision": durable_revision.clone(),
                        }),
                    )?);
                }
            }
        }
        self.last_published_theme_snapshot = Some(snapshot);
        Ok(requests)
    }

    pub(in crate::shell) fn reload_locale_if_settings_changed(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let requests = VecDeque::new();
        let now = std::time::Instant::now();
        if now < self.next_shell_settings_reload_check {
            return Ok(requests);
        }
        // Keep the fallback bounded even while the managed watcher is healthy;
        // the watcher only reduces latency and must not become a blind spot.
        self.next_shell_settings_reload_check = now + SHELL_SETTINGS_RELOAD_POLL_INTERVAL;

        let Ok(metadata) = std::fs::metadata(&self.settings_watch.path) else {
            return Ok(requests);
        };
        let Ok(modified_at) = metadata.modified() else {
            return Ok(requests);
        };

        if self.settings_watch.modified_at == Some(modified_at) {
            return Ok(requests);
        }

        let (store, profile_revision) = match SettingsStore::load_from(&self.settings_watch.path)
            .and_then(|shared| {
                let profile = self.active_profile_id.as_ref().and_then(|profile_id| {
                    mesh_core_module::package::ProfilePaths::from_root_graph(
                        &self.installed_module_graph_path(),
                    )
                    .and_then(|paths| paths.load(profile_id))
                    .ok()
                });
                let profile_revision = profile.as_ref().map(|profile| profile.revision);
                super::super::discovery::effective_profile_settings(shared, profile.as_ref())
                    .map(|store| (store, profile_revision))
            }) {
            Ok((mut store, profile_revision)) => {
                if let Some(graph) = self.installed_module_graph.as_ref()
                    && let Err(error) =
                        super::super::discovery::register_graph_settings_schemas(&mut store, graph)
                {
                    tracing::warn!(
                        "failed to register settings schemas during reload; retaining previous snapshot: {error}"
                    );
                    return Ok(requests);
                }
                (store, profile_revision)
            }
            Err(e) => {
                tracing::warn!("failed to reload settings: {e}");
                return Ok(requests);
            }
        };
        mesh_core_config::log_settings_diagnostics(
            "settings reload",
            &mesh_core_config::new_settings_diagnostics(
                self.settings_store.diagnostics(),
                store.diagnostics(),
            ),
        );
        let new_settings = mesh_core_config::resolve_shell_locale_settings(store.shell());

        let old_theme = self.settings.theme.clone();
        let old_i18n = self.settings.i18n.clone();
        let old_icons = self.settings.icons.clone();
        let old_fonts = self.settings.fonts.clone();
        let new_i18n = &new_settings.i18n;
        let locale_changed = old_i18n.locale != new_i18n.locale
            || old_i18n.fallback_locale != new_i18n.fallback_locale
            || old_i18n.policy != new_i18n.policy;

        let prepared_locale = if locale_changed {
            // A settings-only locale selection does not change the installed
            // catalog graph. Reuse its immutable snapshot; graph/profile
            // activation remains responsible for preparing a replacement.
            let result = self.prepare_locale_selection_for_settings(&new_settings);
            match result {
                Ok(candidate) => Some(candidate),
                Err(error) => {
                    tracing::warn!(
                        "locale settings reload rejected; retaining prior snapshot: {error}"
                    );
                    self.diagnostics.record_lifecycle_error(
                        "@mesh/shell",
                        "locale_reload_rejected",
                        error.to_string(),
                    );
                    return Ok(requests);
                }
            }
        } else {
            None
        };

        let theme_changed = old_theme != new_settings.theme || old_fonts != new_settings.fonts;
        let prepared_theme = if theme_changed {
            let Some(graph) = self.installed_module_graph.as_ref().filter(|graph| {
                graph
                    .theme_catalog()
                    .get(&new_settings.theme.active)
                    .is_some()
            }) else {
                self.record_theme_reload_failure(&mesh_core_theme::ThemeError::NotFound(
                    new_settings.theme.active.clone(),
                ));
                return Ok(requests);
            };
            let (theme, theme_watch) = match prepare_theme_state_for_graph(&new_settings, graph) {
                Ok(Some(candidate)) => (candidate.engine, candidate.watch),
                Ok(None) => {
                    self.record_theme_reload_failure(&mesh_core_theme::ThemeError::NotFound(
                        new_settings.theme.active.clone(),
                    ));
                    return Ok(requests);
                }
                Err(error) => {
                    self.record_theme_reload_failure(&error);
                    return Ok(requests);
                }
            };
            let mut theme = theme;
            theme.update_active(|active| {
                crate::shell::discovery::apply_font_registry_tokens(active, &self.font_registry);
            });
            let active_theme_id = theme.active().id.clone();
            tracing::info!(
                "active theme changed: {} -> {}",
                old_theme.active,
                active_theme_id
            );
            Some((theme, theme_watch))
        } else {
            None
        };
        let icon_changed = old_icons != new_settings.icons;

        if locale_changed {
            tracing::info!(
                "locale changed: {} (fallback: {}) -> {} (fallback: {})",
                old_i18n.locale,
                old_i18n.fallback_locale,
                new_i18n.locale,
                new_i18n.fallback_locale,
            );
            // The locale candidate is committed together with settings below.
        }

        let shared_revision = store.revision();
        let commit = ControlPlaneSettingsCommit {
            store,
            revision: DurableControlPlaneRevision::new(shared_revision, profile_revision),
        };
        let result = self.commit_control_plane_batch(
            commit,
            prepared_theme,
            prepared_locale,
            theme_changed || icon_changed,
            locale_changed,
        );
        if result.is_ok() {
            // Advance the watch only after the complete effective settings,
            // theme, and locale candidate has committed. A rejected or
            // partially prepared file must remain eligible for retry without
            // replacing the last-known-good runtime snapshot.
            self.settings_watch.modified_at = Some(modified_at);
        }
        result
    }

    /// Hand the reloaded store to every component.
    ///
    /// One file holds every namespace, so a single reload here serves every
    /// component: they all adopt the snapshot the shell just read.
    pub(in crate::shell) fn apply_settings_to_components(&mut self) -> Result<(), ShellRunError> {
        let store = self.settings_store.clone();
        let mut role_changes = Vec::new();
        for component_index in 0..self.components.len() {
            if self.component_is_quarantined(component_index) {
                continue;
            }
            let result = self.components[component_index]
                .component
                .apply_settings(&store);
            let changed = match result {
                Ok(changed) => changed,
                Err(error) => {
                    self.contain_component_failure(component_index, "settings", &error);
                    continue;
                }
            };
            if changed {
                tracing::info!(
                    "settings changed for component '{}'",
                    self.components[component_index].component.id()
                );
            }
            if let Some(role) = self.components[component_index]
                .component
                .pending_surface_role_change()
            {
                role_changes.push((self.components[component_index].surface_id.clone(), role));
            }
        }
        for (surface_id, role) in role_changes {
            // Settings-selected roles use the same transactional supervisor as
            // explicit promotion so child surfaces, focus, and cached
            // compositor state are torn down together.
            self.set_surface_role(surface_id, role)?;
        }
        Ok(())
    }

    pub(in crate::shell) fn mark_components_locale_changed(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let locale = self.locale.clone();
        let locale_id = locale.current().to_string();
        for component_index in 0..self.components.len() {
            if self.component_is_quarantined(component_index) {
                continue;
            }
            match self.components[component_index]
                .component
                .locale_changed(&locale)
            {
                Ok(()) => self.components[component_index].parent.force_full_present = true,
                Err(error) => {
                    self.contain_component_failure(component_index, "locale", &error);
                }
            }
        }
        self.broadcast_core_event(CoreEvent::LocaleChanged { locale: locale_id })
    }

    pub(in crate::shell) fn apply_set_locale(
        &mut self,
        locale: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let locale = locale.trim();
        if locale.is_empty() {
            tracing::warn!("ignoring empty locale request");
            return Ok(VecDeque::new());
        }
        let candidate = match self.prepare_and_persist_locale_change(locale) {
            Ok(candidate) => candidate,
            Err(error) => {
                tracing::warn!(%error, "locale change transaction rejected");
                self.diagnostics.record_lifecycle_error(
                    "@mesh/shell",
                    "locale_change_rejected",
                    error.to_string(),
                );
                return self.sync_locale_service_state();
            }
        };
        let Some((candidate, locale)) = candidate else {
            return self.sync_locale_service_state();
        };

        tracing::info!("active locale changed to '{}'", locale.current());
        let commit = self.commit_control_plane_settings(candidate)?;
        self.commit_control_plane_batch(commit, None, Some(locale), false, true)
    }

    /// Prepare the normalized locale and complete catalog before committing a
    /// durable shared-settings or active-profile write. The runtime remains
    /// unchanged when validation, catalog preparation, or revision checking
    /// rejects the candidate.
    fn prepare_and_persist_locale_change(
        &self,
        requested_locale: &str,
    ) -> Result<Option<(ControlPlaneSettingsCandidate, LocaleEngine)>, ShellRunError> {
        let normalized = mesh_core_locale::LocaleSelection::try_new(
            requested_locale,
            self.settings.i18n.fallback_locale.clone(),
            0,
        )
        .map_err(|error| ShellRunError::LocaleCatalog(error.to_string()))?;
        if normalized.active() == self.locale.current()
            && normalized.fallback() == self.locale.fallback_locale()
            && self.settings.i18n.policy == mesh_core_config::LocalePolicy::Manual
        {
            return Ok(None);
        }

        let active = normalized.active().to_owned();
        let fallback = normalized.fallback().to_owned();
        let candidate = self.prepare_control_plane_settings(|shared, profile| {
            let patch = serde_json::json!({
                "i18n": {
                    "policy": "manual",
                    "locale": active,
                    "fallback_locale": fallback,
                }
            });
            if let Some(profile) = profile {
                let target = profile
                    .settings
                    .entry(mesh_core_config::SHELL_NAMESPACE.to_string())
                    .or_insert_with(|| serde_json::json!({}));
                mesh_core_config::merge_json(target, &patch);
            } else {
                shared.merge_namespace(mesh_core_config::SHELL_NAMESPACE, &patch);
            }
            Ok(())
        })?;
        // The active graph and its immutable catalog snapshot are unchanged by
        // this selection write, so only the normalized selection is prepared.
        let locale = self.prepare_locale_selection_for_settings(candidate.store.shell())?;
        Ok(Some((candidate, locale)))
    }

    pub(in crate::shell) fn sync_locale_service_state(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let selection = self.locale.selection();
        let locale = selection.active().to_string();
        let chain = selection.chain().to_vec();
        let direction = selection.direction().as_str();
        let policy = self.settings.i18n.policy.as_str();
        let revision = selection.revision().to_string();
        let durable_revision = self.control_plane_revision.as_string();
        // As with theme, the shell supplies the host-derived snapshot while
        // the selected provider owns the interface state observed by modules.
        let source_module = self.active_service_provider_or("mesh.locale", "@mesh/shell");
        self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module,
            payload: serde_json::json!({
                "locale": locale.clone(),
                "current": locale,
                "chain": chain,
                "direction": direction,
                "policy": policy,
                "revision": revision,
                "durable_revision": durable_revision,
            }),
        })
    }
}
