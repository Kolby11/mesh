use super::super::*;
use std::hash::{Hash, Hasher};

const THEME_RELOAD_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
const SHELL_SETTINGS_RELOAD_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(500);

fn is_dark_theme_id(theme_id: &str) -> bool {
    theme_id.contains("dark") || theme_id.eq_ignore_ascii_case("tokyo-night")
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
    fn persist_shell_appearance_override(&mut self, patch: serde_json::Value) {
        // Keep the live effective store in sync immediately, then persist the
        // same sparse override to the shared store. Profile-owned overrides
        // remain a separate composition concern; this is the global picker.
        let mut effective = self.settings_store.as_ref().clone();
        effective.merge_namespace(mesh_core_config::SHELL_NAMESPACE, &patch);
        self.settings_store = Arc::new(effective);

        let path = self.settings_watch.path.clone();
        let result = SettingsStore::load_from(&path).and_then(|mut shared| {
            shared.merge_namespace(mesh_core_config::SHELL_NAMESPACE, &patch);
            shared.save()
        });
        match result {
            Ok(()) => {
                self.settings_watch.modified_at = std::fs::metadata(&path)
                    .ok()
                    .and_then(|metadata| metadata.modified().ok());
            }
            Err(error) => tracing::warn!(
                "failed to persist shell appearance setting to {}: {error}",
                path.display()
            ),
        }
    }

    /// Publish the effective settings snapshot through the ordinary interface
    /// path. Consumers observe one revisioned value and never need the
    /// settings-file path or a raw component namespace injected by the shell.
    pub(in crate::shell) fn sync_settings_service_state(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let namespaces = self.settings_store.to_value();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        namespaces.to_string().hash(&mut hasher);
        let payload = serde_json::json!({
            "revision": format!("{:016x}", hasher.finish()),
            "namespaces": namespaces,
        });
        self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.settings".into(),
            source_module: self.active_service_provider_or("mesh.settings", "@mesh/shell"),
            payload,
        })
    }

    pub(in crate::shell) fn reload_theme_if_changed(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let now = std::time::Instant::now();
        if now < self.next_theme_reload_check {
            return Ok(VecDeque::new());
        }
        self.next_theme_reload_check = now
            + if self.file_watcher_active {
                super::FILE_WATCHER_RELOAD_PARK
            } else {
                THEME_RELOAD_POLL_INTERVAL
            };

        let Ok(metadata) = std::fs::metadata(&self.theme_watch.path) else {
            return Ok(VecDeque::new());
        };
        let Ok(modified_at) = metadata.modified() else {
            return Ok(VecDeque::new());
        };

        if self.theme_watch.modified_at == Some(modified_at) {
            return Ok(VecDeque::new());
        }

        let old_theme_id = self.theme.active().id.clone();
        let mut theme = if let Some(graph) = self.installed_module_graph.as_ref().filter(|graph| {
            graph
                .theme_catalog()
                .get(&self.settings.theme.active)
                .is_some()
        }) {
            match prepare_theme_for_graph(&self.settings, graph) {
                Ok((theme, _watch)) => theme,
                Err(error) => {
                    self.record_theme_reload_failure(&error);
                    return Ok(VecDeque::new());
                }
            }
        } else {
            match mesh_core_theme::load_theme_from_path(&self.theme_watch.path) {
                Ok(theme) => theme,
                Err(error) => {
                    self.record_theme_reload_failure(&error);
                    return Ok(VecDeque::new());
                }
            }
        };
        apply_font_family(&mut theme, self.settings.fonts.ui_family.as_deref());
        tracing::info!(
            "reloaded active theme '{}' from {}",
            theme.id,
            self.theme_watch.path.display()
        );
        let previous_theme = self.theme.active().clone();
        let previous_watch = self.theme_watch.clone();
        self.theme.replace_active(theme);
        self.theme_watch.modified_at = Some(modified_at);
        if let Err(error) = self.mark_components_theme_changed() {
            self.theme.replace_active(previous_theme);
            self.theme_watch = previous_watch;
            self.record_theme_reload_failure(&error);
            return Ok(VecDeque::new());
        }
        let new_theme_id = self.theme.active().id.clone();
        if new_theme_id != old_theme_id {
            return self.sync_theme_service_state(&new_theme_id);
        }
        Ok(VecDeque::new())
    }

    fn record_theme_reload_failure(&mut self, error: &dyn std::fmt::Display) {
        self.diagnostics.record_lifecycle_error(
            "@mesh/shell",
            "theme_reload_rejected",
            error.to_string(),
        );
        tracing::warn!("retaining last-known-good theme after reload failure: {error}");
    }

    pub(in crate::shell) fn mark_components_theme_changed(&mut self) -> Result<(), ShellRunError> {
        let theme_id = self.theme.active().id.clone();
        let is_dark = is_dark_theme_id(&theme_id);
        for runtime in &mut self.components {
            runtime
                .component
                .theme_changed()
                .map_err(ShellRunError::Component)?;
            runtime.parent.force_full_present = true;
        }
        // Broadcast event so script-side subscribers can react. Painter-level
        // invalidation is handled by `theme_changed()` above; the event is
        // additive — components that opt in via `handle_core_event` can use it
        // for non-visual derived state (e.g. icon name based on dark mode).
        let _ = self.broadcast_core_event(CoreEvent::ThemeChanged { theme_id, is_dark })?;
        Ok(())
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
        let mut candidate_settings = self.settings.clone();
        candidate_settings.theme.active = theme_id.to_string();
        // A mode is scoped to its pack; switching packs starts at the
        // descriptor's validated default instead of reusing a stale mode.
        candidate_settings.theme.mode = None;
        let (theme, watch) = match prepare_theme_for_graph(&candidate_settings, &graph) {
            Ok(candidate) => candidate,
            Err(error) => {
                tracing::warn!("cannot compose theme '{theme_id}': {error}");
                return Ok(VecDeque::new());
            }
        };
        self.theme.replace_active(theme);
        self.theme_watch = watch;
        tracing::info!("active theme changed to '{theme_id}'");
        self.settings.theme.active = theme_id.to_string();
        self.mark_components_theme_changed()?;
        self.sync_theme_service_state(theme_id)
    }

    pub(in crate::shell) fn apply_set_icon_theme(
        &mut self,
        theme_id: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let theme_id = theme_id.trim();
        let available = mesh_core_resources::system_resource_catalog()
            .icon_themes
            .iter()
            .any(|theme| theme.id == theme_id && !theme.hidden);
        if !available {
            tracing::warn!("cannot select unavailable system icon theme '{theme_id}'");
            return Ok(VecDeque::new());
        }
        self.settings.icons.default_pack = Some(theme_id.to_owned());
        self.persist_shell_appearance_override(serde_json::json!({
            "icons": { "default_pack": theme_id }
        }));
        mesh_core_icon::set_default_shell_pack(Some(theme_id.to_owned()));
        self.mark_components_theme_changed()?;
        let active_theme_id = self.theme.active().id.clone();
        let mut requests = self.sync_theme_service_state(&active_theme_id)?;
        requests.extend(self.sync_settings_service_state()?);
        Ok(requests)
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
        self.settings.fonts.ui_family = Some(family.to_owned());
        self.persist_shell_appearance_override(serde_json::json!({
            "fonts": { "ui_family": family }
        }));
        apply_font_family(self.theme.active_mut(), Some(family));
        self.mark_components_theme_changed()?;
        let active_theme_id = self.theme.active().id.clone();
        let mut requests = self.sync_theme_service_state(&active_theme_id)?;
        requests.extend(self.sync_settings_service_state()?);
        Ok(requests)
    }

    pub(in crate::shell) fn sync_theme_service_state(
        &mut self,
        theme_id: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let is_dark = is_dark_theme_id(theme_id);
        let mut themes = self
            .theme
            .available_themes()
            .iter()
            .map(|theme| {
                serde_json::json!({
                    "id": theme.id,
                    "label": theme.name,
                    "palette": theme_preview_palette(theme),
                })
            })
            .collect::<Vec<_>>();
        if !themes
            .iter()
            .any(|theme| theme["id"].as_str() == Some(theme_id))
        {
            themes.push(serde_json::json!({
                "id": self.theme.active().id,
                "label": self.theme.active().name,
                "palette": theme_preview_palette(self.theme.active()),
            }));
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
        let payload = serde_json::json!({
            "current": theme_id,
            "theme_id": theme_id,
            "is_dark": is_dark,
            "themes": themes,
            "available": available,
            "system_resources": system_resources_json(&self.settings),
        });
        // The shell derives the system theme snapshot, but the selected
        // provider owns the interface state. Publishing under that provider
        // keeps the optimistic update on the same generic routing path as
        // the provider's later confirmation.
        let source_module = self.active_service_provider_or("mesh.theme", "@mesh/shell");
        if let Some(tx) = self.service_handlers.get("mesh.theme") {
            let _ = tx.send(ServiceCommandMsg {
                call_id: mesh_core_backend::CallId::next(),
                command: "set-current".to_string(),
                payload: payload.clone(),
                coalesce: true,
            });
        }
        self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.theme".into(),
            source_module,
            payload,
        })
    }

    pub(in crate::shell) fn reload_locale_if_settings_changed(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        let now = std::time::Instant::now();
        if now < self.next_shell_settings_reload_check {
            return Ok(requests);
        }
        self.next_shell_settings_reload_check = now
            + if self.file_watcher_active {
                super::FILE_WATCHER_RELOAD_PARK
            } else {
                SHELL_SETTINGS_RELOAD_POLL_INTERVAL
            };

        let Ok(metadata) = std::fs::metadata(&self.settings_watch.path) else {
            return Ok(requests);
        };
        let Ok(modified_at) = metadata.modified() else {
            return Ok(requests);
        };

        if self.settings_watch.modified_at == Some(modified_at) {
            return Ok(requests);
        }

        self.settings_watch.modified_at = Some(modified_at);

        let store = match SettingsStore::load().and_then(|shared| {
            let profile = self.active_profile_id.as_ref().and_then(|profile_id| {
                mesh_core_module::package::ProfilePaths::from_root_graph(
                    &self.installed_module_graph_path(),
                )
                .and_then(|paths| paths.load(profile_id))
                .ok()
            });
            super::super::discovery::effective_profile_settings(shared, profile.as_ref())
        }) {
            Ok(mut store) => {
                if let Some(graph) = self.installed_module_graph.as_ref()
                    && let Err(error) =
                        super::super::discovery::register_graph_settings_schemas(&mut store, graph)
                {
                    tracing::warn!(
                        "failed to register settings schemas during reload; retaining previous snapshot: {error}"
                    );
                    return Ok(requests);
                }
                Arc::new(store)
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
        let new_settings = store.shell().clone();

        let old_theme = self.settings.theme.clone();
        let old_i18n = self.settings.i18n.clone();
        let old_icons = self.settings.icons.clone();
        let old_fonts = self.settings.fonts.clone();
        let new_i18n = &new_settings.i18n;
        let locale_changed = old_i18n.locale != new_i18n.locale
            || old_i18n.fallback_locale != new_i18n.fallback_locale;

        let theme_changed = old_theme != new_settings.theme || old_fonts != new_settings.fonts;
        if theme_changed {
            let (theme, theme_watch) = if let Some(graph) =
                self.installed_module_graph.as_ref().filter(|graph| {
                    graph
                        .theme_catalog()
                        .get(&new_settings.theme.active)
                        .is_some()
                }) {
                match prepare_theme_for_graph(&new_settings, graph) {
                    Ok(candidate) => candidate,
                    Err(error) => {
                        self.record_theme_reload_failure(&error);
                        return Ok(requests);
                    }
                }
            } else {
                load_active_theme(&new_settings)
            };
            let active_theme_id = theme.active().id.clone();
            tracing::info!(
                "active theme changed: {} -> {}",
                old_theme.active,
                active_theme_id
            );
            self.theme = theme;
            self.theme_watch = theme_watch;
            self.mark_components_theme_changed()?;
            requests.extend(self.sync_theme_service_state(&active_theme_id)?);
        }

        if old_icons != new_settings.icons {
            mesh_core_icon::set_default_shell_pack(new_settings.icons.default_pack.clone());
            if !theme_changed {
                self.mark_components_theme_changed()?;
            }
        }

        if locale_changed {
            tracing::info!(
                "locale changed: {} (fallback: {}) -> {} (fallback: {})",
                old_i18n.locale,
                old_i18n.fallback_locale,
                new_i18n.locale,
                new_i18n.fallback_locale,
            );
            self.locale = LocaleEngine::with_fallback_locale(
                new_i18n.locale.clone(),
                new_i18n.fallback_locale.clone(),
            );
            self.mark_components_locale_changed()?;
            requests.extend(self.sync_locale_service_state()?);
        }

        mesh_core_render::set_blur_quality(blur_quality_from_settings(&new_settings.render.blur));
        self.settings = new_settings;
        self.settings_store = store;
        self.apply_settings_to_components()?;
        requests.extend(self.sync_settings_service_state()?);

        Ok(requests)
    }

    /// Hand the reloaded store to every component.
    ///
    /// One file holds every namespace, so a single reload here serves every
    /// component: they all adopt the snapshot the shell just read.
    pub(in crate::shell) fn apply_settings_to_components(&mut self) -> Result<(), ShellRunError> {
        let store = self.settings_store.clone();
        for runtime in &mut self.components {
            let changed = runtime
                .component
                .apply_settings(&store)
                .map_err(ShellRunError::Component)?;
            if changed {
                tracing::info!(
                    "settings changed for component '{}'",
                    runtime.component.id()
                );
            }
        }
        Ok(())
    }

    pub(in crate::shell) fn mark_components_locale_changed(&mut self) -> Result<(), ShellRunError> {
        let locale = self.locale.clone();
        let locale_id = locale.current().to_string();
        for runtime in &mut self.components {
            runtime
                .component
                .locale_changed(&locale)
                .map_err(ShellRunError::Component)?;
            runtime.parent.force_full_present = true;
        }
        let _ = self.broadcast_core_event(CoreEvent::LocaleChanged { locale: locale_id })?;
        Ok(())
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
        if self.locale.current() == locale {
            return self.sync_locale_service_state();
        }
        tracing::info!("active locale changed to '{locale}'");
        self.locale.set_locale(locale);
        self.mark_components_locale_changed()?;
        self.sync_locale_service_state()
    }

    pub(in crate::shell) fn sync_locale_service_state(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let locale = self.locale.current().to_string();
        // As with theme, the shell supplies the host-derived snapshot while
        // the selected provider owns the interface state observed by modules.
        let source_module = self.active_service_provider_or("mesh.locale", "@mesh/shell");
        self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module,
            payload: serde_json::json!({
                "locale": locale.clone(),
                "current": locale
            }),
        })
    }
}
