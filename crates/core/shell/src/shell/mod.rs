use mesh_core_config::{
    ShellConfig, ShellSettings, default_settings_path, load_config, load_shell_settings,
};
use mesh_core_debug::{
    BackendRuntimeEntry, DebugOverlayState, DebugSnapshot, HealthEntry, InterfaceEntry,
    PluginEntry, ProviderEntry,
};
use mesh_core_diagnostics::DiagnosticsCollector;
use mesh_core_events::EventBus;
use mesh_core_locale::LocaleEngine;
use mesh_core_plugin::lifecycle::{PluginInstance, PluginState};
use mesh_core_plugin::package::{InstalledModuleGraph, ModuleKind, load_installed_module_graph};
use mesh_core_plugin::{DependencyGraphError, PluginType, validate_plugin_dependency_graph};
use mesh_core_service::{
    InterfaceContract, InterfaceProvider, InterfaceRegistry, ServiceRegistry,
    canonical_interface_name, load_interface_contract,
};
use mesh_core_theme::ThemeEngine;
use mesh_core_wayland::{Layer, StubSurface};

use std::collections::{HashMap, VecDeque};
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

mod component;
mod ipc;
mod layout;
mod service;
mod sounds;
mod surface_layout;
mod types;

use component::{FrontendCatalog, FrontendSurfaceComponent};
use ipc::spawn_ipc_server;
use mesh_core_backend::{BackendServiceEvent, spawn_backend_service};
use mesh_core_render::{
    DebugOverlay, LayerSurfaceConfig, PixelBuffer, RenderEngine, WindowEvent, WindowKeyEvent,
    coalesce_pointer_moves, event_surface_id,
};
use sounds::{SoundKind, play_shell_sound};
use surface_layout::{default_surface_visibility, load_active_theme};
pub use types::{
    ComponentContext, ComponentError, ComponentInput, CoreEvent, CoreRequest, ServiceEvent,
    ShellComponent, SurfaceId,
};
use types::{
    ComponentRuntime, LatestServiceState, ServiceCommandMsg, SettingsWatchState, ShellCoreState,
    ShellMessage, SurfaceState, ThemeWatchState,
};

use service::{service_command_control_capability, service_name_from_interface};

pub struct Shell {
    pub config: ShellConfig,
    pub settings: ShellSettings,
    pub theme: ThemeEngine,
    pub locale: LocaleEngine,
    pub events: EventBus,
    pub diagnostics: DiagnosticsCollector,
    pub services: ServiceRegistry,
    pub interfaces: InterfaceRegistry,
    plugins: HashMap<String, PluginInstance>,
    plugin_dirs: Vec<PathBuf>,
    core: ShellCoreState,
    components: Vec<ComponentRuntime>,
    surfaces: HashMap<SurfaceId, StubSurface>,
    render_engine: RenderEngine,
    theme_watch: ThemeWatchState,
    settings_watch: SettingsWatchState,
    debug: DebugOverlayState,
    debug_overlay: DebugOverlay,
    service_handlers: HashMap<String, mpsc::UnboundedSender<ServiceCommandMsg>>,
    backend_runtimes: HashMap<String, BackendRuntimeSlot>,
    backend_runtime_statuses: HashMap<(String, String), BackendRuntimeStatusEntry>,
    latest_service_state: HashMap<String, LatestServiceState>,
}

#[derive(Debug, Clone)]
struct BackendRuntimeSlot {
    interface: String,
    provider_id: String,
    command_tx: mpsc::UnboundedSender<ServiceCommandMsg>,
    task: AbortHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendRuntimeStatus {
    NoActiveProvider,
    UnmetBackendRequirement,
    InvalidManifest,
    MissingEntrypoint,
    MissingBinary,
    InitFailed,
    Running,
    PollFailed,
    Failed,
    Stopped,
}

impl BackendRuntimeStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::NoActiveProvider => "no_active_provider",
            Self::UnmetBackendRequirement => "unmet_backend_requirement",
            Self::InvalidManifest => "invalid_manifest",
            Self::MissingEntrypoint => "missing_entrypoint",
            Self::MissingBinary => "missing_binary",
            Self::InitFailed => "init_failed",
            Self::Running => "running",
            Self::PollFailed => "poll_failed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }

    fn from_str(status: &str) -> Self {
        match status {
            "no_active_provider" => Self::NoActiveProvider,
            "unmet_backend_requirement" => Self::UnmetBackendRequirement,
            "invalid_manifest" => Self::InvalidManifest,
            "missing_entrypoint" => Self::MissingEntrypoint,
            "missing_binary" => Self::MissingBinary,
            "init_failed" => Self::InitFailed,
            "running" => Self::Running,
            "poll_failed" => Self::PollFailed,
            "stopped" => Self::Stopped,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug, Clone)]
struct BackendRuntimeStatusEntry {
    interface: String,
    provider_id: String,
    status: BackendRuntimeStatus,
    message: String,
}

#[derive(Debug, Clone)]
struct BackendLaunchCandidate {
    module_id: String,
    interface: String,
    service_name: String,
    entrypoint_path: PathBuf,
    script_source: String,
    capabilities: Vec<String>,
    settings: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendLifecycleStatusRecord {
    interface: String,
    provider_id: Option<String>,
    status: &'static str,
    message: String,
}

pub fn default_ipc_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_IPC_SOCKET") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join("mesh.sock");
    }

    let uid = std::env::var("UID").unwrap_or_else(|_| "unknown".to_string());
    PathBuf::from("/tmp").join(format!("mesh-{uid}.sock"))
}

impl Shell {
    pub fn new() -> Self {
        let config_path = mesh_core_config::default_config_path();
        let config = load_config(&config_path).unwrap_or_else(|e| {
            tracing::warn!("failed to load config, using defaults: {e}");
            ShellConfig {
                shell: Default::default(),
                plugins: HashMap::new(),
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
            plugins: HashMap::new(),
            plugin_dirs: default_plugin_dirs(),
            core: ShellCoreState::default(),
            components: Vec::new(),
            surfaces: HashMap::new(),
            render_engine: RenderEngine::select(),
            theme_watch,
            settings_watch,
            debug: DebugOverlayState::default(),
            debug_overlay: DebugOverlay::new(),
            service_handlers: HashMap::new(),
            backend_runtimes: HashMap::new(),
            backend_runtime_statuses: HashMap::new(),
            latest_service_state: HashMap::new(),
        }
    }

    pub fn discover_plugins(&mut self) {
        for dir in self.plugin_dirs.clone() {
            if !dir.exists() {
                tracing::debug!("plugin directory does not exist: {}", dir.display());
                continue;
            }
            self.scan_plugin_dir(&dir);
        }
        tracing::info!("discovered {} plugins", self.plugins.len());
    }

    fn scan_plugin_dir(&mut self, dir: &Path) {
        let has_manifest = dir.join("plugin.json").exists() || dir.join("mesh.toml").exists();
        if has_manifest {
            match mesh_core_plugin::manifest::load_manifest(dir) {
                Ok(loaded) => {
                    let id = loaded.manifest.package.id.clone();
                    if loaded.manifest.package.plugin_type == PluginType::Interface {
                        if let Some(interface) = &loaded.manifest.interface {
                            match load_interface_contract(
                                dir,
                                &interface.name,
                                &interface.version,
                                &interface.file,
                            ) {
                                Ok(contract) => self.interfaces.register_contract(contract),
                                Err(err) => tracing::warn!(
                                    "failed to load interface contract for plugin {}: {err}",
                                    id
                                ),
                            }
                        }
                    }
                    for provided in loaded.manifest.declared_provides() {
                        self.interfaces.register(InterfaceProvider {
                            interface: canonical_interface_name(&provided.interface),
                            version: provided.version.clone(),
                            base_plugin: provided.base_plugin.clone(),
                            provider_plugin: id.clone(),
                            backend_name: provided
                                .backend_name
                                .clone()
                                .unwrap_or_else(|| id.clone()),
                            priority: provided.priority,
                        });
                    }
                    tracing::info!(
                        "discovered plugin: {} v{} ({}) from {}",
                        id,
                        loaded.manifest.package.version,
                        loaded.manifest.package.plugin_type,
                        loaded.source
                    );
                    self.plugins.insert(
                        id,
                        PluginInstance::new(
                            loaded.manifest,
                            dir.to_path_buf(),
                            loaded.path,
                            loaded.source,
                        ),
                    );
                }
                Err(e) => tracing::warn!("failed to load plugin {}: {e}", dir.display()),
            }
            return;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!("failed to read plugin directory {}: {e}", dir.display());
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.scan_plugin_dir(&path);
            }
        }
    }

    pub fn resolve_plugins(&mut self) -> Result<(), ShellRunError> {
        validate_plugin_dependency_graph(self.plugins.values().map(|plugin| &plugin.manifest))?;
        let ids: Vec<String> = self.plugins.keys().cloned().collect();
        for id in ids {
            if let Some(plugin) = self.plugins.get_mut(&id) {
                if plugin.state == PluginState::Discovered {
                    if let Err(e) = plugin.transition(PluginState::Resolved) {
                        tracing::warn!("failed to resolve plugin {id}: {e}");
                    }
                }
            }
        }
        Ok(())
    }

    pub fn plugin(&self, id: &str) -> Option<&PluginInstance> {
        self.plugins.get(id)
    }

    fn build_debug_snapshot(&self) -> DebugSnapshot {
        let plugins = self
            .plugins
            .values()
            .map(|inst| PluginEntry {
                id: inst.manifest.package.id.clone(),
                plugin_type: format!("{:?}", inst.manifest.package.plugin_type).to_lowercase(),
                state: inst.state.to_string(),
                error_count: inst.error_count,
                last_error: inst.last_error.clone(),
            })
            .collect();

        let catalog = self.interfaces.catalog();
        let mut interfaces: Vec<InterfaceEntry> = catalog
            .providers
            .iter()
            .map(|(name, providers)| {
                let providers = providers
                    .iter()
                    .map(|p| ProviderEntry {
                        backend_name: p.backend_name.clone(),
                        priority: p.priority,
                    })
                    .collect();
                InterfaceEntry {
                    name: name.clone(),
                    providers,
                }
            })
            .collect();
        interfaces.sort_by(|a, b| a.name.cmp(&b.name));

        let health = self
            .diagnostics
            .snapshot()
            .into_iter()
            .map(|(id, status)| HealthEntry {
                plugin_id: id,
                status: status.to_string(),
            })
            .collect();

        let mut backend_runtimes: Vec<BackendRuntimeEntry> = self
            .backend_runtime_statuses
            .values()
            .map(|entry| BackendRuntimeEntry {
                interface: entry.interface.clone(),
                provider_id: entry.provider_id.clone(),
                status: entry.status.as_str().to_string(),
                message: entry.message.clone(),
            })
            .collect();
        backend_runtimes.sort_by(|a, b| {
            a.interface
                .cmp(&b.interface)
                .then_with(|| a.provider_id.cmp(&b.provider_id))
        });

        let active_surfaces = self
            .core
            .surfaces
            .iter()
            .filter(|(_, s)| s.visible)
            .map(|(id, _)| id.clone())
            .collect();

        DebugSnapshot {
            plugins,
            interfaces,
            backend_runtimes,
            health,
            active_surfaces,
        }
    }

    pub fn plugins(&self) -> impl Iterator<Item = (&str, PluginState)> {
        self.plugins
            .iter()
            .map(|(id, inst)| (id.as_str(), inst.state))
    }

    pub fn run(&mut self) -> Result<(), ShellRunError> {
        self.discover_plugins();
        for theme in mesh_core_theme::load_themes_from_dir(&mesh_core_theme::theme_dir_path()) {
            tracing::debug!("registering theme '{}'", theme.id);
            self.theme.register_theme(theme);
        }
        self.resolve_plugins()?;
        self.load_frontend_components()?;

        let runtime = Runtime::new().map_err(ShellRunError::RuntimeInit)?;
        let (tx, mut rx) = mpsc::unbounded_channel::<ShellMessage>();
        self.spawn_backend_plugins(&runtime, tx.clone());
        let ipc_socket_path = default_ipc_socket_path();
        spawn_ipc_server(&runtime, ipc_socket_path.clone(), tx).map_err(|source| {
            ShellRunError::IpcInit {
                path: ipc_socket_path.clone(),
                source,
            }
        })?;

        let mut pending = VecDeque::new();
        pending.extend(self.mount_components()?);
        pending.extend(self.replay_cached_service_events()?);
        self.mark_components_locale_changed()?;
        pending.extend(self.broadcast_core_event(CoreEvent::Started)?);
        play_shell_sound(
            SoundKind::Startup,
            &self.settings.sounds,
            self.service_handlers.get("mesh.audio"),
        );

        tracing::info!(
            "MESH shell core is running with {} frontend component(s)",
            self.components.len()
        );

        while !self.core.shutting_down {
            pending.extend(self.reload_theme_if_changed()?);
            pending.extend(self.reload_locale_if_settings_changed()?);
            self.reload_plugin_settings_if_changed()?;
            self.reload_frontend_components_if_changed()?;
            self.dispatch_wayland()?;

            while let Ok(message) = rx.try_recv() {
                match message {
                    ShellMessage::Service(event) => {
                        pending.extend(self.broadcast_service_event(event)?);
                    }
                    ShellMessage::BackendLifecycle {
                        interface,
                        provider_id,
                        stage,
                        status,
                        message,
                    } => self.handle_backend_lifecycle(
                        interface,
                        provider_id,
                        stage,
                        status,
                        message,
                    ),
                    ShellMessage::Ipc(request) => {
                        pending.push_back(request);
                    }
                }
            }

            pending.extend(self.tick_components()?);
            self.drain_requests(&mut pending)?;
            self.render_components()?;
            self.flush_wayland()?;
            self.render_engine.pump();

            std::thread::sleep(Duration::from_millis(16));
        }

        let mut shutdown_requests = self.broadcast_core_event(CoreEvent::ShuttingDown)?;
        self.drain_requests(&mut shutdown_requests)?;
        let _ = std::fs::remove_file(&ipc_socket_path);
        tracing::info!("shell event loop stopped");
        Ok(())
    }

    fn reload_theme_if_changed(&mut self) -> Result<VecDeque<CoreRequest>, ShellRunError> {
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
        let theme = mesh_core_theme::load_theme_from_path(&self.theme_watch.path)
            .map_err(ShellRunError::Theme)?;
        tracing::info!(
            "reloaded active theme '{}' from {}",
            theme.id,
            self.theme_watch.path.display()
        );
        self.theme.replace_active(theme);
        self.theme_watch.modified_at = Some(modified_at);
        self.mark_components_theme_changed()?;
        let new_theme_id = self.theme.active().id.clone();
        if new_theme_id != old_theme_id {
            return self.sync_theme_service_state(&new_theme_id);
        }
        Ok(VecDeque::new())
    }

    fn reload_frontend_components_if_changed(&mut self) -> Result<(), ShellRunError> {
        for runtime in &mut self.components {
            let Some(source_path) = runtime.source_path.as_ref() else {
                continue;
            };

            let Ok(metadata) = std::fs::metadata(source_path) else {
                continue;
            };
            let Ok(modified_at) = metadata.modified() else {
                continue;
            };

            if runtime.source_modified_at == Some(modified_at) {
                continue;
            }

            let reloaded = runtime
                .component
                .reload_source()
                .map_err(ShellRunError::Component)?;
            runtime.source_modified_at = Some(modified_at);

            if reloaded {
                tracing::info!(
                    "recompiled frontend component '{}' from {}",
                    runtime.component.id(),
                    source_path.display()
                );
            }
        }

        Ok(())
    }

    fn mark_components_theme_changed(&mut self) -> Result<(), ShellRunError> {
        for runtime in &mut self.components {
            runtime
                .component
                .theme_changed()
                .map_err(ShellRunError::Component)?;
        }
        Ok(())
    }

    fn apply_set_theme(&mut self, theme_id: &str) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if self.theme.set_active(theme_id).is_err() {
            let path = mesh_core_theme::theme_path_for_id(theme_id);
            match mesh_core_theme::load_theme_from_path(&path) {
                Ok(theme) => {
                    self.theme.register_theme(theme);
                    if let Err(e) = self.theme.set_active(theme_id) {
                        tracing::warn!("failed to activate theme '{theme_id}': {e}");
                        return Ok(VecDeque::new());
                    }
                }
                Err(e) => {
                    tracing::warn!("cannot load theme '{theme_id}': {e}");
                    return Ok(VecDeque::new());
                }
            }
        }
        tracing::info!("active theme changed to '{theme_id}'");
        let path = mesh_core_theme::theme_path_for_id(theme_id);
        let modified_at = std::fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        self.theme_watch = ThemeWatchState { path, modified_at };
        self.mark_components_theme_changed()?;
        self.sync_theme_service_state(theme_id)
    }

    fn sync_theme_service_state(
        &mut self,
        theme_id: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let is_dark = theme_id.contains("dark");
        let payload =
            serde_json::json!({ "current": theme_id, "theme_id": theme_id, "is_dark": is_dark });
        if let Some(tx) = self.service_handlers.get("mesh.theme") {
            let _ = tx.send(ServiceCommandMsg {
                command: "set-current".to_string(),
                payload: payload.clone(),
            });
        }
        self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.theme".into(),
            source_plugin: "@mesh/shell".into(),
            payload,
        })
    }

    fn reload_locale_if_settings_changed(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
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

        let new_settings = match load_shell_settings() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("failed to reload shell settings: {e}");
                return Ok(requests);
            }
        };

        let old_theme = self.settings.theme.clone();
        let old_i18n = self.settings.i18n.clone();
        let new_i18n = &new_settings.i18n;
        let locale_changed = old_i18n.locale != new_i18n.locale
            || old_i18n.fallback_locale != new_i18n.fallback_locale;

        let theme_changed = old_theme.active != new_settings.theme.active;
        if theme_changed {
            let (theme, theme_watch) = load_active_theme(&new_settings);
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
        }

        self.settings = new_settings;

        Ok(requests)
    }

    fn reload_plugin_settings_if_changed(&mut self) -> Result<(), ShellRunError> {
        for runtime in &mut self.components {
            let current_settings_path = runtime.component.plugin_settings_path().map(PathBuf::from);
            if runtime.plugin_settings_path != current_settings_path {
                runtime.plugin_settings_path = current_settings_path.clone();
                runtime.plugin_settings_modified_at = None;
            }

            let Some(settings_path) = current_settings_path
                .as_ref()
                .or(runtime.plugin_settings_path.as_ref())
            else {
                continue;
            };

            let Ok(metadata) = std::fs::metadata(settings_path) else {
                continue;
            };
            let Ok(modified_at) = metadata.modified() else {
                continue;
            };

            if runtime.plugin_settings_modified_at == Some(modified_at) {
                continue;
            }

            runtime.plugin_settings_modified_at = Some(modified_at);

            let changed = runtime
                .component
                .reload_plugin_settings()
                .map_err(ShellRunError::Component)?;

            if changed {
                tracing::info!(
                    "plugin settings changed for component '{}'",
                    runtime.component.id()
                );
            }
        }
        Ok(())
    }

    fn mark_components_locale_changed(&mut self) -> Result<(), ShellRunError> {
        let locale = self.locale.clone();
        for runtime in &mut self.components {
            runtime
                .component
                .locale_changed(&locale)
                .map_err(ShellRunError::Component)?;
        }
        Ok(())
    }

    fn load_frontend_components(&mut self) -> Result<(), ShellRunError> {
        if !self.components.is_empty() {
            return Ok(());
        }

        let frontend_catalog = FrontendCatalog::from_plugins(&self.plugins)?;
        for entry in frontend_catalog.top_level_surfaces() {
            self.register_component(Box::new(FrontendSurfaceComponent::new(
                entry.compiled,
                entry.plugin_dir,
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

    fn mount_components(&mut self) -> Result<VecDeque<CoreRequest>, ShellRunError> {
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

    fn tick_components(&mut self) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            requests.extend(runtime.component.tick().map_err(ShellRunError::Component)?);
        }
        Ok(requests)
    }

    fn broadcast_core_event(
        &mut self,
        event: CoreEvent,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            requests.extend(
                runtime
                    .component
                    .handle_core_event(&event)
                    .map_err(ShellRunError::Component)?,
            );
        }
        Ok(requests)
    }

    fn broadcast_service_event(
        &mut self,
        event: ServiceEvent,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if !self.record_latest_service_state(&event) {
            return Ok(VecDeque::new());
        }
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            requests.extend(
                runtime
                    .component
                    .handle_service_event(&event)
                    .map_err(ShellRunError::Component)?,
            );
        }
        Ok(requests)
    }

    fn record_latest_service_state(&mut self, event: &ServiceEvent) -> bool {
        let ServiceEvent::Updated {
            service,
            source_plugin,
            payload,
        } = event;
        let interface = canonical_interface_name(service);
        let shell_authoritative_theme_update =
            interface == "mesh.theme" && source_plugin == "@mesh/shell";
        if let Some(slot) = self.backend_runtimes.get(&interface) {
            if slot.provider_id != *source_plugin && !shell_authoritative_theme_update {
                tracing::debug!(
                    interface,
                    source_plugin,
                    active_provider = %slot.provider_id,
                    "ignoring stale service update from inactive provider"
                );
                return false;
            }
        } else if self
            .backend_runtime_statuses
            .get(&(interface.clone(), source_plugin.clone()))
            .is_some_and(|entry| {
                matches!(
                    entry.status,
                    BackendRuntimeStatus::InitFailed
                        | BackendRuntimeStatus::Failed
                        | BackendRuntimeStatus::Stopped
                )
            })
        {
            tracing::debug!(
                interface,
                source_plugin,
                "ignoring service update from terminal backend provider"
            );
            return false;
        }
        self.validate_service_state_shape(&interface, source_plugin, payload);
        self.latest_service_state.insert(
            interface.clone(),
            LatestServiceState {
                interface,
                provider_id: source_plugin.clone(),
                state: payload.clone(),
            },
        );
        true
    }

    fn validate_service_state_shape(
        &mut self,
        interface: &str,
        provider_id: &str,
        payload: &serde_json::Value,
    ) {
        let resolution = self.interfaces.resolve(interface, None);
        let Some(contract) = resolution.contract.as_ref() else {
            return;
        };
        for warning in service_state_contract_warnings(contract, payload) {
            self.record_service_contract_warning(interface, provider_id, warning);
        }
    }

    fn record_service_contract_warning(
        &mut self,
        interface: &str,
        provider_id: &str,
        message: String,
    ) {
        let message = format!("service_contract_warning: {interface}: {message}");
        tracing::warn!(interface, provider_id, "{message}");
        self.diagnostics.record_lifecycle_error(
            provider_id.to_string(),
            "service_contract_warning",
            message,
        );
    }

    fn replay_cached_service_events(&mut self) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        let events = self
            .latest_service_state
            .values()
            .map(|latest| ServiceEvent::Updated {
                service: latest.interface.clone(),
                source_plugin: latest.provider_id.clone(),
                payload: latest.state.clone(),
            })
            .collect::<Vec<_>>();
        for event in events {
            requests.extend(self.broadcast_service_event(event)?);
        }
        Ok(requests)
    }

    fn drain_requests(
        &mut self,
        requests: &mut VecDeque<CoreRequest>,
    ) -> Result<(), ShellRunError> {
        while let Some(request) = requests.pop_front() {
            let emitted = self.apply_request(request)?;
            requests.extend(emitted);
        }
        Ok(())
    }

    fn apply_request(
        &mut self,
        request: CoreRequest,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        match request {
            CoreRequest::PositionSurface {
                surface_id,
                margin_top,
                margin_left,
            } => {
                if let Some(runtime) = self
                    .components
                    .iter_mut()
                    .find(|r| r.surface_id == surface_id)
                {
                    runtime.component.apply_position(margin_top, margin_left);
                }
                Ok(VecDeque::new())
            }
            CoreRequest::ToggleSurface { surface_id } => {
                let visible = self
                    .core
                    .surfaces
                    .get(&surface_id)
                    .map(|state| !state.visible)
                    .unwrap_or(true);
                self.set_surface_visibility(surface_id, visible)
            }
            CoreRequest::ShowSurface { surface_id } => {
                self.set_surface_visibility(surface_id, true)
            }
            CoreRequest::HideSurface { surface_id } => {
                self.set_surface_visibility(surface_id, false)
            }
            CoreRequest::PublishDiagnostics { message } => {
                tracing::info!("diagnostic: {message}");
                Ok(VecDeque::new())
            }
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                source_plugin_id,
                source_capabilities,
            } => {
                let _ = self.dispatch_service_command(
                    &interface,
                    &command,
                    &payload,
                    &source_plugin_id,
                    &source_capabilities,
                );
                Ok(VecDeque::new())
            }
            CoreRequest::SetTheme { theme_id } => self.apply_set_theme(&theme_id),
            CoreRequest::ToggleDebugOverlay => {
                self.debug.toggle();
                tracing::debug!(
                    "debug overlay: {}",
                    if self.debug.enabled { "on" } else { "off" }
                );
                Ok(VecDeque::new())
            }
            CoreRequest::CycleDebugTab => {
                self.debug.cycle_tab();
                Ok(VecDeque::new())
            }
            CoreRequest::Shutdown => {
                self.core.shutting_down = true;
                Ok(VecDeque::new())
            }
        }
    }

    fn dispatch_service_command(
        &mut self,
        interface: &str,
        command: &str,
        payload: &serde_json::Value,
        source_plugin_id: &str,
        source_capabilities: &mesh_core_capability::CapabilitySet,
    ) -> serde_json::Value {
        let required = service_command_control_capability(interface);
        if !source_capabilities.is_granted(&required) {
            tracing::warn!(
                source_plugin_id,
                interface,
                command,
                required_capability = %required,
                "denied unauthorized service command dispatch"
            );
            return serde_json::json!({
                "ok": false,
                "error": "capability_denied",
                "status": "capability_denied",
            });
        }

        if !self.service_command_is_supported(interface, command) {
            let message = format!("unsupported_service_command: {interface}.{command}");
            tracing::warn!(
                source_plugin_id,
                interface,
                command,
                "unsupported_service_command"
            );
            self.diagnostics.record_lifecycle_error(
                source_plugin_id.to_string(),
                "unsupported_service_command",
                message.clone(),
            );
            return serde_json::json!({
                "ok": false,
                "error": message,
                "status": "unsupported_service_command",
            });
        }

        if let Some(tx) = self.service_handlers.get(interface) {
            match tx.send(ServiceCommandMsg {
                command: command.to_string(),
                payload: payload.clone(),
            }) {
                Ok(()) => serde_json::json!({ "ok": true, "queued": true }),
                Err(_) => serde_json::json!({
                    "ok": false,
                    "error": "service_unavailable",
                    "status": "service_unavailable",
                }),
            }
        } else {
            tracing::debug!("no handler registered for service: {interface}");
            serde_json::json!({
                "ok": false,
                "error": "service_unavailable",
                "status": "service_unavailable",
            })
        }
    }

    fn service_command_is_supported(&self, interface: &str, command: &str) -> bool {
        let resolution = self.interfaces.resolve(interface, None);
        let Some(contract) = resolution.contract.as_ref() else {
            return true;
        };
        contract.methods.iter().any(|method| method.name == command)
    }

    fn set_surface_visibility(
        &mut self,
        surface_id: SurfaceId,
        visible: bool,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        self.core
            .surfaces
            .entry(surface_id.clone())
            .and_modify(|state| state.visible = visible)
            .or_insert(SurfaceState { visible });

        self.broadcast_core_event(CoreEvent::SurfaceVisibilityChanged {
            surface_id,
            visible,
        })
    }

    fn render_components(&mut self) -> Result<(), ShellRunError> {
        // Build the snapshot once per frame if the overlay is active.
        let debug_snapshot = self.debug.enabled.then(|| self.build_debug_snapshot());

        for runtime in &mut self.components {
            if !runtime.component.wants_render() {
                continue;
            }

            let mut rerender_attempts = 0;
            let mut buffer = loop {
                let surface = self
                    .surfaces
                    .get_mut(&runtime.surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(runtime.surface_id.clone()))?;
                runtime
                    .component
                    .render(surface)
                    .map_err(ShellRunError::Component)?;

                let visible = self
                    .core
                    .surfaces
                    .get(&runtime.surface_id)
                    .map(|state| state.visible)
                    .unwrap_or(surface.visible);
                let cfg = if visible {
                    LayerSurfaceConfig {
                        edge: surface.edge,
                        layer: surface.layer.unwrap_or(Layer::Top),
                        width: surface.width,
                        height: surface.height,
                        exclusive_zone: surface.exclusive_zone,
                        keyboard_mode: surface.keyboard_mode,
                        namespace: runtime.surface_id.clone(),
                        margin_top: surface.margin_top,
                        margin_right: surface.margin_right,
                        margin_bottom: surface.margin_bottom,
                        margin_left: surface.margin_left,
                    }
                } else {
                    // Hidden surfaces should not keep reserving layer-shell space.
                    // Configure them to a harmless unmapped footprint before the
                    // null-buffer present below.
                    LayerSurfaceConfig {
                        edge: surface.edge,
                        layer: surface.layer.unwrap_or(Layer::Top),
                        width: 1,
                        height: 1,
                        exclusive_zone: 0,
                        keyboard_mode: surface.keyboard_mode,
                        namespace: runtime.surface_id.clone(),
                        margin_top: 0,
                        margin_right: 0,
                        margin_bottom: 0,
                        margin_left: 0,
                    }
                };
                self.render_engine.configure(&runtime.surface_id, cfg);

                if !visible {
                    break PixelBuffer::new(1, 1);
                }

                let width = surface.width.max(1);
                let height = surface.height.max(1);
                let mut buffer = PixelBuffer::new(width, height);
                runtime
                    .component
                    .paint(self.theme.active(), width, height, &mut buffer)
                    .map_err(ShellRunError::Component)?;

                // Content-sized widgets may need one follow-up render pass after paint
                // computes a tighter measured size.
                if !runtime.component.wants_render() || rerender_attempts >= 1 {
                    break buffer;
                }

                rerender_attempts += 1;
            };

            let visible = self
                .core
                .surfaces
                .get(&runtime.surface_id)
                .map(|state| state.visible)
                .unwrap_or_else(|| {
                    self.surfaces
                        .get(&runtime.surface_id)
                        .map(|surface| surface.visible)
                        .unwrap_or(true)
                });

            // Paint debug overlay on top of the rendered buffer.
            if visible && let Some(snapshot) = &debug_snapshot {
                if self.debug.show_layout_bounds {
                    if let Some(tree) = runtime.component.last_widget_tree() {
                        self.debug_overlay
                            .paint_layout_bounds(tree, &mut buffer, 1.0);
                    }
                }
                self.debug_overlay
                    .paint_panel(snapshot, self.debug.active_tab, &mut buffer, 1.0);
            }

            self.render_engine
                .present(
                    &runtime.surface_id,
                    runtime.component.id(),
                    visible,
                    &buffer,
                )
                .map_err(ShellRunError::Render)?;
        }
        Ok(())
    }

    fn dispatch_wayland(&mut self) -> Result<(), ShellRunError> {
        let events = coalesce_pointer_moves(self.render_engine.poll_events());
        for event in events {
            tracing::trace!(
                "[hover] dispatch_wayland: got event {:?}",
                std::mem::discriminant(&event)
            );
            let surface_id = event_surface_id(&event);

            let Some(index) = self
                .components
                .iter()
                .position(|runtime| runtime.surface_id == *surface_id)
            else {
                continue;
            };

            let runtime_surface_id = self.components[index].surface_id.clone();
            let Some(surface) = self.surfaces.get(&runtime_surface_id) else {
                continue;
            };

            // Intercept global shortcuts before routing to components.
            // Key names vary by backend (minifb: "D", xkbcommon: "d") so
            // we normalise to lowercase. Modifier state comes from KeyMods
            // embedded in the event, not shell-side tracking.
            if let WindowEvent::Key {
                event: WindowKeyEvent::Pressed(key, mods),
                ..
            } = &event
            {
                match key.to_ascii_lowercase().as_str() {
                    "d" if mods.ctrl && mods.shift => {
                        let mut pending = VecDeque::from([CoreRequest::ToggleDebugOverlay]);
                        self.drain_requests(&mut pending)?;
                        continue;
                    }
                    "tab" | "iso_left_tab" if mods.ctrl && self.debug.enabled => {
                        let mut pending = VecDeque::from([CoreRequest::CycleDebugTab]);
                        self.drain_requests(&mut pending)?;
                        continue;
                    }
                    _ => {}
                }
            }

            let input = match event {
                WindowEvent::PointerMove { x, y, .. } => ComponentInput::PointerMove { x, y },
                WindowEvent::PointerButton { x, y, pressed, .. } => {
                    ComponentInput::PointerButton { x, y, pressed }
                }
                WindowEvent::Scroll { x, y, dx, dy, .. } => ComponentInput::Scroll { x, y, dx, dy },
                WindowEvent::Key {
                    event: WindowKeyEvent::Pressed(key, _mods),
                    ..
                } => ComponentInput::KeyPressed { key },
                WindowEvent::Key {
                    event: WindowKeyEvent::Released(key),
                    ..
                } => ComponentInput::KeyReleased { key },
                WindowEvent::Char { ch, .. } => ComponentInput::Char { ch },
            };

            tracing::trace!(
                "[hover] dispatch_wayland: routing event to surface_id={}",
                runtime_surface_id
            );
            let emitted = {
                let runtime = &mut self.components[index];
                runtime.component.handle_input(
                    self.theme.active(),
                    surface.width.max(1),
                    surface.height.max(1),
                    input,
                )
            }
            .map_err(ShellRunError::Component)?;

            for request in emitted {
                let mut pending = VecDeque::from([request]);
                self.drain_requests(&mut pending)?;
            }
        }

        Ok(())
    }

    fn flush_wayland(&mut self) -> Result<(), ShellRunError> {
        for runtime in &self.components {
            let surface = self
                .surfaces
                .get(&runtime.surface_id)
                .ok_or_else(|| ShellRunError::MissingSurface(runtime.surface_id.clone()))?;
            tracing::trace!(
                "flushing surface '{}' size={}x{} visible={}",
                runtime.surface_id,
                surface.width,
                surface.height,
                surface.visible
            );
        }
        Ok(())
    }

    fn record_backend_runtime_status(
        &mut self,
        interface: String,
        provider_id: String,
        status: BackendRuntimeStatus,
        message: String,
    ) {
        if matches!(
            status,
            BackendRuntimeStatus::InvalidManifest
                | BackendRuntimeStatus::MissingEntrypoint
                | BackendRuntimeStatus::MissingBinary
                | BackendRuntimeStatus::InitFailed
                | BackendRuntimeStatus::PollFailed
                | BackendRuntimeStatus::Failed
        ) {
            self.diagnostics.record_lifecycle_error(
                provider_id.clone(),
                status.as_str(),
                message.clone(),
            );
        }
        self.backend_runtime_statuses.insert(
            (interface.clone(), provider_id.clone()),
            BackendRuntimeStatusEntry {
                interface,
                provider_id,
                status,
                message,
            },
        );
    }

    fn stop_backend_runtime(&mut self, interface: &str) {
        self.service_handlers.remove(interface);
        if let Some(slot) = self.backend_runtimes.remove(interface) {
            slot.task.abort();
            let key = (slot.interface.clone(), slot.provider_id.clone());
            let terminal_failure_already_recorded = self
                .backend_runtime_statuses
                .get(&key)
                .map(|entry| {
                    matches!(
                        entry.status,
                        BackendRuntimeStatus::InitFailed | BackendRuntimeStatus::Failed
                    )
                })
                .unwrap_or(false);
            if !terminal_failure_already_recorded {
                self.record_backend_runtime_status(
                    slot.interface,
                    slot.provider_id,
                    BackendRuntimeStatus::Stopped,
                    "runtime stopped".to_string(),
                );
            }
        }
    }

    fn replace_backend_runtime(&mut self, interface: String, slot: BackendRuntimeSlot) {
        self.stop_backend_runtime(&interface);
        self.service_handlers
            .insert(interface.clone(), slot.command_tx.clone());
        self.backend_runtimes.insert(interface, slot);
    }

    fn handle_backend_lifecycle(
        &mut self,
        interface: String,
        provider_id: String,
        stage: String,
        status: String,
        message: String,
    ) {
        let runtime_status = BackendRuntimeStatus::from_str(&status);
        self.record_backend_runtime_status(
            interface.clone(),
            provider_id.clone(),
            runtime_status,
            message,
        );
        let event_provider_is_current = self
            .backend_runtimes
            .get(&interface)
            .is_some_and(|slot| slot.provider_id == provider_id);
        if matches!(
            runtime_status,
            BackendRuntimeStatus::InitFailed
                | BackendRuntimeStatus::Failed
                | BackendRuntimeStatus::Stopped
        ) && event_provider_is_current
        {
            tracing::debug!(
                interface = interface,
                stage = stage,
                "cleaning backend runtime slot"
            );
            self.stop_backend_runtime(&interface);
        }
    }

    fn spawn_backend_plugins(
        &mut self,
        runtime: &Runtime,
        tx: mpsc::UnboundedSender<ShellMessage>,
    ) {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let graph_path = workspace_root.join("config/package.json");
        match load_installed_module_graph(&graph_path) {
            Ok(graph) => {
                let (candidates, statuses) = backend_launch_candidates_from_graph(
                    &graph,
                    &self.plugins,
                    &self.config,
                    &self.interfaces,
                );
                for status in statuses {
                    self.record_backend_runtime_status(
                        status.interface.clone(),
                        status
                            .provider_id
                            .clone()
                            .unwrap_or_else(|| "<none>".to_string()),
                        BackendRuntimeStatus::from_str(status.status),
                        status.message.clone(),
                    );
                    tracing::warn!(
                        interface = status.interface,
                        provider_id = status.provider_id.as_deref().unwrap_or("<none>"),
                        status = status.status,
                        "{}",
                        status.message
                    );
                }
                for mut candidate in candidates {
                    self.apply_shell_runtime_settings(&mut candidate);
                    self.spawn_backend_candidate(runtime, tx.clone(), candidate);
                }
            }
            Err(err) => {
                tracing::warn!(
                    "failed to load installed module graph from {}; using legacy backend discovery: {err}",
                    graph_path.display()
                );
                for mut candidate in
                    legacy_backend_candidates_from_discovery(&self.plugins, &self.config)
                {
                    self.apply_shell_runtime_settings(&mut candidate);
                    self.spawn_backend_candidate(runtime, tx.clone(), candidate);
                }
            }
        }
    }

    fn apply_shell_runtime_settings(&self, candidate: &mut BackendLaunchCandidate) {
        if candidate.interface != "mesh.theme" {
            return;
        }

        let current_theme = self.theme.active().id.clone();
        if let Some(settings) = candidate.settings.as_object_mut() {
            settings.insert(
                "current_theme".to_string(),
                serde_json::Value::String(current_theme),
            );
        } else {
            candidate.settings = serde_json::json!({
                "current_theme": current_theme,
            });
        }
    }

    fn spawn_backend_candidate(
        &mut self,
        runtime: &Runtime,
        tx: mpsc::UnboundedSender<ShellMessage>,
        candidate: BackendLaunchCandidate,
    ) {
        self.stop_backend_runtime(&candidate.interface);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let shell_tx = tx.clone();
        let interface = candidate.interface.clone();
        let provider_id = candidate.module_id.clone();
        let (backend_tx, mut backend_rx) = mpsc::unbounded_channel::<BackendServiceEvent>();
        let bridge_interface = interface.clone();
        let bridge_provider_id = provider_id.clone();
        runtime.spawn(async move {
            while let Some(event) = backend_rx.recv().await {
                match event {
                    BackendServiceEvent::Update(update) => {
                        if shell_tx
                            .send(ShellMessage::Service(ServiceEvent::Updated {
                                service: update.service,
                                source_plugin: update.source_plugin,
                                payload: update.payload,
                            }))
                            .is_err()
                        {
                            break;
                        }
                    }
                    BackendServiceEvent::CommandResult(result) => {
                        tracing::debug!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            command = result.command.as_str(),
                            result = %result.result,
                            "backend command result"
                        );
                    }
                    BackendServiceEvent::Started { .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            stage: "runtime".to_string(),
                            status: "running".to_string(),
                            message: "backend runtime started".to_string(),
                        });
                        tracing::info!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "backend runtime started"
                        );
                    }
                    BackendServiceEvent::InitFailed { message, .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            stage: "init".to_string(),
                            status: "init_failed".to_string(),
                            message: message.clone(),
                        });
                        tracing::warn!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "{message}"
                        );
                    }
                    BackendServiceEvent::PollFailed { message, .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            stage: "poll".to_string(),
                            status: "poll_failed".to_string(),
                            message: message.clone(),
                        });
                        tracing::warn!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "{message}"
                        );
                    }
                    BackendServiceEvent::Failed { stage, message, .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            stage,
                            status: "failed".to_string(),
                            message: message.clone(),
                        });
                        tracing::warn!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "{message}"
                        );
                    }
                    BackendServiceEvent::Stopped { .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            stage: "runtime".to_string(),
                            status: "stopped".to_string(),
                            message: "backend runtime stopped".to_string(),
                        });
                        tracing::info!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "backend runtime stopped"
                        );
                    }
                }
            }
        });
        let task = runtime.spawn(spawn_backend_service(
            candidate.module_id,
            candidate.service_name,
            candidate.capabilities,
            candidate.settings,
            candidate.script_source,
            backend_tx,
            cmd_rx,
        ));
        self.replace_backend_runtime(
            interface.clone(),
            BackendRuntimeSlot {
                interface,
                provider_id,
                command_tx: cmd_tx,
                task: task.abort_handle(),
            },
        );
    }
}

fn backend_launch_candidates_from_graph(
    graph: &InstalledModuleGraph,
    plugins: &HashMap<String, PluginInstance>,
    config: &ShellConfig,
    interfaces: &InterfaceRegistry,
) -> (
    Vec<BackendLaunchCandidate>,
    Vec<BackendLifecycleStatusRecord>,
) {
    let mut statuses = backend_requirement_statuses(graph);
    let mut interface_names: Vec<String> = graph
        .backend_modules()
        .into_iter()
        .flat_map(|module| {
            module
                .manifest
                .mesh
                .provides
                .iter()
                .map(|provided| provided.interface.clone())
                .collect::<Vec<_>>()
        })
        .collect();
    interface_names.sort();
    interface_names.dedup();

    let mut candidates = Vec::new();
    for interface in interface_names {
        let Some(active_provider) = graph.active_provider(&interface) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface,
                provider_id: None,
                status: "no_active_provider",
                message: "no active provider selected".to_string(),
            });
            continue;
        };

        let Some(module) = graph.module(&active_provider.module_id) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} is not installed",
                    active_provider.module_id
                ),
            });
            continue;
        };

        if !module.enabled || module.kind != ModuleKind::Backend {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} is not an enabled backend module",
                    active_provider.module_id
                ),
            });
            continue;
        }

        if !module
            .manifest
            .mesh
            .provides
            .iter()
            .any(|provided| provided.interface == interface)
        {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} does not declare interface {interface}",
                    active_provider.module_id
                ),
            });
            continue;
        }

        if let Some(status) =
            validate_backend_provider_contract(&interface, &active_provider.module_id, interfaces)
        {
            statuses.push(status);
            continue;
        }

        let Some(plugin) = plugins.get(&active_provider.module_id) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} has no discovered runtime manifest",
                    active_provider.module_id
                ),
            });
            continue;
        };

        if let Some(binary) = plugin
            .manifest
            .dependencies
            .binaries
            .iter()
            .find(|binary| !binary_exists(&binary.name))
        {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "missing_binary",
                message: format!(
                    "backend provider {} requires unavailable binary {}",
                    active_provider.module_id, binary.name
                ),
            });
            continue;
        }

        let entrypoint = module.manifest.mesh.entrypoints.main.as_deref().or(plugin
            .manifest
            .entrypoints
            .main
            .as_deref());
        let Some(entrypoint) = entrypoint else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "missing_entrypoint",
                message: format!(
                    "backend provider {} has no service entrypoint",
                    active_provider.module_id
                ),
            });
            continue;
        };

        let entrypoint_path = plugin.path.join(entrypoint);
        let Ok(script_source) = std::fs::read_to_string(&entrypoint_path) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "missing_entrypoint",
                message: format!(
                    "backend provider {} entrypoint is unreadable: {}",
                    active_provider.module_id,
                    entrypoint_path.display()
                ),
            });
            continue;
        };

        let capabilities = plugin
            .manifest
            .capabilities
            .required
            .iter()
            .chain(plugin.manifest.capabilities.optional.iter())
            .cloned()
            .collect::<Vec<_>>();
        let settings = backend_plugin_settings_json(config, &active_provider.module_id);
        candidates.push(BackendLaunchCandidate {
            module_id: active_provider.module_id.clone(),
            interface: interface.clone(),
            service_name: service_name_from_interface(&interface),
            entrypoint_path,
            script_source,
            capabilities,
            settings,
        });
    }

    (candidates, statuses)
}

fn backend_requirement_statuses(graph: &InstalledModuleGraph) -> Vec<BackendLifecycleStatusRecord> {
    let mut statuses = Vec::new();
    for frontend in graph.frontend_modules() {
        let Some(requirements) = graph.requirements_for_frontend(&frontend.id) else {
            continue;
        };
        for interface in requirements.backend.keys() {
            if graph.backend_providers_for_interface(interface).is_empty() {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "unmet_backend_requirement",
                    message: format!(
                        "frontend module {} requires {interface}, but no enabled backend provider is installed",
                        frontend.id
                    ),
                });
            } else if graph.active_provider(interface).is_none() {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "no_active_provider",
                    message: format!(
                        "frontend module {} requires {interface}, but no active provider is selected",
                        frontend.id
                    ),
                });
            }
        }
    }
    statuses
}

fn validate_backend_provider_contract(
    interface: &str,
    provider_id: &str,
    interfaces: &InterfaceRegistry,
) -> Option<BackendLifecycleStatusRecord> {
    let resolution = interfaces.resolve(interface, None);
    if resolution.contract.is_none() && resolution.provider.is_none() {
        return None;
    }

    if resolution.contract.is_none() {
        return Some(BackendLifecycleStatusRecord {
            interface: canonical_interface_name(interface),
            provider_id: Some(provider_id.to_string()),
            status: "invalid_manifest",
            message: format!("active provider {provider_id} has no interface contract"),
        });
    }

    if !interfaces
        .providers_for(interface)
        .iter()
        .any(|provider| provider.provider_plugin == provider_id)
    {
        return Some(BackendLifecycleStatusRecord {
            interface: canonical_interface_name(interface),
            provider_id: Some(provider_id.to_string()),
            status: "invalid_manifest",
            message: format!(
                "active provider {provider_id} is not registered for interface {}",
                canonical_interface_name(interface)
            ),
        });
    }

    None
}

fn legacy_backend_candidates_from_discovery(
    plugins: &HashMap<String, PluginInstance>,
    config: &ShellConfig,
) -> Vec<BackendLaunchCandidate> {
    let mut plugin_ids: Vec<String> = plugins.keys().cloned().collect();
    plugin_ids.sort();
    let mut services: HashMap<String, Vec<(&PluginInstance, u32)>> = HashMap::new();

    for plugin_id in plugin_ids {
        let Some(plugin) = plugins.get(&plugin_id) else {
            continue;
        };
        if plugin.manifest.package.plugin_type != PluginType::Backend {
            continue;
        }
        let Some(service) = plugin.manifest.primary_service() else {
            continue;
        };
        let service_name = service_name_from_interface(&service.provides);
        services
            .entry(service_name)
            .or_default()
            .push((plugin, service.priority));
    }

    let mut candidates = Vec::new();
    for (service_name, mut service_candidates) in services {
        service_candidates.sort_by(|(a, a_priority), (b, b_priority)| {
            b_priority
                .cmp(a_priority)
                .then_with(|| a.manifest.package.id.cmp(&b.manifest.package.id))
        });
        let Some((plugin, _)) = service_candidates.into_iter().next() else {
            continue;
        };
        let missing_binary = plugin
            .manifest
            .dependencies
            .binaries
            .iter()
            .find(|binary| !binary_exists(&binary.name));
        if let Some(binary) = missing_binary {
            tracing::info!(
                "skipping legacy backend '{}' for service '{}' because binary '{}' is unavailable",
                plugin.manifest.package.id,
                service_name,
                binary.name
            );
            continue;
        }
        let Some(entrypoint) = plugin.manifest.entrypoints.main.as_deref() else {
            tracing::warn!(
                "legacy backend plugin {} has no service entrypoint",
                plugin.manifest.package.id
            );
            continue;
        };
        let entrypoint_path = plugin.path.join(entrypoint);
        let Ok(script_source) = std::fs::read_to_string(&entrypoint_path) else {
            tracing::warn!(
                "legacy backend plugin {} has no readable script at {}",
                plugin.manifest.package.id,
                entrypoint_path.display()
            );
            continue;
        };
        let capabilities = plugin
            .manifest
            .capabilities
            .required
            .iter()
            .chain(plugin.manifest.capabilities.optional.iter())
            .cloned()
            .collect::<Vec<_>>();
        candidates.push(BackendLaunchCandidate {
            module_id: plugin.manifest.package.id.clone(),
            interface: format!("mesh.{service_name}"),
            service_name,
            entrypoint_path,
            script_source,
            capabilities,
            settings: backend_plugin_settings_json(config, &plugin.manifest.package.id),
        });
    }

    candidates
}

fn binary_exists(name: &str) -> bool {
    if name.contains(std::path::MAIN_SEPARATOR) {
        return Path::new(name).is_file();
    }

    let Some(paths) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&paths).any(|dir| dir.join(name).is_file())
}

fn backend_plugin_settings_json(config: &ShellConfig, plugin_id: &str) -> serde_json::Value {
    config
        .plugins
        .get(plugin_id)
        .map(|plugin| match serde_json::to_value(&plugin.values) {
            Ok(serde_json::Value::Object(map)) => serde_json::Value::Object(map),
            Ok(_) => serde_json::json!({}),
            Err(err) => {
                tracing::warn!(
                    plugin_id = plugin_id,
                    "failed to serialize backend plugin settings: {err}"
                );
                serde_json::json!({})
            }
        })
        .unwrap_or_else(|| serde_json::json!({}))
}

fn service_state_contract_warnings(
    contract: &InterfaceContract,
    payload: &serde_json::Value,
) -> Vec<String> {
    let Some(object) = payload.as_object() else {
        return vec![format!(
            "state for {} must be a JSON object, got {}",
            contract.interface,
            json_type_name(payload)
        )];
    };

    let mut warnings = Vec::new();
    for field in &contract.state_fields {
        if is_runtime_metadata_state_field(&field.name) {
            continue;
        }
        let Some(value) = object.get(&field.name) else {
            warnings.push(format!(
                "missing required state field '{}' for {}",
                field.name, contract.interface
            ));
            continue;
        };
        if !json_value_matches_contract_type(value, &field.field_type) {
            warnings.push(format!(
                "state field '{}' for {} expected {}, got {}",
                field.name,
                contract.interface,
                field.field_type,
                json_type_name(value)
            ));
        }
    }
    warnings
}

fn is_runtime_metadata_state_field(name: &str) -> bool {
    name == "source_plugin"
}

fn json_value_matches_contract_type(value: &serde_json::Value, field_type: &str) -> bool {
    let normalized = field_type.trim().to_ascii_lowercase();
    if normalized.starts_with('[') && normalized.ends_with(']') {
        return value.is_array();
    }

    match normalized.as_str() {
        "bool" | "boolean" => value.is_boolean(),
        "float" | "double" | "number" => value.is_number(),
        "int" | "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "string" => value.is_string(),
        "object" | "table" | "map" => value.is_object(),
        _ => true,
    }
}

fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellRunError {
    #[error("failed to initialize async runtime: {0}")]
    RuntimeInit(std::io::Error),

    #[error(transparent)]
    Component(#[from] ComponentError),

    #[error("failed to compile frontend plugin '{plugin_id}': {source}")]
    FrontendCompile {
        plugin_id: String,
        source: mesh_core_render::CompileFrontendError,
    },

    #[error(transparent)]
    DependencyGraph(#[from] DependencyGraphError),

    #[error("{message}")]
    FrontendComposition { message: String },

    #[error("missing shell surface: {0}")]
    MissingSurface(String),

    #[error(transparent)]
    Render(#[from] mesh_core_render::RenderError),

    #[error("failed to initialize ipc socket at {path}: {source}")]
    IpcInit {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error(transparent)]
    Theme(#[from] mesh_core_theme::ThemeError),
}

fn default_plugin_dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");

    vec![
        workspace_root.join("packages/plugins"),
        PathBuf::from("/usr/share/mesh/plugins"),
        PathBuf::from(&home).join(".local/share/mesh/plugins"),
        PathBuf::from(&home).join(".local/share/mesh/dev-plugins"),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        BackendLaunchCandidate, BackendRuntimeSlot, BackendRuntimeStatus, InterfaceProvider,
        InterfaceRegistry, ServiceCommandMsg, ServiceEvent, Shell,
        backend_launch_candidates_from_graph,
        layout::measure_content_size,
        service::{apply_service_update, seed_service_state, service_name_from_interface},
        surface_layout::{SurfaceSizePolicy, load_active_theme, load_frontend_plugin_settings},
    };
    use mesh_core_config::ShellConfig;
    use mesh_core_elements::{LayoutRect, VariableStore, WidgetNode};
    use mesh_core_plugin::PluginInstance;
    use mesh_core_plugin::manifest::{
        CapabilitiesSection, CompatibilitySection, DependenciesSection, EntrypointsSection,
        ExportsSection, Manifest, ManifestSource, PackageSection, PluginType, ProvidedInterface,
        SurfaceLayoutSection,
    };
    use mesh_core_plugin::package::{
        InstalledModuleGraph, LoadedModuleManifest, ModuleManifestSource, ModulePackageManifest,
        RootPackageManifest,
    };
    use mesh_core_scripting::ScriptState;
    use mesh_core_service::{
        ContractCapabilities, InterfaceContract, InterfaceMethod, contract::ContractStateField,
        parse_contract_version,
    };
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::runtime::Runtime;
    use tokio::sync::mpsc;

    static SETTINGS_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        old: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &std::path::Path) -> Self {
            let old = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, old }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.old {
                    Some(value) => std::env::set_var(self.key, value),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    fn node(tag: &str, x: f32, y: f32, width: f32, height: f32) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.layout = LayoutRect {
            x,
            y,
            width,
            height,
        };
        node
    }

    fn minimal_manifest(id: &str) -> Manifest {
        Manifest {
            package: PackageSection {
                id: id.to_string(),
                name: None,
                version: "0.1.0".into(),
                plugin_type: PluginType::Surface,
                api_version: "0.1".into(),
                license: None,
                description: None,
                authors: Vec::new(),
                repository: None,
            },
            compatibility: CompatibilitySection::default(),
            dependencies: DependenciesSection::default(),
            capabilities: CapabilitiesSection::default(),
            entrypoints: EntrypointsSection::default(),
            accessibility: None,
            settings: None,
            i18n: None,
            theme: None,
            service: None,
            provides: Vec::new(),
            interface: None,
            extensions: Vec::new(),
            exports: ExportsSection::default(),
            provides_slots: HashMap::new(),
            slot_contributions: HashMap::new(),
            assets: None,
            icon_requirements: mesh_core_plugin::IconRequirementsSection::default(),
            translations: HashMap::new(),
            surface_layout: None,
        }
    }

    fn minimal_backend_manifest(id: &str, entrypoint: Option<&str>) -> Manifest {
        let mut manifest = minimal_manifest(id);
        manifest.package.plugin_type = PluginType::Backend;
        manifest.entrypoints.main = entrypoint.map(str::to_string);
        manifest.provides = vec![ProvidedInterface {
            interface: "mesh.audio".to_string(),
            version: Some("1.0".to_string()),
            base_plugin: None,
            backend_name: Some(id.to_string()),
            priority: 100,
            optional_capabilities: Vec::new(),
        }];
        manifest
    }

    fn plugin_instance(id: &str, entrypoint: Option<&str>) -> (tempfile::TempDir, PluginInstance) {
        let dir = tempfile::tempdir().unwrap();
        if let Some(entrypoint) = entrypoint {
            let path = dir.path().join(entrypoint);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "function init()\nend\nfunction on_poll()\nend").unwrap();
        }
        let manifest = minimal_backend_manifest(id, entrypoint);
        let instance = PluginInstance::new(
            manifest,
            dir.path().to_path_buf(),
            dir.path().join("plugin.json"),
            ManifestSource::PluginJson,
        );
        (dir, instance)
    }

    fn test_config() -> ShellConfig {
        ShellConfig {
            shell: Default::default(),
            plugins: HashMap::new(),
        }
    }

    fn loaded_module(json: &str) -> LoadedModuleManifest {
        LoadedModuleManifest {
            manifest: ModulePackageManifest::from_json_str(json).unwrap(),
            path: PathBuf::from("<test>/package.json"),
            source: ModuleManifestSource::PackageJson,
        }
    }

    fn graph_from_json(root: &str, modules: Vec<&str>) -> InstalledModuleGraph {
        InstalledModuleGraph::from_parts(
            RootPackageManifest::from_json_str(root).unwrap(),
            modules.into_iter().map(loaded_module).collect(),
        )
        .unwrap()
    }

    fn test_contract(interface: &str) -> InterfaceContract {
        InterfaceContract {
            interface: interface.to_string(),
            version: parse_contract_version("1.0").unwrap(),
            file_path: PathBuf::from("interface.toml"),
            state_fields: vec![
                ContractStateField {
                    name: "available".to_string(),
                    field_type: "boolean".to_string(),
                    description: None,
                },
                ContractStateField {
                    name: "percent".to_string(),
                    field_type: "float".to_string(),
                    description: None,
                },
                ContractStateField {
                    name: "source_plugin".to_string(),
                    field_type: "string".to_string(),
                    description: None,
                },
            ],
            methods: vec![InterfaceMethod {
                name: "set_volume".to_string(),
                args: Vec::new(),
                returns: Some("Result".to_string()),
            }],
            events: Vec::new(),
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        }
    }

    fn register_test_provider(interfaces: &InterfaceRegistry, interface: &str, provider_id: &str) {
        interfaces.register(InterfaceProvider {
            interface: interface.to_string(),
            version: Some("1.0".to_string()),
            base_plugin: Some("@mesh/test-interface".to_string()),
            provider_plugin: provider_id.to_string(),
            backend_name: provider_id.to_string(),
            priority: 100,
        });
    }

    fn backend_runtime_slot(
        runtime: &Runtime,
        interface: &str,
        provider_id: &str,
    ) -> (
        BackendRuntimeSlot,
        mpsc::UnboundedReceiver<ServiceCommandMsg>,
    ) {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let task = runtime.spawn(async {
            std::future::pending::<()>().await;
        });
        (
            BackendRuntimeSlot {
                interface: interface.to_string(),
                provider_id: provider_id.to_string(),
                command_tx,
                task: task.abort_handle(),
            },
            command_rx,
        )
    }

    fn service_update(
        interface: &str,
        provider_id: &str,
        payload: serde_json::Value,
    ) -> ServiceEvent {
        ServiceEvent::Updated {
            service: interface.to_string(),
            source_plugin: provider_id.to_string(),
            payload,
        }
    }

    struct RecordingComponent {
        events: Arc<Mutex<Vec<ServiceEvent>>>,
    }

    impl RecordingComponent {
        fn new(events: Arc<Mutex<Vec<ServiceEvent>>>) -> Self {
            Self { events }
        }
    }

    impl super::types::ShellComponent for RecordingComponent {
        fn id(&self) -> &str {
            "@test/recording"
        }

        fn surface_id(&self) -> &str {
            "@test/recording"
        }

        fn mount(
            &mut self,
            _ctx: super::types::ComponentContext,
        ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
            Ok(Vec::new())
        }

        fn handle_core_event(
            &mut self,
            _event: &super::types::CoreEvent,
        ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
            Ok(Vec::new())
        }

        fn handle_service_event(
            &mut self,
            event: &ServiceEvent,
        ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
            self.events.lock().unwrap().push(event.clone());
            Ok(Vec::new())
        }

        fn tick(&mut self) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
            Ok(Vec::new())
        }

        fn wants_render(&self) -> bool {
            false
        }

        fn render(
            &mut self,
            _surface: &mut dyn mesh_core_wayland::ShellSurface,
        ) -> Result<(), super::types::ComponentError> {
            Ok(())
        }

        fn paint(
            &mut self,
            _theme: &mesh_core_theme::Theme,
            _width: u32,
            _height: u32,
            _buffer: &mut mesh_core_render::PixelBuffer,
        ) -> Result<(), super::types::ComponentError> {
            Ok(())
        }

        fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
            Ok(())
        }
    }

    #[test]
    fn latest_service_state_is_keyed_by_interface() {
        let mut shell = Shell::new();

        shell
            .broadcast_service_event(service_update(
                "audio",
                "@mesh/pipewire-audio",
                serde_json::json!({ "available": true, "percent": 42.0 }),
            ))
            .unwrap();

        assert!(shell.latest_service_state.contains_key("mesh.audio"));
        assert!(!shell.latest_service_state.contains_key("audio"));
        let latest = shell.latest_service_state.get("mesh.audio").unwrap();
        assert_eq!(latest.interface, "mesh.audio");
        assert_eq!(latest.state["percent"], serde_json::json!(42.0));
    }

    #[test]
    fn latest_service_state_tracks_provider_metadata_separately() {
        let mut shell = Shell::new();

        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pipewire-audio",
                serde_json::json!({ "available": true, "percent": 65.0, "muted": false }),
            ))
            .unwrap();

        let latest = shell.latest_service_state.get("mesh.audio").unwrap();
        assert_eq!(latest.provider_id, "@mesh/pipewire-audio");
        assert_eq!(latest.state["available"], serde_json::json!(true));
        assert!(latest.state.get("source_plugin").is_none());
    }

    #[test]
    fn provider_swap_replaces_interface_latest_state() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        let (pipewire_slot, _pipewire_rx) =
            backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), pipewire_slot);
        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pipewire-audio",
                serde_json::json!({ "available": true, "percent": 40.0 }),
            ))
            .unwrap();

        let (pulse_slot, _pulse_rx) =
            backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pulseaudio-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), pulse_slot);
        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pulseaudio-audio",
                serde_json::json!({ "available": true, "percent": 55.0 }),
            ))
            .unwrap();

        assert_eq!(shell.latest_service_state.len(), 1);
        assert!(shell.latest_service_state.contains_key("mesh.audio"));
        let latest = shell.latest_service_state.get("mesh.audio").unwrap();
        assert_eq!(latest.interface, "mesh.audio");
        assert_eq!(latest.provider_id, "@mesh/pulseaudio-audio");
        assert_eq!(latest.state["percent"], serde_json::json!(55.0));
        assert!(
            !shell
                .latest_service_state
                .values()
                .any(|latest| latest.provider_id == "@mesh/pipewire-audio")
        );
    }

    #[test]
    fn stale_provider_update_does_not_replace_current_latest_state() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        let (pipewire_slot, _pipewire_rx) =
            backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), pipewire_slot);
        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pipewire-audio",
                serde_json::json!({ "available": true, "percent": 40.0 }),
            ))
            .unwrap();

        let (pulse_slot, _pulse_rx) =
            backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pulseaudio-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), pulse_slot);
        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pulseaudio-audio",
                serde_json::json!({ "available": true, "percent": 55.0 }),
            ))
            .unwrap();
        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pipewire-audio",
                serde_json::json!({ "available": true, "percent": 5.0 }),
            ))
            .unwrap();

        let latest = shell.latest_service_state.get("mesh.audio").unwrap();
        assert_eq!(latest.provider_id, "@mesh/pulseaudio-audio");
        assert_eq!(latest.state["percent"], serde_json::json!(55.0));
    }

    #[test]
    fn stale_provider_update_does_not_reach_components() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        let seen_events = Arc::new(Mutex::new(Vec::new()));
        shell
            .components
            .push(super::types::ComponentRuntime::new(Box::new(
                RecordingComponent::new(seen_events.clone()),
            )));
        let (old_slot, _old_rx) =
            backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pipewire-audio",
                serde_json::json!({ "available": true, "percent": 40.0 }),
            ))
            .unwrap();

        let (new_slot, _new_rx) =
            backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pulseaudio-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);
        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pulseaudio-audio",
                serde_json::json!({ "available": true, "percent": 55.0 }),
            ))
            .unwrap();
        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pipewire-audio",
                serde_json::json!({ "available": true, "percent": 5.0 }),
            ))
            .unwrap();

        let events = seen_events.lock().unwrap();
        assert_eq!(events.len(), 2);
        let ServiceEvent::Updated {
            source_plugin,
            payload,
            ..
        } = &events[0];
        assert_eq!(source_plugin, "@mesh/pipewire-audio");
        assert_eq!(payload["percent"], serde_json::json!(40.0));
        let ServiceEvent::Updated {
            source_plugin,
            payload,
            ..
        } = events.last().unwrap();
        assert_eq!(source_plugin, "@mesh/pulseaudio-audio");
        assert_eq!(payload["percent"], serde_json::json!(55.0));
    }

    #[test]
    fn terminal_provider_update_does_not_replace_latest_state_or_reach_components() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        let seen_events = Arc::new(Mutex::new(Vec::new()));
        shell
            .components
            .push(super::types::ComponentRuntime::new(Box::new(
                RecordingComponent::new(seen_events.clone()),
            )));
        let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), slot);
        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pipewire-audio",
                serde_json::json!({ "available": true, "percent": 40.0 }),
            ))
            .unwrap();

        shell.stop_backend_runtime("mesh.audio");
        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pipewire-audio",
                serde_json::json!({ "available": true, "percent": 5.0 }),
            ))
            .unwrap();

        assert_eq!(seen_events.lock().unwrap().len(), 1);
        assert_eq!(
            shell
                .latest_service_state
                .get("mesh.audio")
                .and_then(|state| state.state.get("percent")),
            Some(&serde_json::json!(40.0))
        );
    }

    #[test]
    fn shell_theme_update_is_authoritative_when_theme_provider_is_active() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        let seen_events = Arc::new(Mutex::new(Vec::new()));
        shell
            .components
            .push(super::types::ComponentRuntime::new(Box::new(
                RecordingComponent::new(seen_events.clone()),
            )));
        let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
        shell.replace_backend_runtime("mesh.theme".to_string(), slot);

        shell
            .broadcast_service_event(service_update(
                "mesh.theme",
                "@mesh/shell",
                serde_json::json!({
                    "current": "mesh-default-light",
                    "theme_id": "mesh-default-light",
                    "is_dark": false,
                }),
            ))
            .unwrap();

        assert_eq!(seen_events.lock().unwrap().len(), 1);
        assert_eq!(
            shell
                .latest_service_state
                .get("mesh.theme")
                .and_then(|state| state.state.get("current")),
            Some(&serde_json::json!("mesh-default-light"))
        );
    }

    #[test]
    fn shell_theme_backend_candidate_receives_resolved_active_theme_setting() {
        let mut shell = Shell::new();
        shell.settings.theme.active = "missing-theme".to_string();
        let (theme, theme_watch) = load_active_theme(&shell.settings);
        shell.theme = theme;
        shell.theme_watch = theme_watch;
        let mut candidate = BackendLaunchCandidate {
            module_id: "@mesh/shell-theme".to_string(),
            interface: "mesh.theme".to_string(),
            service_name: "theme".to_string(),
            entrypoint_path: PathBuf::from("src/main.luau"),
            script_source: String::new(),
            capabilities: Vec::new(),
            settings: serde_json::json!({}),
        };

        shell.apply_shell_runtime_settings(&mut candidate);

        assert_eq!(shell.theme.active().id, "mesh-default-dark");
        assert_eq!(
            candidate
                .settings
                .get("current_theme")
                .and_then(|value| value.as_str()),
            Some("mesh-default-dark")
        );
    }

    #[test]
    fn shell_theme_fallback_backend_restart_keeps_latest_state_on_resolved_theme() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        shell.settings.theme.active = "missing-theme".to_string();
        let (theme, theme_watch) = load_active_theme(&shell.settings);
        shell.theme = theme;
        shell.theme_watch = theme_watch;

        let mut candidate = BackendLaunchCandidate {
            module_id: "@mesh/shell-theme".to_string(),
            interface: "mesh.theme".to_string(),
            service_name: "theme".to_string(),
            entrypoint_path: PathBuf::from("src/main.luau"),
            script_source: String::new(),
            capabilities: Vec::new(),
            settings: serde_json::json!({}),
        };
        shell.apply_shell_runtime_settings(&mut candidate);
        let current_theme = candidate
            .settings
            .get("current_theme")
            .and_then(|value| value.as_str())
            .unwrap()
            .to_string();

        let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
        shell.replace_backend_runtime("mesh.theme".to_string(), slot);
        shell
            .broadcast_service_event(service_update(
                "mesh.theme",
                "@mesh/shell-theme",
                serde_json::json!({
                    "current": current_theme,
                    "is_dark": true,
                    "available": ["mesh-default-dark", "mesh-default-light"],
                }),
            ))
            .unwrap();

        let (replacement_slot, _replacement_rx) =
            backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
        shell.replace_backend_runtime("mesh.theme".to_string(), replacement_slot);
        shell
            .broadcast_service_event(service_update(
                "mesh.theme",
                "@mesh/shell-theme",
                serde_json::json!({
                    "current": "mesh-default-dark",
                    "is_dark": true,
                    "available": ["mesh-default-dark", "mesh-default-light"],
                }),
            ))
            .unwrap();

        let latest = shell.latest_service_state.get("mesh.theme").unwrap();
        assert_eq!(shell.theme.active().id, "mesh-default-dark");
        assert_eq!(latest.state["current"], serde_json::json!("mesh-default-dark"));
        assert_eq!(latest.state["is_dark"], serde_json::json!(true));
    }

    #[test]
    fn settings_theme_reload_syncs_theme_service_state() {
        let _env_lock = SETTINGS_ENV_LOCK.lock().unwrap();
        let runtime = Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("shell-settings.json");
        fs::write(
            &settings_path,
            r#"{"theme":{"active":"mesh-default-dark"}}"#,
        )
        .unwrap();
        let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
        let mut shell = Shell::new();
        let seen_events = Arc::new(Mutex::new(Vec::new()));
        shell
            .components
            .push(super::types::ComponentRuntime::new(Box::new(
                RecordingComponent::new(seen_events.clone()),
            )));
        let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
        shell.replace_backend_runtime("mesh.theme".to_string(), slot);

        fs::write(
            &settings_path,
            r#"{"theme":{"active":"mesh-default-light"}}"#,
        )
        .unwrap();
        shell.settings_watch.modified_at = None;
        shell.reload_locale_if_settings_changed().unwrap();

        assert_eq!(shell.settings.theme.active, "mesh-default-light");
        assert_eq!(seen_events.lock().unwrap().len(), 1);
        assert_eq!(
            shell
                .latest_service_state
                .get("mesh.theme")
                .and_then(|state| state.state.get("current")),
            Some(&serde_json::json!("mesh-default-light"))
        );
        assert_eq!(
            shell
                .latest_service_state
                .get("mesh.theme")
                .and_then(|state| state.state.get("is_dark")),
            Some(&serde_json::json!(false))
        );
    }

    #[test]
    fn settings_theme_reload_publishes_resolved_fallback_theme_state() {
        let _env_lock = SETTINGS_ENV_LOCK.lock().unwrap();
        let runtime = Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("shell-settings.json");
        fs::write(
            &settings_path,
            r#"{"theme":{"active":"mesh-default-dark"}}"#,
        )
        .unwrap();
        let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
        let mut shell = Shell::new();
        let seen_events = Arc::new(Mutex::new(Vec::new()));
        shell
            .components
            .push(super::types::ComponentRuntime::new(Box::new(
                RecordingComponent::new(seen_events.clone()),
            )));
        let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
        shell.replace_backend_runtime("mesh.theme".to_string(), slot);

        fs::write(&settings_path, r#"{"theme":{"active":"missing-theme"}}"#).unwrap();
        shell.settings_watch.modified_at = None;
        shell.reload_locale_if_settings_changed().unwrap();

        assert_eq!(shell.settings.theme.active, "missing-theme");
        assert_eq!(shell.theme.active().id, "mesh-default-dark");
        assert_eq!(seen_events.lock().unwrap().len(), 1);
        assert_eq!(
            shell
                .latest_service_state
                .get("mesh.theme")
                .and_then(|state| state.state.get("current")),
            Some(&serde_json::json!("mesh-default-dark"))
        );
    }

    #[test]
    fn theme_file_recovery_syncs_mesh_theme_latest_state_and_components() {
        let _env_lock = SETTINGS_ENV_LOCK.lock().unwrap();
        let runtime = Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let theme_dir = dir.path().join("themes");
        fs::create_dir_all(&theme_dir).unwrap();
        let settings_path = dir.path().join("shell-settings.json");
        fs::write(
            &settings_path,
            r#"{"theme":{"active":"mesh-recovered-light"}}"#,
        )
        .unwrap();
        let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
        let _theme_dir = EnvGuard::set("MESH_THEME_DIR", &theme_dir);
        let mut shell = Shell::new();
        let seen_events = Arc::new(Mutex::new(Vec::new()));
        shell
            .components
            .push(super::types::ComponentRuntime::new(Box::new(
                RecordingComponent::new(seen_events.clone()),
            )));
        let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
        shell.replace_backend_runtime("mesh.theme".to_string(), slot);

        assert_eq!(shell.settings.theme.active, "mesh-recovered-light");
        assert_eq!(shell.theme.active().id, "mesh-default-dark");
        let fallback_theme_id = shell.theme.active().id.clone();
        shell
            .sync_theme_service_state(&fallback_theme_id)
            .unwrap();
        assert_eq!(
            shell
                .latest_service_state
                .get("mesh.theme")
                .and_then(|state| state.state.get("current")),
            Some(&serde_json::json!("mesh-default-dark"))
        );
        assert_eq!(
            shell
                .latest_service_state
                .get("mesh.theme")
                .and_then(|state| state.state.get("is_dark")),
            Some(&serde_json::json!(true))
        );

        fs::write(
            theme_dir.join("mesh-recovered-light.json"),
            r#"{"id":"mesh-recovered-light","name":"Recovered Light","tokens":{}}"#,
        )
        .unwrap();
        let requests = shell.reload_theme_if_changed().unwrap();

        assert!(requests.is_empty());
        assert_eq!(shell.theme.active().id, "mesh-recovered-light");
        assert_eq!(
            shell
                .latest_service_state
                .get("mesh.theme")
                .and_then(|state| state.state.get("current")),
            Some(&serde_json::json!("mesh-recovered-light"))
        );
        assert_eq!(
            shell
                .latest_service_state
                .get("mesh.theme")
                .and_then(|state| state.state.get("is_dark")),
            Some(&serde_json::json!(false))
        );

        let events = seen_events.lock().unwrap();
        assert_eq!(events.len(), 2);
        let ServiceEvent::Updated { payload, .. } = events.last().unwrap();
        assert_eq!(payload["current"], serde_json::json!("mesh-recovered-light"));
        assert_eq!(payload["is_dark"], serde_json::json!(false));
    }

    #[test]
    fn service_contract_provider_declaration_requires_provider_pair() {
        let graph = graph_from_json(
            r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": { "mesh.audio": "@mesh/backend" }
            }"#,
            vec![
                r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "provides": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
            ],
        );
        let (_dir, plugin) = plugin_instance("@mesh/backend", Some("src/main.luau"));
        let plugins = HashMap::from([("@mesh/backend".to_string(), plugin)]);
        let interfaces = InterfaceRegistry::new();
        interfaces.register_contract(test_contract("mesh.audio"));

        let (candidates, statuses) =
            backend_launch_candidates_from_graph(&graph, &plugins, &test_config(), &interfaces);

        assert!(candidates.is_empty());
        assert!(statuses.iter().any(|status| {
            status.status == "invalid_manifest"
                && status.provider_id.as_deref() == Some("@mesh/backend")
                && status.message.contains("not registered")
        }));

        register_test_provider(&interfaces, "mesh.audio", "@mesh/backend");
        let (candidates, statuses) =
            backend_launch_candidates_from_graph(&graph, &plugins, &test_config(), &interfaces);

        assert_eq!(candidates.len(), 1);
        assert!(
            statuses
                .iter()
                .all(|status| status.provider_id.as_deref() != Some("@mesh/backend"))
        );
    }

    #[test]
    fn state_shape_mismatch_records_service_contract_warning() {
        let mut shell = Shell::new();
        shell
            .interfaces
            .register_contract(test_contract("mesh.audio"));
        register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");

        shell
            .broadcast_service_event(service_update(
                "mesh.audio",
                "@mesh/pipewire-audio",
                serde_json::json!({ "available": true, "percent": "loud" }),
            ))
            .unwrap();

        let snapshot = shell.diagnostics.snapshot();
        assert!(snapshot.iter().any(|(plugin_id, health)| {
            plugin_id == "@mesh/pipewire-audio"
                && health.to_string().contains("service_contract_warning")
        }));
        let latest = shell.latest_service_state.get("mesh.audio").unwrap();
        assert!(latest.state.get("source_plugin").is_none());
    }

    #[test]
    fn service_contract_unknown_service_command_returns_failure_result() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        shell
            .interfaces
            .register_contract(test_contract("mesh.audio"));
        register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
        let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), slot);
        let mut capabilities = mesh_core_capability::CapabilitySet::new();
        capabilities.grant(mesh_core_capability::Capability::new(
            "service.audio.control",
        ));

        let result = shell.dispatch_service_command(
            "mesh.audio",
            "explode",
            &serde_json::json!({}),
            "@mesh/panel",
            &capabilities,
        );

        assert_eq!(result["ok"], serde_json::json!(false));
        assert_eq!(
            result["status"],
            serde_json::json!("unsupported_service_command")
        );
        assert!(rx.try_recv().is_err());
        assert!(
            shell
                .diagnostics
                .snapshot()
                .iter()
                .any(|(plugin_id, health)| {
                    plugin_id == "@mesh/panel"
                        && health.to_string().contains("unsupported_service_command")
                })
        );
    }

    #[test]
    fn closed_service_command_channel_returns_unavailable_result() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        shell
            .interfaces
            .register_contract(test_contract("mesh.audio"));
        register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
        let (slot, rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
        drop(rx);
        shell.replace_backend_runtime("mesh.audio".to_string(), slot);
        let mut capabilities = mesh_core_capability::CapabilitySet::new();
        capabilities.grant(mesh_core_capability::Capability::new(
            "service.audio.control",
        ));

        let result = shell.dispatch_service_command(
            "mesh.audio",
            "set_volume",
            &serde_json::json!({ "volume": 0.4 }),
            "@mesh/panel",
            &capabilities,
        );

        assert_eq!(result["ok"], serde_json::json!(false));
        assert_eq!(result["status"], serde_json::json!("service_unavailable"));
    }

    #[test]
    fn launcher_content_size_ignores_root_surface_bounds() {
        let mut root = node("root", 0.0, 0.0, 640.0, 360.0);
        root.children.push(node("column", 12.0, 12.0, 336.0, 332.0));

        let launcher_layout = SurfaceLayoutSection {
            size_policy: Some("content_measured".into()),
            prefers_content_children_sizing: Some(true),
            min_width: Some(320),
            max_width: Some(640),
            min_height: Some(180),
            max_height: Some(420),
        };
        assert_eq!(
            measure_content_size(&root, 640, 360, Some(&launcher_layout)),
            (348, 344)
        );
    }

    #[test]
    fn non_widget_surfaces_keep_fallback_size() {
        let mut root = node("root", 0.0, 0.0, 1920.0, 32.0);
        root.children.push(node("row", 0.0, 0.0, 640.0, 32.0));

        assert_eq!(measure_content_size(&root, 1920, 32, None), (1920, 32));
    }

    #[test]
    fn launcher_plugin_json_declares_content_measured_policy() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let launcher_plugin_dir = workspace_root.join("packages/plugins/frontend/core/launcher");
        let plugin_json = std::fs::read_to_string(launcher_plugin_dir.join("plugin.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&plugin_json).unwrap();
        assert_eq!(
            value
                .pointer("/surface_layout/size_policy")
                .and_then(|v| v.as_str()),
            Some("content_measured"),
        );
        assert_eq!(
            value
                .pointer("/surface_layout/prefers_content_children_sizing")
                .and_then(|v| v.as_bool()),
            Some(true),
        );
    }

    #[test]
    fn installed_module_graph_exposes_shell_package_choices() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let graph = mesh_core_plugin::package::load_installed_module_graph(
            &workspace_root.join("config/package.json"),
        )
        .unwrap();

        assert_eq!(
            graph.active_provider("mesh.audio").unwrap().module_id,
            "@mesh/pipewire-audio"
        );
        assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);

        let layout = graph.layout_entrypoint().unwrap();
        assert_eq!(layout.module_id, "@mesh/panel");
        assert_eq!(layout.entrypoint_id, "main");
    }

    #[test]
    fn backend_lifecycle_uses_explicit_active_provider_from_package_graph() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let graph = mesh_core_plugin::package::load_installed_module_graph(
            &workspace_root.join("config/package.json"),
        )
        .unwrap();
        let (_pipewire_dir, pipewire) =
            plugin_instance("@mesh/pipewire-audio", Some("src/main.luau"));
        let (_pulse_dir, pulse) = plugin_instance("@mesh/pulseaudio-audio", Some("src/main.luau"));
        let plugins = HashMap::from([
            ("@mesh/pipewire-audio".to_string(), pipewire),
            ("@mesh/pulseaudio-audio".to_string(), pulse),
        ]);

        let (candidates, statuses) = backend_launch_candidates_from_graph(
            &graph,
            &plugins,
            &test_config(),
            &InterfaceRegistry::new(),
        );

        assert!(
            statuses
                .iter()
                .all(|status| status.status != "invalid_manifest")
        );
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].interface, "mesh.audio");
        assert_eq!(candidates[0].module_id, "@mesh/pipewire-audio");
        assert_eq!(candidates[0].service_name, "audio");
        assert!(candidates[0].entrypoint_path.ends_with("src/main.luau"));
        assert!(
            !candidates
                .iter()
                .any(|candidate| candidate.module_id == "@mesh/pulseaudio-audio")
        );
    }

    #[test]
    fn backend_lifecycle_rejects_missing_backend_entrypoint_before_launch() {
        let graph = graph_from_json(
            r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": { "mesh.audio": "@mesh/backend" }
            }"#,
            vec![
                r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "provides": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
            ],
        );
        let (_dir, plugin) = plugin_instance("@mesh/backend", None);
        let plugins = HashMap::from([("@mesh/backend".to_string(), plugin)]);

        let (candidates, statuses) = backend_launch_candidates_from_graph(
            &graph,
            &plugins,
            &test_config(),
            &InterfaceRegistry::new(),
        );

        assert!(candidates.is_empty());
        assert!(statuses.iter().any(|status| {
            status.status == "missing_entrypoint"
                && status.provider_id.as_deref() == Some("@mesh/backend")
        }));
    }

    #[test]
    fn backend_lifecycle_excludes_disabled_backend_modules() {
        let graph = graph_from_json(
            r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/frontend": { "kind": "frontend", "path": "@mesh/frontend", "enabled": true },
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": false }
              },
              "providers": {}
            }"#,
            vec![
                r#"{
                  "name": "@mesh/frontend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "frontend",
                    "dependencies": { "backend": { "mesh.audio": ">=1.0.0" } }
                  }
                }"#,
                r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "provides": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
            ],
        );
        let (_dir, plugin) = plugin_instance("@mesh/backend", Some("src/main.luau"));
        let plugins = HashMap::from([("@mesh/backend".to_string(), plugin)]);

        let (candidates, statuses) = backend_launch_candidates_from_graph(
            &graph,
            &plugins,
            &test_config(),
            &InterfaceRegistry::new(),
        );

        assert!(candidates.is_empty());
        assert!(statuses.iter().any(|status| {
            status.status == "unmet_backend_requirement" && status.interface == "mesh.audio"
        }));
    }

    #[test]
    fn backend_lifecycle_reports_frontend_requirement_without_active_provider() {
        let graph = graph_from_json(
            r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/frontend": { "kind": "frontend", "path": "@mesh/frontend", "enabled": true },
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": {}
            }"#,
            vec![
                r#"{
                  "name": "@mesh/frontend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "frontend",
                    "dependencies": { "backend": { "mesh.audio": ">=1.0.0" } }
                  }
                }"#,
                r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "provides": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
            ],
        );
        let (_dir, plugin) = plugin_instance("@mesh/backend", Some("src/main.luau"));
        let plugins = HashMap::from([("@mesh/backend".to_string(), plugin)]);

        let (candidates, statuses) = backend_launch_candidates_from_graph(
            &graph,
            &plugins,
            &test_config(),
            &InterfaceRegistry::new(),
        );

        assert!(candidates.is_empty());
        assert!(statuses.iter().any(|status| {
            status.status == "no_active_provider" && status.interface == "mesh.audio"
        }));
    }

    #[test]
    fn backend_lifecycle_reports_frontend_requirement_without_installed_provider() {
        let graph = graph_from_json(
            r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/frontend": { "kind": "frontend", "path": "@mesh/frontend", "enabled": true }
              },
              "providers": {}
            }"#,
            vec![
                r#"{
                  "name": "@mesh/frontend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "frontend",
                    "dependencies": { "backend": { "mesh.network": ">=1.0.0" } }
                  }
                }"#,
            ],
        );

        let (candidates, statuses) = backend_launch_candidates_from_graph(
            &graph,
            &HashMap::new(),
            &test_config(),
            &InterfaceRegistry::new(),
        );

        assert!(candidates.is_empty());
        assert!(statuses.iter().any(|status| {
            status.status == "unmet_backend_requirement" && status.interface == "mesh.network"
        }));
    }

    #[test]
    fn backend_lifecycle_status_names_match_phase_contract() {
        let statuses = [
            BackendRuntimeStatus::NoActiveProvider,
            BackendRuntimeStatus::UnmetBackendRequirement,
            BackendRuntimeStatus::InvalidManifest,
            BackendRuntimeStatus::MissingEntrypoint,
            BackendRuntimeStatus::MissingBinary,
            BackendRuntimeStatus::InitFailed,
            BackendRuntimeStatus::Running,
            BackendRuntimeStatus::PollFailed,
            BackendRuntimeStatus::Failed,
            BackendRuntimeStatus::Stopped,
        ]
        .map(BackendRuntimeStatus::as_str);

        assert_eq!(
            statuses,
            [
                "no_active_provider",
                "unmet_backend_requirement",
                "invalid_manifest",
                "missing_entrypoint",
                "missing_binary",
                "init_failed",
                "running",
                "poll_failed",
                "failed",
                "stopped",
            ]
        );
    }

    #[test]
    fn backend_lifecycle_replacement_removes_old_command_sender_before_insert() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
        let old_sender = old_slot.command_tx.clone();
        shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);

        let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
        let new_sender = new_slot.command_tx.clone();
        shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

        assert!(!old_sender.is_closed());
        assert!(!new_sender.is_closed());
        assert_eq!(
            shell
                .backend_runtimes
                .get("mesh.audio")
                .map(|slot| slot.provider_id.as_str()),
            Some("@mesh/new-audio")
        );
        assert!(shell.service_handlers.contains_key("mesh.audio"));
    }

    #[test]
    fn backend_lifecycle_replacement_records_stopped_after_transient_poll_failure() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
        shell.record_backend_runtime_status(
            "mesh.audio".to_string(),
            "@mesh/old-audio".to_string(),
            BackendRuntimeStatus::PollFailed,
            "temporary poll failure".to_string(),
        );

        let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

        assert_eq!(
            shell
                .backend_runtime_statuses
                .get(&("mesh.audio".to_string(), "@mesh/old-audio".to_string()))
                .map(|entry| entry.status.as_str()),
            Some("stopped")
        );
    }

    #[test]
    fn backend_lifecycle_init_failure_removes_command_handler() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), slot);

        shell.handle_backend_lifecycle(
            "mesh.audio".to_string(),
            "@mesh/pipewire-audio".to_string(),
            "init".to_string(),
            "init_failed".to_string(),
            "init boom".to_string(),
        );

        assert!(!shell.service_handlers.contains_key("mesh.audio"));
        assert!(!shell.backend_runtimes.contains_key("mesh.audio"));
        assert_eq!(
            shell
                .backend_runtime_statuses
                .get(&("mesh.audio".to_string(), "@mesh/pipewire-audio".to_string()))
                .map(|entry| entry.status.as_str()),
            Some("init_failed")
        );
    }

    #[test]
    fn stale_backend_lifecycle_event_does_not_stop_current_provider() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);

        let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

        shell.handle_backend_lifecycle(
            "mesh.audio".to_string(),
            "@mesh/old-audio".to_string(),
            "poll".to_string(),
            "failed".to_string(),
            "old provider failed after replacement".to_string(),
        );

        assert!(shell.service_handlers.contains_key("mesh.audio"));
        assert_eq!(
            shell
                .backend_runtimes
                .get("mesh.audio")
                .map(|slot| slot.provider_id.as_str()),
            Some("@mesh/new-audio")
        );
        assert_eq!(
            shell
                .backend_runtime_statuses
                .get(&("mesh.audio".to_string(), "@mesh/old-audio".to_string()))
                .map(|entry| entry.status.as_str()),
            Some("failed")
        );
    }

    #[test]
    fn backend_lifecycle_failed_runtime_does_not_start_fallback_provider() {
        let runtime = Runtime::new().unwrap();
        let mut shell = Shell::new();
        let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), slot);

        shell.handle_backend_lifecycle(
            "mesh.audio".to_string(),
            "@mesh/pipewire-audio".to_string(),
            "poll".to_string(),
            "failed".to_string(),
            "poll boom".to_string(),
        );

        assert!(!shell.service_handlers.contains_key("mesh.audio"));
        assert!(
            !shell
                .backend_runtimes
                .values()
                .any(|slot| slot.provider_id == "@mesh/pulseaudio-audio")
        );
        assert_eq!(
            shell
                .backend_runtime_statuses
                .get(&("mesh.audio".to_string(), "@mesh/pipewire-audio".to_string()))
                .map(|entry| entry.status.as_str()),
            Some("failed")
        );
    }

    #[test]
    fn debug_snapshot_includes_backend_lifecycle_status() {
        let mut shell = Shell::new();
        shell.record_backend_runtime_status(
            "mesh.audio".to_string(),
            "@mesh/pipewire-audio".to_string(),
            BackendRuntimeStatus::Running,
            "backend runtime started".to_string(),
        );

        let snapshot = shell.build_debug_snapshot();
        assert!(snapshot.backend_runtimes.iter().any(|entry| {
            entry.interface == "mesh.audio"
                && entry.provider_id == "@mesh/pipewire-audio"
                && entry.status == "running"
        }));
    }

    #[test]
    fn frontend_settings_override_surface_layout_defaults() {
        let path = unique_test_file("surface-layout");
        fs::write(
            &path,
            r#"{
  "surface": {
    "anchor": "left",
    "layer": "overlay",
    "width": 960,
    "height": 640,
    "exclusive_zone": 12,
    "keyboard_mode": "exclusive",
    "visible_on_start": true
  }
}"#,
        )
        .unwrap();

        let manifest = minimal_manifest("@mesh/base-surface");
        let settings = load_frontend_plugin_settings(&path, &manifest);
        fs::remove_file(&path).ok();

        assert_eq!(settings.layout.edge, mesh_core_wayland::Edge::Left);
        assert_eq!(settings.layout.layer, mesh_core_wayland::Layer::Overlay);
        assert_eq!(settings.layout.width, 960);
        assert_eq!(settings.layout.height, 640);
        assert_eq!(settings.layout.exclusive_zone, 12);
        assert_eq!(
            settings.layout.keyboard_mode,
            mesh_core_wayland::KeyboardMode::Exclusive
        );
        assert!(settings.layout.visible_on_start);
        assert_eq!(settings.layout.size_policy, SurfaceSizePolicy::Fixed);
    }

    #[test]
    fn service_update_populates_frontend_state() {
        let mut state = ScriptState::new();
        seed_service_state(&mut state);
        apply_service_update(
            &mut state,
            true,
            "audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 65, "label": "65%" }),
        );

        let audio = state.get("audio").expect("audio state should exist");
        assert_eq!(audio.get("label").and_then(|v| v.as_str()), Some("65%"));
        assert_eq!(audio.get("percent").and_then(|v| v.as_u64()), Some(65));
    }

    #[test]
    fn service_update_gated_by_capability() {
        let mut state = ScriptState::new();
        seed_service_state(&mut state);
        apply_service_update(
            &mut state,
            false, // no audio.read capability
            "audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 99 }),
        );
        assert!(state.get("audio").is_none());
    }

    #[test]
    fn service_update_accepts_canonical_interface_name() {
        let mut state = ScriptState::new();
        seed_service_state(&mut state);
        apply_service_update(
            &mut state,
            true,
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 42 }),
        );
        assert_eq!(
            state
                .get("last_service_update")
                .and_then(|v| v.get("name").cloned())
                .and_then(|v| v.as_str().map(str::to_string)),
            Some("audio".to_string())
        );
    }

    #[test]
    fn normalizes_service_names_from_interfaces() {
        assert_eq!(service_name_from_interface("mesh.audio"), "audio");
        assert_eq!(service_name_from_interface("audio"), "audio");
    }

    fn unique_test_file(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("mesh-{prefix}-{nanos}.json"))
    }
}
