use super::*;
use super::component::{FrontendCatalog, FrontendSurfaceComponent};

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

        Self {
            config,
            settings,
            theme,
            locale,
            events: EventBus::new(),
            diagnostics: DiagnosticsCollector::new(),
            services: ServiceRegistry::new(),
            interfaces: InterfaceRegistry::new(),
            modules: HashMap::new(),
            module_dirs,
            core: ShellCoreState::default(),
            components: Vec::new(),
            surfaces: HashMap::new(),
            clipboard: Box::new(WaylandClipboard::default()),
            render_engine: RenderEngine::select(),
            theme_watch,
            settings_watch,
            debug: DebugOverlayState::default(),
            debug_overlay: DebugOverlay::new(),
            active_key_modifiers: KeyModifiers::default(),
            service_handlers: HashMap::new(),
            backend_runtimes: HashMap::new(),
            backend_runtime_statuses: HashMap::new(),
            latest_service_state: HashMap::new(),
        }
    }

    pub fn discover_modules(&mut self) {
        for dir in self.module_dirs.clone() {
            if !dir.exists() {
                tracing::debug!("module directory does not exist: {}", dir.display());
                continue;
            }
            self.scan_module_dir(&dir);
        }
        tracing::info!("discovered {} modules", self.modules.len());
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
                    if loaded.manifest.package.module_type == ModuleType::Interface {
                        if let Some(interface) = &loaded.manifest.interface {
                            match load_interface_contract(
                                dir,
                                &interface.name,
                                &interface.version,
                                &interface.file,
                            ) {
                                Ok(contract) => self.interfaces.register_contract(contract),
                                Err(err) => tracing::warn!(
                                    "failed to load interface contract for module {}: {err}",
                                    id
                                ),
                            }
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
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
                && path
                    .parent()
                    .and_then(|parent| parent.file_name())
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == "interfaces")
            {
                self.scan_interface_file(&path);
            }
        }
    }

    fn scan_interface_file(&mut self, path: &Path) {
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            return;
        };
        let interface_name = canonical_interface_name(stem);
        match load_interface_contract(
            path.parent().unwrap_or_else(|| Path::new(".")),
            &interface_name,
            "1.0",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
        ) {
            Ok(contract) => {
                tracing::info!(
                    "discovered interface contract: {} from {}",
                    contract.interface,
                    path.display()
                );
                self.interfaces.register_contract(contract);
            }
            Err(err) => tracing::warn!(
                "failed to load interface contract {} from {}: {err}",
                interface_name,
                path.display()
            ),
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

        let frontend_catalog = FrontendCatalog::from_modules(&self.modules)?;
        for entry in frontend_catalog.top_level_surfaces() {
            self.register_component(Box::new(FrontendSurfaceComponent::new(
                entry.compiled,
                entry.module_dir,
                frontend_catalog.clone(),
                self.interfaces.catalog(),
            )));
        }

        Ok(())
    }

    fn register_component(&mut self, component: Box<dyn ShellComponent>) {
        let surface_id = component.surface_id().to_string();
        let initial_visibility = component
            .initial_visibility()
            .unwrap_or_else(default_surface_visibility);
        self.core
            .surfaces
            .entry(surface_id.clone())
            .or_insert_with(|| SurfaceState {
                visible: initial_visibility,
            });
        self.surfaces.entry(surface_id.clone()).or_default();
        self.components.push(ComponentRuntime::new(component));
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
