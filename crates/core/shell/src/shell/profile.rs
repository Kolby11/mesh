use super::backend::{BackendRuntimeStatus, backend_launch_candidates_from_graph};
use super::component::{FrontendCatalog, FrontendCatalogHandle, FrontendSurfaceComponent};
use super::*;
use mesh_core_module::package::{
    InstalledModuleGraph, ProfilePaths, load_installed_module_graph_for_profile,
};
use std::collections::{HashMap, HashSet, VecDeque};

pub(super) struct PreparedProfileFrontend {
    component: FrontendSurfaceComponent,
    requests: VecDeque<CoreRequest>,
}

#[cfg(test)]
mod tests {
    use super::super::discovery::effective_profile_settings;
    use mesh_core_config::SettingsStore;
    use mesh_core_module::package::ShellProfile;

    #[test]
    fn profile_preferences_layer_over_shared_settings_without_mutating_them() {
        let shared = SettingsStore::from_value(
            "/tmp/shared-settings.json",
            serde_json::json!({
                "shell": { "theme": { "active": "mesh-default-light" } },
                "@test/panel": { "props": { "global": { "density": "comfortable" } } }
            }),
        )
        .unwrap();
        let profile = ShellProfile::from_json_str(
            r#"{
                "schemaVersion": 1,
                "roots": {},
                "settings": {
                    "shell": { "theme": { "active": "mesh-default-dark" } },
                    "@test/panel": { "props": { "global": { "density": "compact" } } }
                }
            }"#,
        )
        .unwrap();

        let effective = effective_profile_settings(shared.clone(), Some(&profile)).unwrap();
        assert_eq!(effective.shell().theme.active, "mesh-default-dark");
        assert_eq!(
            effective.namespace("@test/panel")["props"]["global"]["density"],
            "compact"
        );
        assert_eq!(shared.shell().theme.active, "mesh-default-light");
    }

    #[test]
    fn instance_surface_override_is_scoped_to_the_profile_root_key() {
        let shared = SettingsStore::from_value(
            "/tmp/shared-settings.json",
            serde_json::json!({ "@test/panel": { "surface": { "anchor": "top" } } }),
        )
        .unwrap();
        let profile = ShellProfile::from_json_str(
            r#"{
                "schemaVersion": 1,
                "roots": {
                    "@test/panel#bottom": {
                        "module": "@test/panel",
                        "surface": { "anchor": "bottom" }
                    }
                }
            }"#,
        )
        .unwrap();

        let effective = effective_profile_settings(shared, Some(&profile)).unwrap();
        assert_eq!(
            effective.namespace("@test/panel#bottom")["surface"]["anchor"],
            "bottom"
        );
        assert_eq!(
            effective.namespace("@test/panel#other")["surface"]["anchor"],
            "top"
        );
    }
}

pub(super) struct PendingProfileSwitch {
    profile_id: String,
    graph: InstalledModuleGraph,
    settings: Arc<SettingsStore>,
    catalog: FrontendCatalog,
    desired_surfaces: HashSet<String>,
    desired_providers: HashMap<String, String>,
    prepared_frontends: Vec<PreparedProfileFrontend>,
    candidate_backends: HashMap<String, BackendRuntimeSlot>,
    waiting_backends: HashSet<String>,
}

impl Shell {
    pub(in crate::shell) fn apply_switch_profile(
        &mut self,
        profile_id: &str,
    ) -> VecDeque<CoreRequest> {
        if self.pending_profile_switch.is_some() {
            self.reject_profile_switch(
                profile_id,
                "another profile switch is already being prepared".into(),
            );
            return VecDeque::new();
        }
        if !self.pending_backend_runtimes.is_empty() {
            self.reject_profile_switch(
                profile_id,
                "a provider switch is already being prepared".into(),
            );
            return VecDeque::new();
        }

        let graph_path = self.installed_module_graph_path();
        let paths = match ProfilePaths::from_root_graph(&graph_path) {
            Ok(paths) => paths,
            Err(error) => {
                self.reject_profile_switch(profile_id, error.to_string());
                return VecDeque::new();
            }
        };
        let profile = match paths.load(profile_id) {
            Ok(profile) => profile,
            Err(error) => {
                self.reject_profile_switch(profile_id, error.to_string());
                return VecDeque::new();
            }
        };
        let graph = match load_installed_module_graph_for_profile(&graph_path, &profile) {
            Ok(graph) => graph,
            Err(error) => {
                self.reject_profile_switch(profile_id, error.to_string());
                return VecDeque::new();
            }
        };
        let shared = match SettingsStore::load() {
            Ok(settings) => settings,
            Err(error) => {
                self.reject_profile_switch(profile_id, error.to_string());
                return VecDeque::new();
            }
        };
        let settings = match super::discovery::effective_profile_settings(shared, Some(&profile)) {
            Ok(settings) => Arc::new(settings),
            Err(error) => {
                self.reject_profile_switch(profile_id, error.to_string());
                return VecDeque::new();
            }
        };

        let catalog = match FrontendCatalog::from_modules(&self.modules, Some(&graph)) {
            Ok(catalog) => catalog,
            Err(error) => {
                self.reject_profile_switch(profile_id, error.to_string());
                return VecDeque::new();
            }
        };
        let temporary_catalog = FrontendCatalogHandle::from(catalog.clone());
        let entries = catalog
            .top_level_surfaces()
            .into_iter()
            .map(|entry| (entry.compiled.manifest.package.id.clone(), entry))
            .collect::<HashMap<_, _>>();
        let desired_surfaces = profile
            .roots
            .iter()
            .filter(|(_, root)| root.active)
            .map(|(instance_id, _)| instance_id.clone())
            .collect::<HashSet<_>>();
        let existing_surfaces = self
            .components
            .iter()
            .map(|runtime| runtime.surface_id.clone())
            .collect::<HashSet<_>>();
        let mut prepared_frontends = Vec::new();
        for (instance_id, root) in profile.roots.iter().filter(|(_, root)| root.active) {
            if existing_surfaces.contains(instance_id) {
                continue;
            }
            let Some(entry) = entries.get(&root.module) else {
                self.reject_profile_switch(
                    profile_id,
                    format!("profile root {instance_id} has no mountable frontend entrypoint"),
                );
                return VecDeque::new();
            };
            let mut component = FrontendSurfaceComponent::new(
                entry.compiled.clone(),
                entry.module_dir.clone(),
                temporary_catalog.clone(),
                Arc::new(self.interfaces.catalog()),
                settings.clone(),
            )
            .with_instance_id(instance_id)
            .with_graph_i18n_catalogs(self.profile_i18n_catalog_paths(&graph));
            let diagnostics = self.diagnostics.register(root.module.clone());
            let mut requests = match component.mount(ComponentContext {
                component_id: root.module.clone(),
                surface_id: instance_id.clone(),
                diagnostics,
            }) {
                Ok(requests) => VecDeque::from(requests),
                Err(error) => {
                    self.reject_profile_switch(profile_id, error.to_string());
                    return VecDeque::new();
                }
            };
            if let Err(error) = component.locale_changed(&self.locale) {
                self.reject_profile_switch(profile_id, error.to_string());
                return VecDeque::new();
            }
            for state in self.latest_service_state.values() {
                let event = ServiceEvent::Updated {
                    service: state.interface.clone(),
                    source_module: state.provider_id.clone(),
                    payload: state.state.clone(),
                };
                if component.observes_service_event(&event) {
                    match component.handle_service_event(&event) {
                        Ok(next) => requests.extend(next),
                        Err(error) => {
                            self.reject_profile_switch(profile_id, error.to_string());
                            return VecDeque::new();
                        }
                    }
                }
            }
            prepared_frontends.push(PreparedProfileFrontend {
                component,
                requests,
            });
        }

        let (mut candidates, statuses) = backend_launch_candidates_from_graph(
            &graph,
            &self.modules,
            &settings,
            &self.interfaces,
        );
        if let Some(status) = statuses.iter().find(|status| {
            !matches!(
                status.status,
                "optional_backend_unavailable" | "optional_backend_inactive"
            )
        }) {
            self.reject_profile_switch(profile_id, status.message.clone());
            return VecDeque::new();
        }
        let candidate_theme_id = load_active_theme(settings.shell()).0.active().id.clone();
        for candidate in &mut candidates {
            Self::apply_runtime_settings(
                candidate,
                &candidate_theme_id,
                &settings.shell().i18n.locale,
            );
        }
        let desired_providers = candidates
            .iter()
            .map(|candidate| (candidate.interface.clone(), candidate.module_id.clone()))
            .collect::<HashMap<_, _>>();
        let current_configs = self
            .installed_module_graph
            .as_ref()
            .map(|current_graph| {
                let (mut current, _) = backend_launch_candidates_from_graph(
                    current_graph,
                    &self.modules,
                    &self.settings_store,
                    &self.interfaces,
                );
                for candidate in &mut current {
                    self.apply_shell_runtime_settings(candidate);
                }
                current
                    .into_iter()
                    .map(|candidate| {
                        (
                            candidate.interface,
                            (candidate.module_id, candidate.settings),
                        )
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let changed = candidates
            .into_iter()
            .filter(|candidate| {
                let running_matches = self
                    .backend_runtimes
                    .get(&candidate.interface)
                    .is_some_and(|running| running.provider_id == candidate.module_id);
                let config_matches = current_configs
                    .get(&candidate.interface)
                    .is_some_and(|(provider_id, settings)| {
                        provider_id == &candidate.module_id && settings == &candidate.settings
                    });
                !(running_matches && config_matches)
            })
            .collect::<Vec<_>>();

        let mut pending = PendingProfileSwitch {
            profile_id: profile_id.to_string(),
            graph,
            settings,
            catalog,
            desired_surfaces,
            desired_providers,
            prepared_frontends,
            candidate_backends: HashMap::new(),
            waiting_backends: HashSet::new(),
        };
        if !changed.is_empty() {
            let Some(ctx) = self.backend_respawn.clone() else {
                self.reject_profile_switch(
                    profile_id,
                    "backend runtime is unavailable while candidate providers need preparation"
                        .into(),
                );
                return VecDeque::new();
            };
            for candidate in changed {
                let interface = candidate.interface.clone();
                let event_provider_id = format!(
                    "@mesh/profile-candidate/{}/{}/{}",
                    profile_id, interface, candidate.module_id
                );
                let slot = self.start_backend_candidate_with_event_id(
                    &ctx.handle,
                    ctx.tx.clone(),
                    candidate,
                    ctx.eventfd_fd,
                    event_provider_id,
                );
                pending.waiting_backends.insert(interface.clone());
                pending.candidate_backends.insert(interface, slot);
            }
        }

        self.pending_profile_switch = Some(pending);
        if self
            .pending_profile_switch
            .as_ref()
            .is_some_and(|pending| pending.waiting_backends.is_empty())
        {
            return self.commit_pending_profile_switch();
        }
        tracing::info!(
            profile_id,
            "profile candidate prepared; waiting for backend readiness"
        );
        VecDeque::new()
    }

    pub(in crate::shell) fn handle_profile_backend_lifecycle(
        &mut self,
        interface: &str,
        provider_id: &str,
        status: BackendRuntimeStatus,
    ) -> bool {
        let Some(pending) = self.pending_profile_switch.as_mut() else {
            return false;
        };
        let is_candidate = pending
            .candidate_backends
            .get(interface)
            .is_some_and(|slot| {
                *slot
                    .event_provider_id
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    == provider_id
            });
        if !is_candidate {
            return false;
        }
        if status == BackendRuntimeStatus::Running {
            pending.waiting_backends.remove(interface);
            if pending.waiting_backends.is_empty() {
                let requests = self.commit_pending_profile_switch();
                self.deferred_requests.extend(requests);
            }
        } else if matches!(
            status,
            BackendRuntimeStatus::InitFailed
                | BackendRuntimeStatus::Failed
                | BackendRuntimeStatus::Stopped
        ) {
            self.abort_pending_profile_switch(format!(
                "provider {provider_id} failed to initialize for {interface}"
            ));
        }
        true
    }

    fn commit_pending_profile_switch(&mut self) -> VecDeque<CoreRequest> {
        let Some(mut pending) = self.pending_profile_switch.take() else {
            return VecDeque::new();
        };
        let paths = match ProfilePaths::from_root_graph(&self.installed_module_graph_path()) {
            Ok(paths) => paths,
            Err(error) => {
                self.abort_profile_candidate(pending, error.to_string());
                return VecDeque::new();
            }
        };
        if let Err(error) = paths.set_active(&pending.profile_id) {
            self.abort_profile_candidate(pending, error.to_string());
            return VecDeque::new();
        }

        self.frontend_catalog.replace(pending.catalog, None);
        for prepared in &mut pending.prepared_frontends {
            prepared
                .component
                .adopt_frontend_catalog(self.frontend_catalog.clone());
        }
        for index in (0..self.components.len()).rev() {
            if !pending
                .desired_surfaces
                .contains(&self.components[index].surface_id)
            {
                self.remove_profile_component(index);
            }
        }
        let mut requests = VecDeque::new();
        for prepared in pending.prepared_frontends {
            requests.extend(prepared.requests);
            self.register_component(Box::new(prepared.component));
        }

        let obsolete = self
            .backend_runtimes
            .keys()
            .filter(|interface| !pending.desired_providers.contains_key(*interface))
            .cloned()
            .collect::<Vec<_>>();
        for interface in obsolete {
            self.stop_backend_runtime(&interface);
        }
        for (interface, slot) in pending.candidate_backends {
            *slot
                .event_provider_id
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = slot.provider_id.clone();
            self.backend_supervision.remove(&interface);
            self.replace_backend_runtime(interface.clone(), slot);
            self.note_backend_running(&interface);
        }

        let old_theme = self.settings.theme.active.clone();
        let old_locale = self.settings.i18n.clone();
        self.settings_store = pending.settings;
        self.settings = self.settings_store.shell().clone();
        mesh_core_icon::set_default_shell_pack(self.settings.icons.default_pack.clone());
        mesh_core_render::set_blur_quality(blur_quality_from_settings(&self.settings.render.blur));
        if old_theme != self.settings.theme.active {
            let (theme, watch) = load_active_theme(&self.settings);
            self.theme = theme;
            self.theme_watch = watch;
            if let Err(error) = self.mark_components_theme_changed() {
                tracing::warn!("profile theme refresh failed after commit: {error}");
            }
            let theme_id = self.theme.active().id.clone();
            if let Ok(next) = self.sync_theme_service_state(&theme_id) {
                requests.extend(next);
            }
        }
        if old_locale.locale != self.settings.i18n.locale
            || old_locale.fallback_locale != self.settings.i18n.fallback_locale
        {
            self.locale = LocaleEngine::with_fallback_locale(
                self.settings.i18n.locale.clone(),
                self.settings.i18n.fallback_locale.clone(),
            );
            if let Err(error) = self.mark_components_locale_changed() {
                tracing::warn!("profile locale refresh failed after commit: {error}");
            }
            if let Ok(next) = self.sync_locale_service_state() {
                requests.extend(next);
            }
        }
        if let Err(error) = self.apply_settings_to_components() {
            tracing::warn!("profile settings refresh failed after commit: {error}");
        }

        self.installed_module_graph = Some(pending.graph);
        self.active_profile_id = Some(pending.profile_id.clone());
        let active_graph = self
            .installed_module_graph
            .as_ref()
            .expect("candidate graph was installed")
            .clone();
        self.register_interfaces_from_graph(&active_graph);
        self.sync_frontend_catalog_components();
        self.components_want_render = true;
        tracing::info!(
            profile_id = pending.profile_id,
            "switched shell profile live"
        );
        requests
    }

    fn remove_profile_component(&mut self, index: usize) {
        let surface_id = self.components[index].surface_id.clone();
        self.destroy_all_child_surfaces(index);
        self.presentation_engine.destroy_surface(&surface_id);
        self.components.remove(index);
        self.core.surfaces.remove(&surface_id);
        self.surfaces.remove(&surface_id);
        self.pending_popover_hides.remove(&surface_id);
        self.transfer_owned_keyboard_modes.remove(&surface_id);
        if self.keyboard_focus_surface.as_deref() == Some(surface_id.as_str()) {
            self.keyboard_focus_surface = None;
        }
        self.rebuild_component_surface_index();
        self.service_delivery_index.mark_dirty();
    }

    fn profile_i18n_catalog_paths(
        &self,
        graph: &InstalledModuleGraph,
    ) -> Vec<(String, String, PathBuf)> {
        graph
            .contributed_i18n()
            .iter()
            .filter_map(|catalog| {
                let module_dir = catalog.source.manifest_path.parent()?;
                Some((
                    catalog.module_id.clone(),
                    catalog.locale.clone(),
                    module_dir.join(&catalog.path),
                ))
            })
            .collect()
    }

    fn abort_pending_profile_switch(&mut self, message: String) {
        if let Some(pending) = self.pending_profile_switch.take() {
            self.abort_profile_candidate(pending, message);
        }
    }

    fn abort_profile_candidate(&mut self, pending: PendingProfileSwitch, message: String) {
        for slot in pending.candidate_backends.into_values() {
            slot.task.abort();
        }
        self.reject_profile_switch(&pending.profile_id, message);
    }

    fn reject_profile_switch(&mut self, profile_id: &str, message: String) {
        tracing::warn!(profile_id, "profile switch rejected: {message}");
        self.diagnostics.record_lifecycle_error(
            "@mesh/shell",
            "profile_switch_rejected",
            format!("profile {profile_id}: {message}"),
        );
    }
}
