use super::backend::{
    BackendRuntimeStatus, backend_launch_candidates_from_graph_with_capabilities,
};
use super::component::{FrontendCatalog, FrontendCatalogHandle, FrontendSurfaceComponent};
use super::*;
use mesh_core_module::package::{
    ComponentPlacement, InstalledModuleGraph, NodeSlotOverride, ProfilePaths, ShellProfile,
    load_installed_module_graph_for_profile,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

fn resolve_component_props_schema(
    mut schema: serde_json::Value,
    translator: &mesh_core_locale::ModuleTranslator<'_>,
) -> serde_json::Value {
    let Some(properties) = schema
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return schema;
    };

    for definition in properties.values_mut() {
        let Some(definition) = definition.as_object_mut() else {
            continue;
        };
        for field in ["label", "description"] {
            let Some(value) = definition.get_mut(field) else {
                continue;
            };
            let Some(localized) = value.as_object() else {
                continue;
            };
            let key = localized
                .get("t")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            let fallback = localized
                .get("fallback")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            if let Some(key) = key.as_deref() {
                *value =
                    serde_json::Value::String(translator.resolve(key, fallback.as_deref()).text);
            } else if let Some(fallback) = fallback {
                *value = serde_json::Value::String(fallback);
            }
        }
    }
    schema
}

pub(super) struct PreparedProfileFrontend {
    component: FrontendSurfaceComponent,
    requests: VecDeque<CoreRequest>,
}

#[cfg(test)]
mod tests {
    use super::super::discovery::effective_profile_settings;
    use super::update_module_prop_override;
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
                "schemaVersion": 3,
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
    fn module_prop_updates_preserve_siblings_and_reset_sparsely() {
        let mut namespace = serde_json::json!({
            "surface": { "anchor": "top" },
            "props": { "global": { "density": "compact" } }
        });

        update_module_prop_override(
            &mut namespace,
            None,
            "blur_enabled",
            Some(serde_json::json!(false)),
        );
        assert_eq!(
            namespace["props"]["global"],
            serde_json::json!({ "density": "compact", "blur_enabled": false })
        );

        update_module_prop_override(&mut namespace, None, "blur_enabled", None);
        update_module_prop_override(&mut namespace, None, "density", None);
        assert!(namespace.get("props").is_none());
        assert_eq!(namespace["surface"]["anchor"], serde_json::json!("top"));
    }

    #[test]
    fn instance_prop_updates_preserve_global_values_and_prune_empty_scopes() {
        let mut namespace = serde_json::json!({
            "props": { "global": { "density": "compact" } }
        });
        update_module_prop_override(
            &mut namespace,
            Some("@test/panel#bottom"),
            "density",
            Some(serde_json::json!("comfortable")),
        );
        assert_eq!(
            namespace["props"]["instances"]["@test/panel#bottom"]["density"],
            "comfortable"
        );

        update_module_prop_override(&mut namespace, Some("@test/panel#bottom"), "density", None);
        assert!(namespace["props"].get("instances").is_none());
        assert_eq!(namespace["props"]["global"]["density"], "compact");
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
                "schemaVersion": 3,
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
    locale: LocaleEngine,
    settings: Arc<SettingsStore>,
    resources: super::discovery::PreparedResourceSnapshot,
    prepared_theme: Option<(mesh_core_theme::Theme, ThemeWatchState)>,
    catalog: FrontendCatalog,
    desired_surfaces: HashSet<String>,
    desired_providers: HashMap<String, String>,
    prepared_frontends: Vec<PreparedProfileFrontend>,
    candidate_backends: HashMap<String, BackendRuntimeSlot>,
    waiting_backends: HashSet<String>,
    candidate_started: HashSet<String>,
    candidate_initial_states: HashMap<String, serde_json::Value>,
}

pub(super) struct PendingResourcePreparation {
    profile_id: String,
    profile: ShellProfile,
    graph: InstalledModuleGraph,
    locale: LocaleEngine,
    settings: Arc<SettingsStore>,
    resource_job: super::discovery::ResourcePreparationJob,
}

impl Shell {
    pub(in crate::shell) fn profile_transition_pending(&self) -> bool {
        self.pending_resource_preparation.is_some() || self.pending_profile_switch.is_some()
    }

    pub(in crate::shell) fn profile_candidate_is_pending(
        &self,
        interface: &str,
        provider_id: &str,
    ) -> bool {
        self.pending_profile_switch
            .as_ref()
            .and_then(|pending| pending.candidate_backends.get(interface))
            .is_some_and(|slot| {
                *slot
                    .event_provider_id
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    == provider_id
            })
    }

    pub(in crate::shell) fn apply_node_slot_edit(
        &mut self,
        profile_id: &str,
        root_instance: &str,
        slot: &str,
        nodes: Option<serde_json::Value>,
        expected_generation: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if self.profile_transition_pending() {
            return Err(ShellRunError::Package(
                "node edit rejected while a profile switch is pending".into(),
            ));
        }
        let graph_path = self.installed_module_graph_path();
        let paths = ProfilePaths::from_root_graph(&graph_path)?;
        let mut profile = paths.load(profile_id)?;
        let expected_revision = profile.revision;
        let generation = profile_generation(&profile)?;
        if generation != expected_generation {
            return Err(ShellRunError::Package(format!(
                "node_edit_generation_conflict: expected {expected_generation}, current {generation}"
            )));
        }

        match nodes {
            Some(nodes) => {
                let nodes: Vec<ComponentPlacement> =
                    serde_json::from_value(nodes).map_err(|error| {
                        ShellRunError::Package(format!("invalid_node_props: {error}"))
                    })?;
                profile
                    .node_slots
                    .entry(root_instance.to_string())
                    .or_default()
                    .insert(slot.to_string(), NodeSlotOverride { nodes });
            }
            None => {
                if let Some(slots) = profile.node_slots.get_mut(root_instance) {
                    slots.remove(slot);
                    if slots.is_empty() {
                        profile.node_slots.remove(root_instance);
                    }
                }
            }
        }
        profile.validate()?;
        let candidate = load_installed_module_graph_for_profile(&graph_path, &profile)?;
        let previous_catalog = self.frontend_catalog.snapshot().catalog;
        FrontendCatalog::from_modules_reusing(
            &self.modules,
            Some(&candidate),
            Some(&previous_catalog),
        )?;

        paths.save_if_revision(profile_id, &profile, expected_revision)?;
        if self.active_profile_id.as_deref() == Some(profile_id) {
            return Ok(self.apply_switch_profile(profile_id));
        }
        Ok(VecDeque::new())
    }

    pub(in crate::shell) fn sync_composition_service_state(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let Some(profile_id) = self.active_profile_id.clone() else {
            return self.broadcast_service_event(ServiceEvent::Updated {
                service: "mesh.composition".into(),
                source_module: "@mesh/shell".into(),
                payload: serde_json::json!({
                    "profile_id": "",
                    "generation": "",
                    "roots": [],
                    "slots": [],
                    "palette": [],
                }),
            });
        };
        let paths = ProfilePaths::from_root_graph(&self.installed_module_graph_path())?;
        let profile_delta = paths.load(&profile_id)?;
        let generation = profile_generation(&profile_delta)?;
        let graph = self.load_installed_module_graph_cached()?.clone();
        let manifests = graph
            .modules()
            .into_iter()
            .map(|module| &module.manifest)
            .collect::<Vec<_>>();
        let profile =
            mesh_core_module::package::resolve_composition(&profile_delta, manifests)?.to_profile();
        let catalog = self.frontend_catalog.snapshot().catalog;
        let mut roots = Vec::new();
        let mut slots = Vec::new();
        let mut palette = Vec::new();

        for (root_instance, root) in &profile.roots {
            roots.push(serde_json::json!({
                "instance": root_instance,
                "module": root.module,
                "active": root.active,
            }));
            if !root.active {
                continue;
            }
            let Some(host) = catalog.modules.get(&root.module) else {
                continue;
            };
            for descriptor in customizable_slots(&host.compiled.component) {
                let Some(point) = descriptor.extension_point.as_deref() else {
                    continue;
                };
                let slot_name = descriptor.name.as_deref().unwrap_or_default();
                let hosted = host.compiled.manifest.hosted_extension_points.get(point);
                let defaults = hosted
                    .and_then(|hosted| hosted.slots.get(slot_name))
                    .map(|slot| slot.defaults.clone())
                    .unwrap_or_default();
                let effective = catalog
                    .node_slot_placement(root_instance, slot_name)
                    .map(|slot| serde_json::to_value(&slot.nodes))
                    .transpose()
                    .map_err(|error| ShellRunError::Package(error.to_string()))?
                    .unwrap_or_else(|| {
                        serde_json::Value::Array(
                            defaults
                                .iter()
                                .enumerate()
                                .map(|(index, reference)| {
                                    serde_json::json!({
                                        "id": format!("default-{index}"),
                                        "use": reference,
                                        "props": {},
                                    })
                                })
                                .collect(),
                        )
                    });
                slots.push(serde_json::json!({
                    "root_instance": root_instance,
                    "name": slot_name,
                    "contract": point,
                    "layout": hosted.and_then(|hosted| hosted.layout.clone()).unwrap_or_else(|| "row".into()),
                    "max": hosted.and_then(|hosted| hosted.max),
                    "defaults": defaults,
                    "nodes": effective,
                    "overridden": catalog.node_slot_placement(root_instance, slot_name).is_some(),
                }));
                for contribution in catalog.extension_point_contributions_for(&root.module, point) {
                    let compiled = catalog
                        .contribution_entry(
                            &contribution.source_module_id,
                            &contribution.contribution_id,
                        )
                        .expect("resolved contribution is compiled");
                    let props_schema = mesh_core_component::props_settings_schema(
                        compiled.component.props.as_ref(),
                    )
                    .map(|schema| {
                        resolve_component_props_schema(
                            schema,
                            &self
                                .locale
                                .module_translator(&contribution.source_module_id),
                        )
                    });
                    palette.push(serde_json::json!({
                        "contract": point,
                        "use": format!("{}:{}", contribution.source_module_id, contribution.contribution_id),
                        "module": contribution.source_module_id,
                        "id": contribution.contribution_id,
                        "props_schema": props_schema,
                    }));
                }
            }
        }
        palette.sort_by(|left, right| left["use"].as_str().cmp(&right["use"].as_str()));
        palette.dedup_by(|left, right| left["use"] == right["use"]);

        self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.composition".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({
                "profile_id": profile_id,
                "generation": generation,
                "roots": roots,
                "slots": slots,
                "palette": palette,
            }),
        })
    }

    pub(in crate::shell) fn apply_set_module_prop(
        &mut self,
        module_id: &str,
        instance_id: Option<&str>,
        prop: &str,
        value: Option<serde_json::Value>,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let catalog = self.frontend_catalog.snapshot();
        let definition = catalog
            .catalog
            .module(module_id)
            .and_then(|entry| entry.compiled.component.props.as_ref())
            .and_then(|block| {
                block
                    .props
                    .iter()
                    .find(|definition| definition.name == prop)
            })
            .filter(|definition| definition.expose)
            .cloned()
            .ok_or_else(|| ShellRunError::FrontendComposition {
                message: format!(
                    "module '{module_id}' does not expose a configurable prop named '{prop}'"
                ),
            })?;

        if let Some(instance_id) = instance_id
            && !self.components.iter().any(|runtime| {
                runtime.surface_id == instance_id && runtime.component.id() == module_id
            })
        {
            return Err(ShellRunError::FrontendComposition {
                message: format!("'{instance_id}' is not a live instance of module '{module_id}'"),
            });
        }

        let settings_path = instance_id
            .map(|id| format!("props.instances.{id}"))
            .unwrap_or_else(|| "props.global".to_string());

        if let Some(value) = value.as_ref() {
            let parsed = mesh_core_component::json_to_prop_value_ref(value).map_err(|error| {
                ShellRunError::FrontendComposition {
                    message: format!(
                        "setting {module_id}.{settings_path}.{prop} must be a string, number, or boolean: {error}"
                    ),
                }
            })?;
            mesh_core_component::validate_prop_value(&definition, &parsed).map_err(|error| {
                ShellRunError::FrontendComposition {
                    message: format!("invalid setting {module_id}.{settings_path}.{prop}: {error}"),
                }
            })?;
        }

        let effective = if let Some(profile_id) = self.active_profile_id.clone() {
            let paths = ProfilePaths::from_root_graph(&self.installed_module_graph_path())?;
            let mut profile = paths.load(&profile_id)?;
            let expected_revision = profile.revision;
            let namespace = profile
                .settings
                .entry(module_id.to_string())
                .or_insert_with(|| serde_json::json!({}));
            update_module_prop_override(namespace, instance_id, prop, value);
            if namespace.as_object().is_some_and(serde_json::Map::is_empty) {
                profile.settings.remove(module_id);
            }
            paths.save_if_revision(&profile_id, &profile, expected_revision)?;
            let shared =
                SettingsStore::load().map_err(|error| ShellRunError::FrontendComposition {
                    message: format!("failed to reload shared settings: {error}"),
                })?;
            super::discovery::effective_profile_settings(shared, Some(&profile)).map_err(
                |error| ShellRunError::FrontendComposition {
                    message: format!("failed to resolve updated profile settings: {error}"),
                },
            )?
        } else {
            let mut shared =
                SettingsStore::load().map_err(|error| ShellRunError::FrontendComposition {
                    message: format!("failed to load settings for update: {error}"),
                })?;
            let expected_revision = shared.revision();
            let mut namespace = shared.namespace(module_id);
            update_module_prop_override(&mut namespace, instance_id, prop, value);
            shared.set_namespace(module_id, namespace);
            shared
                .save_if_revision(expected_revision)
                .map_err(|error| ShellRunError::FrontendComposition {
                    message: format!("failed to save settings: {error}"),
                })?;
            shared
        };

        let mut effective = effective;
        if let Some(graph) = self.installed_module_graph.as_ref() {
            super::discovery::register_graph_settings_schemas(&mut effective, graph).map_err(
                |error| ShellRunError::FrontendComposition {
                    message: format!("failed to register settings schemas: {error}"),
                },
            )?;
        }
        self.settings_store = Arc::new(effective);
        self.settings =
            mesh_core_config::resolve_shell_locale_settings(self.settings_store.shell());
        self.settings_watch.modified_at = std::fs::metadata(&self.settings_watch.path)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        self.apply_settings_to_components()?;
        self.components_want_render = true;
        Ok(VecDeque::new())
    }

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
        if let Some(pending) = self.pending_resource_preparation.take() {
            pending.resource_job.cancel();
            self.reject_profile_switch(
                &pending.profile_id,
                format!("profile switch was superseded by '{profile_id}'"),
            );
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
            Ok(mut settings) => {
                if let Err(error) =
                    super::discovery::register_graph_settings_schemas(&mut settings, &graph)
                {
                    self.reject_profile_switch(
                        profile_id,
                        format!("failed to register candidate settings schemas: {error}"),
                    );
                    return VecDeque::new();
                }
                Arc::new(settings)
            }
            Err(error) => {
                self.reject_profile_switch(profile_id, error.to_string());
                return VecDeque::new();
            }
        };

        let locale = match self.prepare_locale_for_settings(settings.shell(), &graph) {
            Ok(locale) => locale,
            Err(error) => {
                self.reject_profile_switch(
                    profile_id,
                    format!("candidate locale catalogs are invalid: {error}"),
                );
                return VecDeque::new();
            }
        };
        let resource_job = match self.start_resource_preparation_job(&graph, &settings) {
            Ok(job) => job,
            Err(error) => {
                self.reject_profile_switch(
                    profile_id,
                    format!("candidate resources are invalid: {error}"),
                );
                return VecDeque::new();
            }
        };
        self.pending_resource_preparation = Some(PendingResourcePreparation {
            profile_id: profile_id.to_string(),
            profile,
            graph,
            locale,
            settings,
            resource_job,
        });
        self.poll_pending_resource_preparation()
    }

    pub(in crate::shell) fn poll_pending_resource_preparation(&mut self) -> VecDeque<CoreRequest> {
        let Some(pending) = self.pending_resource_preparation.take() else {
            return VecDeque::new();
        };
        let PendingResourcePreparation {
            profile_id,
            profile,
            graph,
            locale,
            settings,
            mut resource_job,
        } = pending;
        let Some(result) = resource_job.try_wait() else {
            self.pending_resource_preparation = Some(PendingResourcePreparation {
                profile_id,
                profile,
                graph,
                locale,
                settings,
                resource_job,
            });
            return VecDeque::new();
        };
        match result {
            Ok(resources)
                if resources
                    .resource_lease
                    .as_ref()
                    .is_some_and(mesh_core_resources::ResourcePreparationLease::is_current) =>
            {
                self.continue_profile_switch(
                    profile_id, profile, graph, locale, settings, resources,
                )
            }
            Ok(_) => {
                resource_job.retire();
                self.reject_profile_switch(
                    &profile_id,
                    "candidate resources were superseded".into(),
                );
                VecDeque::new()
            }
            Err(error) => {
                resource_job.retire();
                self.reject_profile_switch(
                    &profile_id,
                    format!("candidate resources are invalid: {error}"),
                );
                VecDeque::new()
            }
        }
    }

    fn continue_profile_switch(
        &mut self,
        profile_id: String,
        profile: ShellProfile,
        graph: InstalledModuleGraph,
        locale: LocaleEngine,
        settings: Arc<SettingsStore>,
        resources: super::discovery::PreparedResourceSnapshot,
    ) -> VecDeque<CoreRequest> {
        let resolved_shell_settings =
            mesh_core_config::resolve_shell_locale_settings(settings.shell());
        let previous_catalog = self.frontend_catalog.snapshot().catalog;
        let catalog = match FrontendCatalog::from_modules_reusing(
            &self.modules,
            Some(&graph),
            Some(&previous_catalog),
        ) {
            Ok(catalog) => catalog,
            Err(error) => {
                self.reject_profile_switch(&profile_id, error.to_string());
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
        let existing_surface_modules = self
            .components
            .iter()
            .map(|runtime| {
                (
                    runtime.surface_id.clone(),
                    runtime.component.id().to_string(),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut prepared_frontends = Vec::new();
        for (instance_id, root) in profile.roots.iter().filter(|(_, root)| root.active) {
            if existing_surface_modules
                .get(instance_id)
                .is_some_and(|module_id| module_id == &root.module)
            {
                continue;
            }
            let Some(entry) = entries.get(&root.module) else {
                self.reject_profile_switch(
                    &profile_id,
                    format!("profile root {instance_id} has no mountable frontend entrypoint"),
                );
                return VecDeque::new();
            };
            let mut component = FrontendSurfaceComponent::new(
                entry.compiled.clone(),
                entry.module_dir.clone(),
                temporary_catalog.clone(),
                Arc::new(self.interfaces.resolved_catalog()),
                settings.clone(),
            )
            .with_effective_capabilities(self.effective_capabilities.clone())
            .with_instance_id(instance_id)
            .with_locale_catalog_snapshot(locale.catalog_snapshot());
            let diagnostics = self
                .diagnostics
                .register_instance(root.module.clone(), instance_id.clone());
            let mut requests = match component.mount(ComponentContext {
                component_id: root.module.clone(),
                surface_id: instance_id.clone(),
                diagnostics,
            }) {
                Ok(requests) => VecDeque::from(requests),
                Err(error) => {
                    self.reject_profile_switch(&profile_id, error.to_string());
                    return VecDeque::new();
                }
            };
            if let Err(error) = component.locale_changed(&locale) {
                self.reject_profile_switch(&profile_id, error.to_string());
                return VecDeque::new();
            }
            for state in self.latest_service_state.values() {
                let event = ServiceEvent::Updated {
                    service: state.interface.clone(),
                    source_module: state.provider_id.clone(),
                    payload: state.state.clone(),
                };
                if component.observes_service_event(&event) {
                    match component.handle_service_event_with_generation(&event, state.generation) {
                        Ok(next) => requests.extend(next),
                        Err(error) => {
                            self.reject_profile_switch(&profile_id, error.to_string());
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

        let (mut candidates, statuses) = backend_launch_candidates_from_graph_with_capabilities(
            &graph,
            &self.modules,
            &settings,
            &self.interfaces,
            Some(&self.effective_capabilities),
        );
        if let Some(status) = statuses.iter().find(|status| {
            !matches!(
                status.status,
                "optional_backend_unavailable" | "optional_backend_inactive"
            )
        }) {
            self.reject_profile_switch(&profile_id, status.message.clone());
            return VecDeque::new();
        }
        let theme_changed = self.settings.theme != settings.shell().theme
            || self.settings.fonts != settings.shell().fonts;
        let prepared_theme = if theme_changed {
            if graph.theme_catalog().is_empty() {
                let (engine, watch) = load_active_theme(settings.shell());
                Some((engine.active().clone(), watch))
            } else {
                match prepare_theme_for_graph(settings.shell(), &graph) {
                    Ok(prepared) => Some(prepared),
                    Err(error) => {
                        self.reject_profile_switch(
                            &profile_id,
                            format!("candidate theme is invalid: {error}"),
                        );
                        return VecDeque::new();
                    }
                }
            }
        } else {
            None
        };
        let candidate_theme_id = prepared_theme
            .as_ref()
            .map(|(theme, _)| theme.id.clone())
            .unwrap_or_else(|| self.theme.active().id.clone());
        for candidate in &mut candidates {
            Self::apply_runtime_settings(
                candidate,
                &candidate_theme_id,
                &resolved_shell_settings.i18n.locale,
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
                let (mut current, _) = backend_launch_candidates_from_graph_with_capabilities(
                    current_graph,
                    &self.modules,
                    &self.settings_store,
                    &self.interfaces,
                    Some(&self.effective_capabilities),
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
                let config_matches = current_configs.get(&candidate.interface).is_some_and(
                    |(provider_id, settings)| {
                        provider_id == &candidate.module_id && settings == &candidate.settings
                    },
                );
                !(running_matches && config_matches)
            })
            .collect::<Vec<_>>();

        let mut pending = PendingProfileSwitch {
            profile_id: profile_id.clone(),
            graph,
            locale,
            settings,
            resources,
            prepared_theme,
            catalog,
            desired_surfaces,
            desired_providers,
            prepared_frontends,
            candidate_backends: HashMap::new(),
            waiting_backends: HashSet::new(),
            candidate_started: HashSet::new(),
            candidate_initial_states: HashMap::new(),
        };
        if !changed.is_empty() {
            let Some(ctx) = self.backend_respawn.clone() else {
                self.reject_profile_switch(
                    &profile_id,
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
        let needs_initial_state = self.service_requires_initial_state(interface);
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
            pending.candidate_started.insert(interface.to_string());
            if !needs_initial_state || pending.candidate_initial_states.contains_key(interface) {
                pending.waiting_backends.remove(interface);
            }
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

    pub(in crate::shell) fn capture_profile_backend_update(
        &mut self,
        interface: &str,
        provider_id: &str,
        event: ServiceEvent,
    ) -> bool {
        let Some(pending) = self.pending_profile_switch.as_ref() else {
            return false;
        };
        let Some(candidate) = pending.candidate_backends.get(interface) else {
            return false;
        };
        let is_candidate = candidate
            .event_provider_id
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_str()
            == provider_id;
        if !is_candidate {
            return false;
        }
        let actual_provider_id = candidate.provider_id.clone();
        let ServiceEvent::Updated { payload, .. } = self.normalize_service_event(event) else {
            return true;
        };
        if !self.validate_service_state_shape(interface, &actual_provider_id, &payload) {
            self.abort_pending_profile_switch(format!(
                "provider {actual_provider_id} emitted an invalid initial service snapshot for {interface}"
            ));
            return true;
        }
        let should_commit = if let Some(pending) = self.pending_profile_switch.as_mut() {
            pending
                .candidate_initial_states
                .insert(interface.to_string(), payload);
            if pending.candidate_started.contains(interface) {
                pending.waiting_backends.remove(interface);
            }
            pending.waiting_backends.is_empty()
        } else {
            false
        };
        if should_commit {
            let requests = self.commit_pending_profile_switch();
            self.deferred_requests.extend(requests);
        }
        tracing::debug!(interface, provider_id, "buffered prepared profile snapshot");
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
        // Capture the pointer this candidate is about to overwrite so a
        // failure in a later, still-fallible commit step (resource snapshot)
        // can restore it instead of leaving the durable pointer advanced to
        // a candidate the running shell aborted and never actually adopted.
        let previous_active = match paths.active_profile_id() {
            Ok(previous) => previous,
            Err(error) => {
                self.abort_profile_candidate(pending, error.to_string());
                return VecDeque::new();
            }
        };
        if let Err(error) = paths.set_active(&pending.profile_id) {
            self.abort_profile_candidate(pending, error.to_string());
            return VecDeque::new();
        }
        if let Err(error) = self.commit_resource_snapshot(&pending.resources) {
            if let Err(restore_error) = paths.restore_active(previous_active.as_deref()) {
                tracing::error!(
                    "failed to restore previous active-profile pointer after an aborted \
                     profile switch; the durable pointer may now name a profile the running \
                     shell never adopted: {restore_error}"
                );
            }
            self.abort_profile_candidate(pending, error.to_string());
            return VecDeque::new();
        }

        self.frontend_catalog.replace(pending.catalog, None);
        for prepared in &mut pending.prepared_frontends {
            prepared
                .component
                .adopt_frontend_catalog(self.frontend_catalog.clone());
        }
        let prepared_surfaces = pending
            .prepared_frontends
            .iter()
            .map(|prepared| prepared.component.surface_id().to_string())
            .collect::<HashSet<_>>();
        for index in (0..self.components.len()).rev() {
            if !pending
                .desired_surfaces
                .contains(&self.components[index].surface_id)
                || prepared_surfaces.contains(&self.components[index].surface_id)
            {
                self.remove_profile_component(index);
            }
        }
        let mut requests = VecDeque::new();
        for prepared in pending.prepared_frontends {
            requests.extend(prepared.requests);
            self.register_component(Box::new(prepared.component));
        }
        let initial_states = std::mem::take(&mut pending.candidate_initial_states);
        let mut prepared_states = Vec::new();
        let mut ready_providers = Vec::new();

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
            let initial_state = initial_states.get(&interface).cloned();
            let provider_id = slot.provider_id.clone();
            *slot
                .event_provider_id
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = slot.provider_id.clone();
            self.backend_supervision.remove(&interface);
            self.replace_backend_runtime(interface.clone(), slot);
            self.note_backend_running(&interface);
            ready_providers.push((interface.clone(), provider_id.clone()));
            if let Some(payload) = initial_state {
                prepared_states.push((interface, provider_id, payload));
            }
        }

        let old_theme = self.settings.theme.clone();
        let old_fonts = self.settings.fonts.clone();
        let old_locale = self.settings.i18n.clone();
        let old_locale_catalog_revision = self.locale.catalog_snapshot().revision();
        let prepared_theme = pending.prepared_theme.take();
        self.settings_store = pending.settings;
        self.settings =
            mesh_core_config::resolve_shell_locale_settings(self.settings_store.shell());
        mesh_core_icon::set_default_shell_pack(self.settings.icons.default_pack.clone());
        mesh_core_render::set_blur_quality(blur_quality_from_settings(&self.settings.render.blur));
        if old_theme != self.settings.theme || old_fonts != self.settings.fonts {
            let (theme, watch) = prepared_theme
                .map(|(theme, watch)| (self.theme.with_active(theme), watch))
                .unwrap_or_else(|| {
                    let (theme, watch) = load_active_theme(&self.settings);
                    (theme, watch)
                });
            self.theme = theme;
            self.theme_watch = watch;
            self.theme.update_active(|active| {
                super::discovery::apply_font_registry_tokens(active, &self.font_registry);
            });
            self.theme_watch.revision = self.theme.active_snapshot().revision;
            if let Err(error) = self.mark_components_theme_changed() {
                tracing::warn!("profile theme refresh failed after commit: {error}");
                self.diagnostics.record_lifecycle_error(
                    "@mesh/shell",
                    "profile_post_commit_theme_refresh_failed",
                    format!(
                        "profile '{}' committed but a component rejected the new theme; the shell \
                         is now on the new generation with stale component theme state: {error}",
                        pending.profile_id
                    ),
                );
            }
            if let Ok(next) = self.sync_theme_service_state() {
                requests.extend(next);
            }
        }
        self.locale = pending.locale;
        if old_locale.locale != self.settings.i18n.locale
            || old_locale.fallback_locale != self.settings.i18n.fallback_locale
            || old_locale.policy != self.settings.i18n.policy
            || old_locale_catalog_revision != self.locale.catalog_snapshot().revision()
        {
            if let Err(error) = self.mark_components_locale_changed() {
                tracing::warn!("profile locale refresh failed after commit: {error}");
                self.diagnostics.record_lifecycle_error(
                    "@mesh/shell",
                    "profile_post_commit_locale_refresh_failed",
                    format!(
                        "profile '{}' committed but a component rejected the new locale; the shell \
                         is now on the new generation with stale component locale state: {error}",
                        pending.profile_id
                    ),
                );
            }
            if let Ok(next) = self.sync_locale_service_state() {
                requests.extend(next);
            }
        }
        if let Err(error) = self.apply_settings_to_components() {
            tracing::warn!("profile settings refresh failed after commit: {error}");
            self.diagnostics.record_lifecycle_error(
                "@mesh/shell",
                "profile_post_commit_settings_refresh_failed",
                format!(
                    "profile '{}' committed but applying its settings to a component failed; the \
                     shell is now on the new generation with stale component settings: {error}",
                    pending.profile_id
                ),
            );
        }

        self.installed_module_graph = Some(pending.graph);
        self.active_profile_id = Some(pending.profile_id.clone());
        let active_graph = self
            .installed_module_graph
            .as_ref()
            .expect("candidate graph was installed")
            .clone();
        self.register_interfaces_from_graph(&active_graph);
        for (interface, provider_id, payload) in prepared_states {
            let event = ServiceEvent::Updated {
                service: interface,
                source_module: provider_id,
                payload,
            };
            if self.record_latest_service_state(&event) {
                match self.deliver_service_event(&event) {
                    Ok(next) => requests.extend(next),
                    Err(error) => {
                        tracing::warn!("failed to deliver prepared profile service state: {error}")
                    }
                }
            }
        }
        for (interface, provider_id) in ready_providers {
            self.publish_backend_health(
                &interface,
                &provider_id,
                BackendRuntimeStatus::Running,
                "backend runtime ready",
                true,
            );
        }
        if let Ok(next) = self.sync_composition_service_state() {
            requests.extend(next);
        }
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
        let module_id = self.components[index].component.id().to_string();
        if let Err(error) = self.components[index].component.unmount() {
            tracing::warn!(
                module_id,
                error = %error,
                "profile replacement unmount failed"
            );
        }
        self.destroy_all_child_surfaces(index);
        self.presentation_engine.destroy_surface(&surface_id);
        self.components.remove(index);
        self.diagnostics.unregister(&module_id, &surface_id);
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

    fn abort_pending_profile_switch(&mut self, message: String) {
        if let Some(pending) = self.pending_profile_switch.take() {
            self.abort_profile_candidate(pending, message);
        }
    }

    fn abort_profile_candidate(&mut self, pending: PendingProfileSwitch, message: String) {
        if let Some(lease) = pending.resources.resource_lease.as_ref() {
            lease.retire();
        }
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

pub(super) fn profile_generation(
    profile: &mesh_core_module::package::ShellProfile,
) -> Result<String, ShellRunError> {
    let bytes = serde_json::to_vec(profile).map_err(|error| {
        ShellRunError::Package(format!("profile serialization failed: {error}"))
    })?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

fn customizable_slots(
    component: &mesh_core_component::ComponentFile,
) -> Vec<&mesh_core_component::template::SlotNode> {
    fn visit<'a>(
        nodes: &'a [mesh_core_component::template::TemplateNode],
        out: &mut Vec<&'a mesh_core_component::template::SlotNode>,
    ) {
        use mesh_core_component::template::TemplateNode;
        for node in nodes {
            match node {
                TemplateNode::Slot(slot) if slot.customizable => out.push(slot),
                TemplateNode::Element(node) => visit(&node.children, out),
                TemplateNode::Component(node) => visit(&node.children, out),
                TemplateNode::If(node) => {
                    visit(&node.then_children, out);
                    visit(&node.else_children, out);
                }
                TemplateNode::For(node) => visit(&node.children, out),
                _ => {}
            }
        }
    }
    let mut slots = Vec::new();
    if let Some(template) = &component.template {
        visit(&template.root, &mut slots);
    }
    slots
}

fn update_module_prop_override(
    namespace: &mut serde_json::Value,
    instance_id: Option<&str>,
    prop: &str,
    value: Option<serde_json::Value>,
) {
    if !namespace.is_object() {
        *namespace = serde_json::json!({});
    }
    let namespace_object = namespace.as_object_mut().expect("object established above");
    let props = namespace_object
        .entry("props".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !props.is_object() {
        *props = serde_json::json!({});
    }
    let props_object = props.as_object_mut().expect("object established above");
    if let Some(instance_id) = instance_id {
        let instances = props_object
            .entry("instances".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !instances.is_object() {
            *instances = serde_json::json!({});
        }
        let instances_object = instances.as_object_mut().expect("object established above");
        let instance = instances_object
            .entry(instance_id.to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !instance.is_object() {
            *instance = serde_json::json!({});
        }
        let instance_object = instance.as_object_mut().expect("object established above");
        match value {
            Some(value) => {
                instance_object.insert(prop.to_string(), value);
            }
            None => {
                instance_object.remove(prop);
            }
        }
        if instance_object.is_empty() {
            instances_object.remove(instance_id);
        }
        if instances_object.is_empty() {
            props_object.remove("instances");
        }
    } else {
        let global = props_object
            .entry("global".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !global.is_object() {
            *global = serde_json::json!({});
        }
        let global_object = global.as_object_mut().expect("object established above");
        match value {
            Some(value) => {
                global_object.insert(prop.to_string(), value);
            }
            None => {
                global_object.remove(prop);
            }
        }
        if global_object.is_empty() {
            props_object.remove("global");
        }
    }
    if props_object.is_empty() {
        namespace_object.remove("props");
    }
}
