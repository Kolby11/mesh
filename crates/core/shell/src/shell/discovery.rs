use super::component::{FrontendCatalog, FrontendSurfaceComponent};
use super::*;
use std::collections::HashSet;

const BUILTIN_DEBUG_INSPECTOR_ID: &str = "@mesh/debug-inspector";

impl Shell {
    pub fn new() -> Self {
        let config_path = mesh_core_config::default_config_path();
        let config = load_config(&config_path).unwrap_or_else(|e| {
            tracing::warn!("failed to load config, using defaults: {e}");
            ShellConfig {
                shell: Default::default(),
                modules: HashMap::new(),
            }
        });
        let settings = load_shell_settings().unwrap_or_else(|e| {
            tracing::warn!("failed to load shell settings, using defaults: {e}");
            ShellSettings::default()
        });

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
        let (theme, theme_watch) = load_active_theme(&settings);
        let locale = LocaleEngine::with_fallback_locale(
            settings.i18n.locale.clone(),
            settings.i18n.fallback_locale.clone(),
        );
        let module_dirs = resolve_default_module_dirs(&config);
        let settings_watch = {
            let path = default_settings_path();
            let modified_at = std::fs::metadata(&path)
                .ok()
                .and_then(|m| m.modified().ok());
            SettingsWatchState { path, modified_at }
        };

        let interfaces = InterfaceRegistry::new();
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

        let now = std::time::Instant::now();

        Self {
            config,
            settings,
            theme,
            locale,
            events: EventBus::new(),
            diagnostics: DiagnosticsCollector::new(),
            services: ServiceRegistry::new(),
            interfaces,
            installed_module_graph: None,
            modules: HashMap::new(),
            module_dirs,
            core: ShellCoreState::default(),
            components: Vec::new(),
            components_want_render: false,
            presented_last_frame: true,
            component_by_surface: HashMap::new(),
            surfaces: HashMap::new(),
            clipboard: Box::new(WaylandClipboard::default()),
            presentation_engine: PresentationEngine::select(),
            eventfd_fd: None,
            theme_watch,
            settings_watch,
            next_theme_reload_check: now,
            next_shell_settings_reload_check: now,
            next_frontend_reload_check: now,
            next_module_settings_reload_check: now,
            file_watcher_active: false,
            debug: DebugOverlayState::default(),
            debug_overlay: DebugOverlay::new(),
            active_key_modifiers: KeyModifiers::default(),
            keyboard_focus_surface: None,
            pending_wayland_events: VecDeque::new(),
            transfer_owned_keyboard_modes: HashMap::new(),
            service_handlers: HashMap::new(),
            backend_runtimes: HashMap::new(),
            backend_runtime_statuses: HashMap::new(),
            backend_supervision: HashMap::new(),
            backend_respawn: None,
            latest_service_state: HashMap::new(),
            pending_optimistic_state: HashMap::new(),
            command_throttle: HashMap::new(),
            pending_popover_hides: HashMap::new(),
            profiling: runtime::profiling::ProfilingRuntimeState::default(),
        }
    }

    pub fn discover_modules(&mut self) {
        let module_dirs = std::mem::take(&mut self.module_dirs);
        for dir in &module_dirs {
            if !dir.exists() {
                tracing::debug!("module directory does not exist: {}", dir.display());
                continue;
            }
            self.scan_module_dir(dir);
        }
        self.module_dirs = module_dirs;
        self.register_installed_graph_interfaces();
        tracing::info!("discovered {} modules", self.modules.len());
    }

    pub(in crate::shell) fn installed_module_graph_path(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join("config/module.json")
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

    fn scan_module_dir(&mut self, dir: &Path) {
        let has_manifest = dir.join("package.json").exists()
            || dir.join("module.json").exists()
            || dir.join("mesh.toml").exists();
        let has_module_manifest = dir.join("module.json").exists();
        if has_manifest || has_module_manifest {
            match mesh_core_module::manifest::load_manifest(dir) {
                Ok(loaded) => {
                    let id = loaded.manifest.package.id.clone();
                    // Register declared contracts: `mesh.interface` on interface
                    // modules and inline `mesh.interfaces` on backend modules.
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
                        match parse_interface_contract(
                            &section.name,
                            &section.version,
                            contract_value,
                        ) {
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
                            backend_name: provided
                                .backend_name
                                .clone()
                                .unwrap_or_else(|| id.clone()),
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
                    register_frontend_icon_bindings(
                        &id,
                        &loaded.manifest,
                        self.settings
                            .modules
                            .get(&id)
                            .and_then(|m| m.icons.as_ref()),
                    );
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
                Err(e) => tracing::warn!("failed to load module {}: {e}", dir.display()),
            }
            return;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!("failed to read module directory {}: {e}", dir.display());
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.scan_module_dir(&path);
            }
        }
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
        let frontend_catalog = std::sync::Arc::new(FrontendCatalog::from_modules(
            &self.modules,
            graph.as_ref(),
        )?);
        let enabled_frontends = self.installed_enabled_frontend_ids();
        let graph_i18n_catalogs = self.graph_i18n_catalog_paths();
        let interface_catalog = std::sync::Arc::new(self.interfaces.catalog());
        for entry in frontend_catalog.top_level_surfaces_filtered(enabled_frontends.as_ref()) {
            self.register_component(Box::new(
                FrontendSurfaceComponent::new(
                    entry.compiled,
                    entry.module_dir,
                    frontend_catalog.clone(),
                    interface_catalog.clone(),
                )
                .with_graph_i18n_catalogs(graph_i18n_catalogs.clone()),
            ));
        }

        Ok(())
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
        Ok(requests)
    }
}
