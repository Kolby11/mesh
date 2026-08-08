use super::component::{FrontendCatalog, FrontendCatalogHandle, FrontendSurfaceComponent};
use super::*;
use rayon::prelude::*;
use std::collections::HashSet;

const BUILTIN_DEBUG_INSPECTOR_ID: &str = "@mesh/debug-inspector";

pub(in crate::shell) fn installed_module_graph_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_MODULE_GRAPH_PATH")
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("config/module.json")
}

/// Layer a profile's sparse preferences and per-instance surface overrides on
/// top of shared user defaults. The shared document remains untouched.
pub(in crate::shell) fn effective_profile_settings(
    shared: SettingsStore,
    profile: Option<&mesh_core_module::package::ShellProfile>,
) -> Result<SettingsStore, mesh_core_config::ConfigError> {
    let Some(profile) = profile else {
        return Ok(shared);
    };
    let path = shared.path().to_path_buf();
    let mut document = shared.to_value();
    let root = document
        .as_object_mut()
        .expect("SettingsStore always serializes an object");
    if let Some(theme) = &profile.resources.theme {
        let target = root
            .entry("shell".to_string())
            .or_insert_with(|| serde_json::json!({}));
        mesh_core_config::merge_json(target, &serde_json::json!({ "theme": { "active": theme } }));
    }
    for (namespace, overrides) in &profile.settings {
        let target = root
            .entry(namespace.clone())
            .or_insert_with(|| serde_json::json!({}));
        mesh_core_config::merge_json(target, overrides);
    }
    for (instance_id, instance) in profile.roots.iter().filter(|(_, root)| root.active) {
        let Some(surface) = &instance.surface else {
            continue;
        };
        let target = root
            .entry(instance_id.clone())
            .or_insert_with(|| serde_json::json!({}));
        mesh_core_config::merge_json(target, &serde_json::json!({ "surface": surface }));
    }
    SettingsStore::from_value(path, document)
}

fn builtin_state_contract(
    interface: &str,
    fields: &[(&str, &str)],
) -> mesh_core_service::InterfaceContract {
    builtin_contract(interface, fields, &[])
}

/// A core-provided contract: state fields the shell emits, plus the methods it
/// answers itself.
///
/// The shell is the provider for these interfaces, so the methods here are the
/// public, capability-gated way for any module to change composition and
/// configuration. Declaring them as a contract rather than a reserved channel
/// is what makes the settings frontend replaceable: the caller needs
/// `service.<name>.control`, not a particular module id.
fn builtin_contract(
    interface: &str,
    fields: &[(&str, &str)],
    methods: &[(&str, &[(&str, &str)])],
) -> mesh_core_service::InterfaceContract {
    mesh_core_service::InterfaceContract {
        interface: interface.to_string(),
        version: mesh_core_service::parse_contract_version("1.0")
            .expect("built-in interface version must be valid"),
        state_fields: fields
            .iter()
            .map(|(name, field_type)| mesh_core_service::ContractStateField {
                name: (*name).to_string(),
                field_type: (*field_type).to_string(),
                description: None,
            })
            .collect(),
        methods: methods
            .iter()
            .map(|(name, args)| mesh_core_service::InterfaceMethod {
                name: (*name).to_string(),
                args: args
                    .iter()
                    .map(|(arg, arg_type)| mesh_core_service::InterfaceArgument {
                        name: (*arg).to_string(),
                        arg_type: (*arg_type).to_string(),
                    })
                    .collect(),
                returns: Some("Result".to_string()),
                // These land in the shell synchronously; there is no backend
                // queue to coalesce against and no optimistic state to bind.
                coalesce: false,
                state_binding: None,
            })
            .collect(),
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: mesh_core_service::ContractCapabilities::default(),
    }
}

#[derive(Debug)]
pub(super) struct DiscoveredModuleManifest {
    pub(super) dir: PathBuf,
    pub(super) loaded:
        Result<mesh_core_module::LoadedManifest, mesh_core_module::manifest::ManifestError>,
}

pub(super) fn discover_shell_module_manifests(
    module_dirs: &[PathBuf],
) -> Vec<DiscoveredModuleManifest> {
    let manifest_dirs = discover_shell_module_manifest_dirs(module_dirs);
    load_shell_module_manifests(&manifest_dirs)
}

pub(super) fn discover_shell_module_manifest_dirs(module_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut discovered = Vec::new();
    for dir in module_dirs {
        if !dir.exists() {
            tracing::debug!("module directory does not exist: {}", dir.display());
            continue;
        }
        discovered.extend(discover_shell_module_manifest_dirs_under(dir));
    }
    discovered
}

fn discover_shell_module_manifest_dirs_under(dir: &Path) -> Vec<PathBuf> {
    if shell_module_manifest_exists(dir) {
        return vec![dir.to_path_buf()];
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("failed to read module directory {}: {e}", dir.display());
            return Vec::new();
        }
    };
    let mut child_dirs = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    child_dirs.sort();

    child_dirs
        .par_iter()
        .flat_map(|path| discover_shell_module_manifest_dirs_under(path))
        .collect()
}

fn shell_module_manifest_exists(dir: &Path) -> bool {
    dir.join("package.json").exists()
        || dir.join("module.json").exists()
        || dir.join("mesh.toml").exists()
}

pub(super) fn load_shell_module_manifests(
    module_dirs: &[PathBuf],
) -> Vec<DiscoveredModuleManifest> {
    module_dirs
        .par_iter()
        .map(|dir| DiscoveredModuleManifest {
            dir: dir.clone(),
            loaded: mesh_core_module::manifest::load_manifest(dir),
        })
        .collect()
}

#[cfg(test)]
pub(super) fn load_shell_module_manifests_serial(
    module_dirs: &[PathBuf],
) -> Vec<DiscoveredModuleManifest> {
    module_dirs
        .iter()
        .map(|dir| DiscoveredModuleManifest {
            dir: dir.clone(),
            loaded: mesh_core_module::manifest::load_manifest(dir),
        })
        .collect()
}

impl Shell {
    pub fn new() -> Self {
        let config_path = mesh_core_config::default_config_path();
        let config = load_config(&config_path).unwrap_or_else(|e| {
            tracing::warn!("failed to load config, using defaults: {e}");
            ShellConfig {
                shell: Default::default(),
            }
        });
        let shared_settings = SettingsStore::load().unwrap_or_else(|e| {
            tracing::warn!("failed to load settings, using defaults: {e}");
            SettingsStore::default()
        });
        let graph_path = installed_module_graph_path();
        let active_profile = mesh_core_module::package::ProfilePaths::from_root_graph(&graph_path)
            .and_then(|paths| paths.load_active())
            .unwrap_or_else(|error| {
                tracing::warn!("failed to load active shell profile: {error}");
                None
            });
        let active_profile_id = active_profile.as_ref().map(|(id, _)| id.clone());
        let settings_store = Arc::new(
            effective_profile_settings(
                shared_settings,
                active_profile.as_ref().map(|(_, profile)| profile),
            )
            .unwrap_or_else(|error| {
                tracing::warn!("failed to apply active profile settings: {error}");
                SettingsStore::default()
            }),
        );
        // A hand-edited file is the whole point of the store, so a bad value in
        // it is reported and skipped, never fatal: the shell starts on declared
        // defaults with the reason on stderr.
        mesh_core_config::log_settings_diagnostics("settings", settings_store.diagnostics());
        let settings = settings_store.shell().clone();

        // Discover and register XDG icon themes installed on the system.
        // Icon-pack binding modules reference them by name in their
        // mapping targets (`<theme>/<icon-name>`). Failures are logged
        // but non-fatal — hicolor fallback still works.
        for pack in mesh_core_icon::discover_xdg_packs() {
            let id = pack.id.clone();
            match mesh_core_icon::register_default_pack(pack) {
                Ok(true) => tracing::info!("registered XDG icon theme '{}'", id),
                Ok(false) => tracing::debug!("XDG icon theme '{}' already registered", id),
                Err(err) => {
                    tracing::warn!("failed to register XDG icon theme '{}': {err}", id)
                }
            }
        }
        mesh_core_icon::set_default_shell_pack(settings.icons.default_pack.clone());
        mesh_core_render::set_blur_quality(blur_quality_from_settings(&settings.render.blur));
        let (theme, theme_watch) = load_active_theme(&settings);
        let locale = LocaleEngine::with_fallback_locale(
            settings.i18n.locale.clone(),
            settings.i18n.fallback_locale.clone(),
        );
        let module_dirs = resolve_default_module_dirs(&config);
        let settings_watch = {
            let path = settings_store.path().to_path_buf();
            let modified_at = std::fs::metadata(&path)
                .ok()
                .and_then(|m| m.modified().ok());
            SettingsWatchState { path, modified_at }
        };

        let interfaces = InterfaceRegistry::new();
        interfaces.register_contract(builtin_contract(
            "mesh.theme",
            &[
                ("current", "string"),
                ("theme_id", "string"),
                ("is_dark", "boolean"),
                ("themes", "object[]"),
                ("available", "string[]"),
                ("system_resources", "object"),
            ],
            &[
                ("set_theme", &[("theme_id", "string")]),
                ("set_icon_theme", &[("theme_id", "string")]),
                ("set_font_family", &[("family", "string")]),
            ],
        ));
        // Locale writes stay on the `mesh.locale.set` host API, which already
        // enforces `locale.write`. A second, service-shaped way in would mean
        // two capability names for one operation.
        interfaces.register_contract(builtin_state_contract(
            "mesh.locale",
            &[("current", "string"), ("locale", "string")],
        ));
        interfaces.register_contract(builtin_contract(
            "mesh.settings",
            &[("revision", "string"), ("namespaces", "object")],
            &[
                (
                    "set_prop",
                    &[
                        ("module_id", "string"),
                        ("instance_id", "string?"),
                        ("prop", "string"),
                        ("value", "any"),
                    ],
                ),
                (
                    "unset_prop",
                    &[
                        ("module_id", "string"),
                        ("instance_id", "string?"),
                        ("prop", "string"),
                    ],
                ),
            ],
        ));
        interfaces.register_contract(builtin_contract(
            "mesh.packages",
            &[
                ("modules", "object[]"),
                ("providers", "object"),
                ("profiles", "string[]"),
                ("active_profile", "string"),
            ],
            &[
                (
                    "set_module_enabled",
                    &[("module_id", "string"), ("enabled", "boolean")],
                ),
                (
                    "set_provider",
                    &[("interface", "string"), ("provider_id", "string")],
                ),
                ("switch_profile", &[("profile_id", "string")]),
                (
                    "install",
                    &[
                        ("source", "string"),
                        ("profile_id", "string?"),
                        ("available_only", "boolean?"),
                        ("allow_elevated", "boolean?"),
                        ("allow_high", "boolean?"),
                    ],
                ),
                (
                    "uninstall",
                    &[("module_id", "string"), ("force", "boolean?")],
                ),
            ],
        ));
        interfaces.register(InterfaceProvider {
            interface: mesh_core_debug::DEBUG_INTERFACE.to_string(),
            version: Some("1.0".to_string()),
            base_module: Some("@mesh/debug".to_string()),
            provider_module: mesh_core_debug::DEBUG_SOURCE_MODULE_ID.to_string(),
            backend_name: "Shell".to_string(),
            priority: 100,
        });
        interfaces.register(InterfaceProvider {
            interface: "mesh.theme".to_string(),
            version: Some("1.0".to_string()),
            base_module: Some("@mesh/theme-interface".to_string()),
            provider_module: "@mesh/shell".to_string(),
            backend_name: "Shell Theme".to_string(),
            priority: 200,
        });
        interfaces.register(InterfaceProvider {
            interface: "mesh.locale".to_string(),
            version: Some("1.0".to_string()),
            base_module: Some("@mesh/locale-interface".to_string()),
            provider_module: "@mesh/shell".to_string(),
            backend_name: "Shell Locale".to_string(),
            priority: 200,
        });
        interfaces.register(InterfaceProvider {
            interface: "mesh.settings".to_string(),
            version: Some("1.0".to_string()),
            base_module: Some("@mesh/settings-interface".to_string()),
            provider_module: "@mesh/shell".to_string(),
            backend_name: "Shell Settings Store".to_string(),
            priority: 200,
        });
        interfaces.register(InterfaceProvider {
            interface: "mesh.packages".to_string(),
            version: Some("1.0".to_string()),
            base_module: Some("@mesh/packages-interface".to_string()),
            provider_module: "@mesh/shell".to_string(),
            backend_name: "Shell Package Graph".to_string(),
            priority: 200,
        });

        let now = std::time::Instant::now();

        Self {
            config,
            settings,
            settings_store,
            theme,
            locale,
            events: EventBus::new(),
            diagnostics: DiagnosticsCollector::new(),
            services: ServiceRegistry::new(),
            interfaces,
            installed_module_graph: None,
            active_profile_id,
            modules: HashMap::new(),
            frontend_catalog: FrontendCatalogHandle::default(),
            module_dirs,
            core: ShellCoreState::default(),
            components: Vec::new(),
            components_want_render: false,
            presented_last_frame: true,
            component_by_surface: HashMap::new(),
            service_delivery_index: ServiceDeliveryIndex::default(),
            surfaces: HashMap::new(),
            clipboard: Box::new(WaylandClipboard::default()),
            presentation_engine: PresentationEngine::select(),
            eventfd_fd: None,
            theme_watch,
            settings_watch,
            next_theme_reload_check: now,
            next_shell_settings_reload_check: now,
            next_frontend_reload_check: now,
            file_watcher_active: false,
            debug: DebugOverlayState::default(),
            debug_overlay: DebugOverlay::new(),
            active_key_modifiers: KeyModifiers::default(),
            keyboard_focus_surface: None,
            pending_wayland_events: VecDeque::new(),
            transfer_owned_keyboard_modes: HashMap::new(),
            service_handlers: HashMap::new(),
            backend_runtimes: HashMap::new(),
            pending_backend_runtimes: HashMap::new(),
            pending_profile_switch: None,
            deferred_requests: VecDeque::new(),
            backend_runtime_statuses: HashMap::new(),
            backend_supervision: HashMap::new(),
            backend_respawn: None,
            latest_service_state: HashMap::new(),
            service_contract_validation: HashMap::new(),
            pending_bound_service_state: HashMap::new(),
            command_throttle: HashMap::new(),
            pending_popover_hides: HashMap::new(),
            profiling: runtime::profiling::ProfilingRuntimeState::default(),
        }
    }

    pub fn discover_modules(&mut self) {
        let module_dirs = std::mem::take(&mut self.module_dirs);
        let discovered = discover_shell_module_manifests(&module_dirs);
        for discovered in discovered {
            match discovered.loaded {
                Ok(loaded) => self.register_loaded_module(&discovered.dir, loaded),
                Err(e) => tracing::warn!("failed to load module {}: {e}", discovered.dir.display()),
            }
        }
        self.module_dirs = module_dirs;
        self.register_installed_graph_interfaces();
        tracing::info!("discovered {} modules", self.modules.len());
    }

    pub(in crate::shell) fn installed_module_graph_path(&self) -> PathBuf {
        installed_module_graph_path()
    }

    pub(in crate::shell) fn load_installed_module_graph_cached(
        &mut self,
    ) -> Result<&InstalledModuleGraph, mesh_core_module::package::ModuleManifestError> {
        if self.installed_module_graph.is_none() {
            let graph_path = self.installed_module_graph_path();
            self.installed_module_graph = Some(load_installed_module_graph(&graph_path)?);
        }
        Ok(self
            .installed_module_graph
            .as_ref()
            .expect("installed module graph was just loaded"))
    }

    fn register_installed_graph_interfaces(&mut self) {
        let graph_path = self.installed_module_graph_path();
        let graph = match self.load_installed_module_graph_cached() {
            Ok(graph) => graph.clone(),
            Err(err) => {
                tracing::warn!(
                    "failed to load installed module graph from {}; keeping discovered interfaces only: {err}",
                    graph_path.display()
                );
                return;
            }
        };
        self.register_interfaces_from_graph(&graph);
    }

    pub(in crate::shell) fn register_interfaces_from_graph(
        &mut self,
        graph: &InstalledModuleGraph,
    ) {
        for contract in graph.interface_contracts().values() {
            self.interfaces.register_contract(contract.clone());
        }

        for provider in graph.backend_provider_contributions() {
            self.interfaces.register(InterfaceProvider {
                interface: canonical_interface_name(&provider.interface),
                version: provider.version.clone(),
                base_module: provider.base_module.clone(),
                provider_module: provider.module_id.clone(),
                backend_name: provider
                    .provider
                    .clone()
                    .unwrap_or_else(|| provider.module_id.clone()),
                priority: provider.priority,
            });
        }
    }

    fn register_loaded_module(&mut self, dir: &Path, loaded: mesh_core_module::LoadedManifest) {
        let id = loaded.manifest.package.id.clone();
        // Register declared contracts: `mesh.interface` on interface modules
        // and inline `mesh.interfaces` on backend modules.
        let declared_contract_sections = loaded
            .manifest
            .interface
            .iter()
            .filter(|_| loaded.manifest.package.module_type == ModuleType::Interface)
            .chain(loaded.manifest.interfaces.iter());
        for section in declared_contract_sections {
            let Some(contract_value) = &section.contract else {
                continue;
            };
            match parse_interface_contract(&section.name, &section.version, contract_value) {
                Ok(contract) => self.interfaces.register_contract(contract),
                Err(err) => tracing::warn!(
                    "invalid interface contract {} in module {}: {err}",
                    section.name,
                    id
                ),
            }
        }
        for provided in loaded.manifest.declared_provides() {
            self.interfaces.register(InterfaceProvider {
                interface: canonical_interface_name(&provided.interface),
                version: provided.version.clone(),
                base_module: provided.base_module.clone(),
                provider_module: id.clone(),
                backend_name: provided.backend_name.clone().unwrap_or_else(|| id.clone()),
                priority: provided.priority,
            });
        }
        tracing::info!(
            "discovered module: {} v{} ({}) from {}",
            id,
            loaded.manifest.package.version,
            loaded.manifest.package.module_type,
            loaded.source
        );
        register_module_icon_pack(&id, dir, loaded.manifest.assets.as_ref());
        register_icon_pack_module(&id, dir, loaded.manifest.icon_pack.as_ref());
        // Per-module icon overrides live in the module's own settings
        // namespace, alongside its surface and prop overrides.
        let icon_overrides =
            ModuleSettingsOverrides::from_namespace(&self.settings_store.namespace(&id));
        register_frontend_icon_bindings(&id, &loaded.manifest, icon_overrides.icons.as_ref());
        self.modules.insert(
            id,
            ModuleInstance::new(
                loaded.manifest,
                dir.to_path_buf(),
                loaded.path,
                loaded.source,
            ),
        );
    }

    pub fn resolve_modules(&mut self) -> Result<(), ShellRunError> {
        validate_module_dependency_graph(self.modules.values().map(|module| &module.manifest))?;
        let ids: Vec<String> = self.modules.keys().cloned().collect();
        for id in ids {
            if let Some(module) = self.modules.get_mut(&id) {
                if module.state == ModuleState::Discovered {
                    if let Err(e) = module.transition(ModuleState::Resolved) {
                        tracing::warn!("failed to resolve module {id}: {e}");
                    }
                }
            }
        }
        Ok(())
    }

    pub fn module(&self, id: &str) -> Option<&ModuleInstance> {
        self.modules.get(id)
    }

    pub fn modules(&self) -> impl Iterator<Item = (&str, ModuleState)> {
        self.modules
            .iter()
            .map(|(id, inst)| (id.as_str(), inst.state))
    }

    pub(super) fn load_frontend_components(&mut self) -> Result<(), ShellRunError> {
        if !self.components.is_empty() {
            return Ok(());
        }

        let graph = self.load_installed_module_graph_cached().ok().cloned();
        let frontend_catalog = FrontendCatalog::from_modules(&self.modules, graph.as_ref())?;
        self.frontend_catalog.replace(frontend_catalog, None);
        let frontend_catalog = self.frontend_catalog.snapshot().catalog;
        let enabled_frontends = self.installed_enabled_frontend_ids();
        let graph_i18n_catalogs = self.graph_i18n_catalog_paths();
        let interface_catalog = std::sync::Arc::new(self.interfaces.catalog());
        if let Some(profile_id) = self.active_profile_id.clone() {
            let paths = mesh_core_module::package::ProfilePaths::from_root_graph(
                &self.installed_module_graph_path(),
            )?;
            let profile = paths.load(&profile_id)?;
            let entries = frontend_catalog
                .top_level_surfaces()
                .into_iter()
                .map(|entry| (entry.compiled.manifest.package.id.clone(), entry))
                .collect::<HashMap<_, _>>();
            for (instance_id, root) in profile.roots.iter().filter(|(_, root)| root.active) {
                let Some(entry) = entries.get(&root.module) else {
                    continue;
                };
                self.register_component(Box::new(
                    FrontendSurfaceComponent::new(
                        entry.compiled.clone(),
                        entry.module_dir.clone(),
                        self.frontend_catalog.clone(),
                        interface_catalog.clone(),
                        self.settings_store.clone(),
                    )
                    .with_instance_id(instance_id)
                    .with_graph_i18n_catalogs(graph_i18n_catalogs.clone()),
                ));
            }
            return Ok(());
        }
        for entry in frontend_catalog.top_level_surfaces_filtered(enabled_frontends.as_ref()) {
            self.register_component(Box::new(
                FrontendSurfaceComponent::new(
                    entry.compiled,
                    entry.module_dir,
                    self.frontend_catalog.clone(),
                    interface_catalog.clone(),
                    self.settings_store.clone(),
                )
                .with_graph_i18n_catalogs(graph_i18n_catalogs.clone()),
            ));
        }

        Ok(())
    }

    pub(in crate::shell) fn activate_frontend_module(
        &mut self,
        module_id: &str,
        graph: &InstalledModuleGraph,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let previous_catalog = self.frontend_catalog.snapshot().catalog;
        let catalog = FrontendCatalog::from_modules_reusing(
            &self.modules,
            Some(graph),
            Some(&previous_catalog),
        )?;
        let entry = catalog
            .top_level_surfaces()
            .into_iter()
            .find(|entry| entry.compiled.manifest.package.id == module_id);
        let previous_catalog = self.frontend_catalog.replace(catalog, None);

        let Some(entry) = entry else {
            // Widgets and component-only frontend packages own no surface.
            self.sync_frontend_catalog_components();
            return Ok(VecDeque::new());
        };
        let instance_ids = if let Some(profile_id) = &self.active_profile_id {
            let paths = mesh_core_module::package::ProfilePaths::from_root_graph(
                &self.installed_module_graph_path(),
            )?;
            paths
                .load(profile_id)?
                .roots
                .into_iter()
                .filter(|(_, root)| root.active && root.module == module_id)
                .map(|(instance_id, _)| instance_id)
                .collect::<Vec<_>>()
        } else {
            vec![entry.compiled.surface_id().to_string()]
        };
        let existing = self
            .components
            .iter()
            .map(|runtime| runtime.surface_id.clone())
            .collect::<HashSet<_>>();
        let mounted = (|| {
            let interface_catalog = std::sync::Arc::new(self.interfaces.catalog());
            let mut mounted = Vec::new();
            for instance_id in instance_ids {
                if existing.contains(&instance_id) {
                    continue;
                }
                let mut component = FrontendSurfaceComponent::new(
                    entry.compiled.clone(),
                    entry.module_dir.clone(),
                    self.frontend_catalog.clone(),
                    interface_catalog.clone(),
                    self.settings_store.clone(),
                )
                .with_instance_id(&instance_id)
                .with_graph_i18n_catalogs(self.graph_i18n_catalog_paths());
                let diagnostics = self.diagnostics.register(module_id.to_string());
                let mut requests = VecDeque::from(
                    component
                        .mount(ComponentContext {
                            component_id: module_id.to_string(),
                            surface_id: instance_id,
                            diagnostics,
                        })
                        .map_err(ShellRunError::Component)?,
                );
                component
                    .locale_changed(&self.locale)
                    .map_err(ShellRunError::Component)?;
                for state in self.latest_service_state.values() {
                    let event = ServiceEvent::Updated {
                        service: state.interface.clone(),
                        source_module: state.provider_id.clone(),
                        payload: state.state.clone(),
                    };
                    if component.observes_service_event(&event) {
                        requests.extend(
                            component
                                .handle_service_event(&event)
                                .map_err(ShellRunError::Component)?,
                        );
                    }
                }
                mounted.push((component, requests));
            }
            Ok::<_, ShellRunError>(mounted)
        })();
        let mounted = match mounted {
            Ok(mounted) => mounted,
            Err(error) => {
                self.frontend_catalog.restore(previous_catalog);
                return Err(error);
            }
        };
        let mut requests = VecDeque::new();
        for (component, component_requests) in mounted {
            requests.extend(component_requests);
            self.register_component(Box::new(component));
        }
        self.sync_frontend_catalog_components();
        tracing::info!(module_id, "activated frontend module live");
        Ok(requests)
    }

    pub(in crate::shell) fn deactivate_frontend_module(
        &mut self,
        module_id: &str,
        graph: Option<&InstalledModuleGraph>,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let previous_catalog = self.frontend_catalog.snapshot().catalog;
        let catalog =
            FrontendCatalog::from_modules_reusing(&self.modules, graph, Some(&previous_catalog))?;
        self.frontend_catalog.replace(catalog, None);
        self.sync_frontend_catalog_components();

        let indices = self
            .components
            .iter()
            .enumerate()
            .filter(|(_, runtime)| runtime.component.id() == module_id)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if indices.is_empty() {
            return Ok(VecDeque::new());
        }
        let mut removed_surfaces = Vec::new();
        for index in indices.into_iter().rev() {
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
            removed_surfaces.push(surface_id);
        }
        self.rebuild_component_surface_index();
        self.service_delivery_index.mark_dirty();
        tracing::info!(module_id, "deactivated frontend module live");
        let mut requests = VecDeque::new();
        for surface_id in removed_surfaces {
            match self.broadcast_core_event(CoreEvent::SurfaceVisibilityChanged {
                surface_id,
                visible: false,
            }) {
                Ok(next) => requests.extend(next),
                Err(error) => tracing::warn!(
                    module_id,
                    "frontend was disabled but its visibility notification failed: {error}"
                ),
            }
        }
        Ok(requests)
    }

    pub(in crate::shell) fn sync_frontend_catalog_components(&mut self) -> bool {
        let mut invalidated = false;
        for runtime in &mut self.components {
            invalidated |= runtime.component.frontend_catalog_changed();
        }
        if invalidated {
            self.service_delivery_index.mark_dirty();
            self.components_want_render = true;
        }
        invalidated
    }

    fn graph_i18n_catalog_paths(&self) -> Vec<(String, String, PathBuf)> {
        let Some(graph) = self.installed_module_graph.as_ref() else {
            return Vec::new();
        };
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

    fn installed_enabled_frontend_ids(&mut self) -> Option<HashSet<String>> {
        let graph_path = self.installed_module_graph_path();
        match self.load_installed_module_graph_cached() {
            Ok(graph) => {
                let mut enabled = graph
                    .frontend_modules()
                    .into_iter()
                    .map(|module| module.id.clone())
                    .collect::<HashSet<_>>();
                enabled.insert(BUILTIN_DEBUG_INSPECTOR_ID.to_string());
                Some(enabled)
            }
            Err(err) => {
                tracing::warn!(
                    "failed to load installed module graph from {}; using all discovered frontend modules: {err}",
                    graph_path.display()
                );
                None
            }
        }
    }

    pub(super) fn register_component(&mut self, component: Box<dyn ShellComponent>) {
        let surface_id = component.surface_id().to_string();
        let initial_visibility = component
            .initial_visibility()
            .unwrap_or_else(default_surface_visibility);
        self.core
            .surfaces
            .entry(surface_id.clone())
            .or_insert_with(|| SurfaceState {
                visible: initial_visibility,
                closing_until: None,
            });
        self.surfaces.entry(surface_id.clone()).or_default();
        let component_index = self.components.len();
        self.components.push(ComponentRuntime::new(component));
        self.component_by_surface
            .insert(surface_id, component_index);
        self.service_delivery_index.mark_dirty();
    }

    pub(super) fn mount_components(&mut self) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            let diagnostics = self
                .diagnostics
                .register(runtime.component.id().to_string());
            let ctx = ComponentContext {
                component_id: runtime.component.id().to_string(),
                surface_id: runtime.surface_id.clone(),
                diagnostics,
            };
            requests.extend(
                runtime
                    .component
                    .mount(ctx)
                    .map_err(ShellRunError::Component)?,
            );
        }
        // Mount first so module scripts can establish their service proxy;
        // then deliver the revisioned effective settings snapshot normally.
        requests.extend(self.sync_settings_service_state()?);
        self.service_delivery_index.mark_dirty();
        Ok(requests)
    }
}
