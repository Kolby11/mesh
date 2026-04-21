use mesh_capability::{Capability, CapabilitySet};
/// The top-level Shell struct that owns shell coordination and plugin loading.
use mesh_component_backend::{
    CompiledFrontendPlugin, FrontendCompositionResolver, FrontendRenderMode,
    compile_frontend_plugin, is_frontend_plugin, root_accessibility_role,
};
use mesh_config::{
    ShellConfig, ShellSettings, default_settings_path, load_config, load_shell_settings,
};
use mesh_diagnostics::DiagnosticsCollector;
use mesh_events::EventBus;
use mesh_locale::LocaleEngine;
use mesh_plugin::lifecycle::{PluginInstance, PluginState};
use mesh_plugin::{DependencyGraphError, PluginType, manifest, validate_plugin_dependency_graph};
use mesh_debug::{DebugOverlayState, DebugSnapshot, HealthEntry, InterfaceEntry, PluginEntry, ProviderEntry};
use mesh_renderer::{
    DebugOverlay, DevWindowBackend, DevWindowEvent, DevWindowKeyEvent, LayerShellBackend,
    LayerSurfaceConfig, Painter, PixelBuffer,
};
use mesh_scripting::{LocaleBoundState, PublishedEvent, ScriptContext, ScriptError, ScriptState};
use mesh_service::{
    InterfaceProvider, InterfaceRegistry, ServiceRegistry, canonical_interface_name,
    load_interface_contract,
};
use mesh_theme::{Theme, ThemeEngine, default_theme, load_theme_from_path, theme_path_for_id};
use mesh_ui::WidgetNode;
use mesh_wayland::{Edge, KeyboardMode, Layer, ShellSurface, StubSurface};

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::SystemTime;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

pub type SurfaceId = String;

#[derive(Debug, Clone)]
pub enum CoreRequest {
    ToggleSurface { surface_id: SurfaceId },
    ShowSurface { surface_id: SurfaceId },
    HideSurface { surface_id: SurfaceId },
    PublishDiagnostics { message: String },
    ToggleDebugOverlay,
    CycleDebugTab,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum CoreEvent {
    Started,
    SurfaceVisibilityChanged {
        surface_id: SurfaceId,
        visible: bool,
    },
    ShuttingDown,
}

#[derive(Debug, Clone)]
pub enum ServiceEvent {
    Updated {
        service: String,
        source_plugin: String,
        summary: String,
    },
}

#[derive(Debug, Clone)]
pub struct ComponentContext {
    pub component_id: String,
    pub surface_id: SurfaceId,
}

#[derive(Debug, Clone)]
pub enum ComponentInput {
    PointerMove { x: f32, y: f32 },
    PointerButton { x: f32, y: f32, pressed: bool },
    Scroll { x: f32, y: f32, dx: f32, dy: f32 },
    KeyPressed { key: String },
    KeyReleased { key: String },
    Char { ch: char },
}

#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    #[error("component '{component_id}' failed: {message}")]
    Failed {
        component_id: String,
        message: String,
    },

    #[error("component '{component_id}' script error: {source}")]
    Script {
        component_id: String,
        #[source]
        source: ScriptError,
    },
}

pub trait ShellComponent: Send {
    fn id(&self) -> &str;
    fn surface_id(&self) -> &str;
    fn initial_visibility(&self) -> Option<bool> {
        None
    }
    fn mount(&mut self, ctx: ComponentContext) -> Result<Vec<CoreRequest>, ComponentError>;
    fn handle_core_event(&mut self, event: &CoreEvent) -> Result<Vec<CoreRequest>, ComponentError>;
    fn handle_service_event(
        &mut self,
        event: &ServiceEvent,
    ) -> Result<Vec<CoreRequest>, ComponentError>;
    fn tick(&mut self) -> Result<Vec<CoreRequest>, ComponentError>;
    fn wants_render(&self) -> bool;
    fn render(&mut self, surface: &mut dyn ShellSurface) -> Result<(), ComponentError>;
    fn paint(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        buffer: &mut PixelBuffer,
    ) -> Result<(), ComponentError>;
    fn theme_changed(&mut self) -> Result<(), ComponentError>;
    fn locale_changed(&mut self, _locale: &LocaleEngine) -> Result<(), ComponentError> {
        Ok(())
    }
    fn handle_input(
        &mut self,
        _theme: &Theme,
        _width: u32,
        _height: u32,
        _input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        Ok(Vec::new())
    }
    fn source_path(&self) -> Option<&Path> {
        None
    }
    fn reload_source(&mut self) -> Result<bool, ComponentError> {
        Ok(false)
    }
    fn plugin_settings_path(&self) -> Option<&Path> {
        None
    }
    fn reload_plugin_settings(&mut self) -> Result<bool, ComponentError> {
        Ok(false)
    }
    /// Return the last widget tree built by `paint`, for the debug layout inspector.
    fn last_widget_tree(&self) -> Option<&WidgetNode> {
        None
    }
}

struct ComponentRuntime {
    surface_id: SurfaceId,
    component: Box<dyn ShellComponent>,
    source_path: Option<PathBuf>,
    source_modified_at: Option<SystemTime>,
    plugin_settings_path: Option<PathBuf>,
    plugin_settings_modified_at: Option<SystemTime>,
}

impl ComponentRuntime {
    fn new(component: Box<dyn ShellComponent>) -> Self {
        let surface_id = component.surface_id().to_string();
        let source_path = component.source_path().map(PathBuf::from);
        let source_modified_at = source_path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).ok())
            .and_then(|metadata| metadata.modified().ok());
        let plugin_settings_path = component.plugin_settings_path().map(PathBuf::from);
        Self {
            surface_id,
            component,
            source_path,
            source_modified_at,
            plugin_settings_path,
            plugin_settings_modified_at: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ThemeWatchState {
    path: PathBuf,
    modified_at: Option<SystemTime>,
}

#[derive(Debug, Clone)]
struct SettingsWatchState {
    path: PathBuf,
    modified_at: Option<SystemTime>,
}

#[derive(Debug)]
enum ShellMessage {
    Service(ServiceEvent),
    Ipc(CoreRequest),
}

#[derive(Debug, Default)]
struct ShellCoreState {
    surfaces: HashMap<SurfaceId, SurfaceState>,
    shutting_down: bool,
}

#[derive(Debug, Clone)]
struct SurfaceState {
    visible: bool,
}

impl Default for SurfaceState {
    fn default() -> Self {
        Self { visible: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceSizePolicy {
    Fixed,
    ContentMeasured,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SurfaceLayoutSettings {
    edge: Edge,
    layer: Layer,
    width: u32,
    height: u32,
    exclusive_zone: i32,
    keyboard_mode: KeyboardMode,
    visible_on_start: bool,
    size_policy: SurfaceSizePolicy,
}

#[derive(Debug, Clone)]
struct FrontendPluginSettingsState {
    raw: serde_json::Value,
    layout: SurfaceLayoutSettings,
}

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
    windows: WindowBackend,
    theme_watch: ThemeWatchState,
    settings_watch: SettingsWatchState,
    debug: DebugOverlayState,
    debug_overlay: DebugOverlay,
}

enum WindowBackend {
    LayerShell(LayerShellBackend),
    DevWindow(DevWindowBackend),
}

impl WindowBackend {
    fn select() -> Self {
        let forced = std::env::var("MESH_BACKEND").ok();
        let want_dev = forced.as_deref() == Some("dev-window");
        let want_layer = forced.as_deref() == Some("layer-shell");
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();

        if !want_dev && (want_layer || wayland) {
            match LayerShellBackend::new() {
                Ok(backend) => {
                    tracing::info!("using wlr-layer-shell backend");
                    return WindowBackend::LayerShell(backend);
                }
                Err(err) => {
                    tracing::warn!(
                        "failed to initialise layer-shell backend, falling back to dev window: {err}"
                    );
                }
            }
        }
        tracing::info!("using dev-window backend (minifb)");
        WindowBackend::DevWindow(DevWindowBackend::new())
    }

    fn configure(&mut self, surface_id: &str, cfg: LayerSurfaceConfig) {
        if let WindowBackend::LayerShell(backend) = self {
            backend.configure(surface_id, cfg);
        }
    }

    fn present(
        &mut self,
        surface_id: &str,
        title: &str,
        visible: bool,
        buffer: &PixelBuffer,
    ) -> Result<(), mesh_renderer::RenderError> {
        match self {
            WindowBackend::LayerShell(b) => b.present(surface_id, title, visible, buffer),
            WindowBackend::DevWindow(b) => b.present(surface_id, title, visible, buffer),
        }
    }

    fn pump(&mut self) {
        match self {
            WindowBackend::LayerShell(b) => b.pump(),
            WindowBackend::DevWindow(b) => b.pump(),
        }
    }

    fn poll_events(&mut self) -> Vec<DevWindowEvent> {
        match self {
            WindowBackend::LayerShell(b) => b.poll_events(),
            WindowBackend::DevWindow(b) => b.poll_events(),
        }
    }
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
        let config_path = mesh_config::default_config_path();
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
            windows: WindowBackend::select(),
            theme_watch,
            settings_watch,
            debug: DebugOverlayState::default(),
            debug_overlay: DebugOverlay::new(),
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
            match manifest::load_manifest(dir) {
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
                InterfaceEntry { name: name.clone(), providers }
            })
            .collect();
        interfaces.sort_by(|a, b| a.name.cmp(&b.name));

        let health = self
            .diagnostics
            .snapshot()
            .into_iter()
            .map(|(id, status)| HealthEntry { plugin_id: id, status: status.to_string() })
            .collect();

        let active_surfaces = self
            .core
            .surfaces
            .iter()
            .filter(|(_, s)| s.visible)
            .map(|(id, _)| id.clone())
            .collect();

        DebugSnapshot { plugins, interfaces, health, active_surfaces }
    }

    pub fn plugins(&self) -> impl Iterator<Item = (&str, PluginState)> {
        self.plugins
            .iter()
            .map(|(id, inst)| (id.as_str(), inst.state))
    }

    pub fn run(&mut self) -> Result<(), ShellRunError> {
        self.discover_plugins();
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
        self.mark_components_locale_changed()?;
        pending.extend(self.broadcast_core_event(CoreEvent::Started)?);

        tracing::info!(
            "MESH shell core is running with {} frontend component(s)",
            self.components.len()
        );

        while !self.core.shutting_down {
            self.reload_theme_if_changed()?;
            self.reload_locale_if_settings_changed()?;
            self.reload_plugin_settings_if_changed()?;
            self.reload_frontend_components_if_changed()?;
            self.dispatch_wayland()?;

            while let Ok(message) = rx.try_recv() {
                match message {
                    ShellMessage::Service(event) => {
                        pending.extend(self.broadcast_service_event(event)?);
                    }
                    ShellMessage::Ipc(request) => {
                        pending.push_back(request);
                    }
                }
            }

            pending.extend(self.tick_components()?);
            self.drain_requests(&mut pending)?;
            self.render_components()?;
            self.flush_wayland()?;
            self.windows.pump();

            std::thread::sleep(Duration::from_millis(16));
        }

        let mut shutdown_requests = self.broadcast_core_event(CoreEvent::ShuttingDown)?;
        self.drain_requests(&mut shutdown_requests)?;
        let _ = std::fs::remove_file(&ipc_socket_path);
        tracing::info!("shell event loop stopped");
        Ok(())
    }

    fn reload_theme_if_changed(&mut self) -> Result<(), ShellRunError> {
        let Ok(metadata) = std::fs::metadata(&self.theme_watch.path) else {
            return Ok(());
        };
        let Ok(modified_at) = metadata.modified() else {
            return Ok(());
        };

        if self.theme_watch.modified_at == Some(modified_at) {
            return Ok(());
        }

        let theme = load_theme_from_path(&self.theme_watch.path).map_err(ShellRunError::Theme)?;
        tracing::info!(
            "reloaded active theme '{}' from {}",
            theme.id,
            self.theme_watch.path.display()
        );
        self.theme.replace_active(theme);
        self.theme_watch.modified_at = Some(modified_at);
        self.mark_components_theme_changed()?;
        Ok(())
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

    fn reload_locale_if_settings_changed(&mut self) -> Result<(), ShellRunError> {
        let Ok(metadata) = std::fs::metadata(&self.settings_watch.path) else {
            return Ok(());
        };
        let Ok(modified_at) = metadata.modified() else {
            return Ok(());
        };

        if self.settings_watch.modified_at == Some(modified_at) {
            return Ok(());
        }

        self.settings_watch.modified_at = Some(modified_at);

        let new_settings = match load_shell_settings() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("failed to reload shell settings: {e}");
                return Ok(());
            }
        };

        let old_i18n = &self.settings.i18n;
        let new_i18n = &new_settings.i18n;
        if old_i18n.locale != new_i18n.locale
            || old_i18n.fallback_locale != new_i18n.fallback_locale
        {
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
            self.settings.i18n = new_i18n.clone();
            self.mark_components_locale_changed()?;
        }

        Ok(())
    }

    fn reload_plugin_settings_if_changed(&mut self) -> Result<(), ShellRunError> {
        for runtime in &mut self.components {
            let Some(settings_path) = runtime.plugin_settings_path.as_ref() else {
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
            .unwrap_or_else(|| default_surface_visibility(&surface_id));
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
            let ctx = ComponentContext {
                component_id: runtime.component.id().to_string(),
                surface_id: runtime.surface_id.clone(),
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
            CoreRequest::ToggleDebugOverlay => {
                self.debug.toggle();
                tracing::debug!("debug overlay: {}", if self.debug.enabled { "on" } else { "off" });
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

            // Paint debug overlay on top of the rendered buffer.
            if let Some(snapshot) = &debug_snapshot {
                if self.debug.show_layout_bounds {
                    if let Some(tree) = runtime.component.last_widget_tree() {
                        self.debug_overlay.paint_layout_bounds(tree, &mut buffer, 1.0);
                    }
                }
                self.debug_overlay.paint_panel(snapshot, self.debug.active_tab, &mut buffer, 1.0);
            }

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
            if let Some(surface) = self.surfaces.get(&runtime.surface_id) {
                let cfg = LayerSurfaceConfig {
                    edge: surface.edge,
                    layer: surface.layer.unwrap_or(Layer::Top),
                    width: surface.width,
                    height: surface.height,
                    exclusive_zone: surface.exclusive_zone,
                    keyboard_mode: surface.keyboard_mode,
                    namespace: runtime.surface_id.clone(),
                };
                self.windows.configure(&runtime.surface_id, cfg);
            }
            self.windows
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
        for event in self.windows.poll_events() {
            let surface_id = match &event {
                DevWindowEvent::PointerMove { surface_id, .. }
                | DevWindowEvent::PointerButton { surface_id, .. }
                | DevWindowEvent::Scroll { surface_id, .. }
                | DevWindowEvent::Key { surface_id, .. }
                | DevWindowEvent::Char { surface_id, .. } => surface_id,
            };

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
            if let DevWindowEvent::Key { event: DevWindowKeyEvent::Pressed(key, mods), .. } = &event {
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
                DevWindowEvent::PointerMove { x, y, .. } => ComponentInput::PointerMove { x, y },
                DevWindowEvent::PointerButton { x, y, pressed, .. } => {
                    ComponentInput::PointerButton { x, y, pressed }
                }
                DevWindowEvent::Scroll { x, y, dx, dy, .. } => {
                    ComponentInput::Scroll { x, y, dx, dy }
                }
                DevWindowEvent::Key {
                    event: DevWindowKeyEvent::Pressed(key, _mods),
                    ..
                } => ComponentInput::KeyPressed { key },
                DevWindowEvent::Key {
                    event: DevWindowKeyEvent::Released(key),
                    ..
                } => ComponentInput::KeyReleased { key },
                DevWindowEvent::Char { ch, .. } => ComponentInput::Char { ch },
            };

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

    fn spawn_backend_plugins(&self, runtime: &Runtime, tx: mpsc::UnboundedSender<ShellMessage>) {
        let mut plugin_ids: Vec<String> = self.plugins.keys().cloned().collect();
        plugin_ids.sort();

        for plugin_id in plugin_ids {
            let Some(plugin) = self.plugins.get(&plugin_id) else {
                continue;
            };

            if plugin.manifest.package.plugin_type != PluginType::Backend {
                continue;
            }

            let Some(service) = plugin.manifest.primary_service() else {
                continue;
            };

            runtime.spawn(spawn_mock_backend_service(
                tx.clone(),
                plugin.manifest.package.id.clone(),
                service.provides,
            ));
        }
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
        source: mesh_component_backend::CompileFrontendError,
    },

    #[error(transparent)]
    DependencyGraph(#[from] DependencyGraphError),

    #[error("{message}")]
    FrontendComposition { message: String },

    #[error("missing shell surface: {0}")]
    MissingSurface(String),

    #[error(transparent)]
    Render(#[from] mesh_renderer::RenderError),

    #[error("failed to initialize ipc socket at {path}: {source}")]
    IpcInit {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error(transparent)]
    Theme(#[from] mesh_theme::ThemeError),
}

fn default_plugin_dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    vec![
        workspace_root.join("plugins"),
        PathBuf::from("/usr/share/mesh/plugins"),
        PathBuf::from(&home).join(".local/share/mesh/plugins"),
        PathBuf::from(&home).join(".local/share/mesh/dev-plugins"),
    ]
}

fn default_surface_visibility(surface_id: &str) -> bool {
    default_surface_layout(surface_id).visible_on_start
}

fn default_surface_layout(surface_id: &str) -> SurfaceLayoutSettings {
    match surface_id {
        "@mesh/panel" => SurfaceLayoutSettings {
            edge: Edge::Top,
            layer: Layer::Top,
            width: 1920,
            height: 32,
            exclusive_zone: 32,
            keyboard_mode: KeyboardMode::None,
            visible_on_start: false,
            size_policy: SurfaceSizePolicy::Fixed,
        },
        "@mesh/launcher" => SurfaceLayoutSettings {
            edge: Edge::Top,
            layer: Layer::Overlay,
            width: 372,
            height: 344,
            exclusive_zone: 0,
            keyboard_mode: KeyboardMode::OnDemand,
            visible_on_start: false,
            size_policy: SurfaceSizePolicy::ContentMeasured,
        },
        "@mesh/notification-center" => SurfaceLayoutSettings {
            edge: Edge::Right,
            layer: Layer::Overlay,
            width: 420,
            height: 720,
            exclusive_zone: 0,
            keyboard_mode: KeyboardMode::OnDemand,
            visible_on_start: false,
            size_policy: SurfaceSizePolicy::Fixed,
        },
        "@mesh/quick-settings" => SurfaceLayoutSettings {
            edge: Edge::Top,
            layer: Layer::Overlay,
            width: 480,
            height: 420,
            exclusive_zone: 0,
            keyboard_mode: KeyboardMode::OnDemand,
            visible_on_start: false,
            size_policy: SurfaceSizePolicy::Fixed,
        },
        _ => SurfaceLayoutSettings {
            edge: Edge::Top,
            layer: Layer::Top,
            width: 480,
            height: 240,
            exclusive_zone: 0,
            keyboard_mode: KeyboardMode::None,
            visible_on_start: false,
            size_policy: SurfaceSizePolicy::Fixed,
        },
    }
}

fn load_frontend_plugin_settings(
    settings_path: &Path,
    surface_id: &str,
) -> FrontendPluginSettingsState {
    let raw = match std::fs::read_to_string(settings_path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!(
                    "failed to parse frontend settings at {}: {}",
                    settings_path.display(),
                    err
                );
                serde_json::Value::Object(serde_json::Map::new())
            }
        },
        Err(_) => serde_json::Value::Object(serde_json::Map::new()),
    };

    let mut layout = default_surface_layout(surface_id);
    let surface = raw.get("surface");

    if let Some(anchor) = surface
        .and_then(|value| value.get("anchor"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_surface_edge)
    {
        layout.edge = anchor;
    }

    if let Some(layer) = surface
        .and_then(|value| value.get("layer"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_surface_layer)
    {
        layout.layer = layer;
    }

    if let Some(width) = surface
        .and_then(|value| value.get("width"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
    {
        layout.width = width.max(1);
        layout.size_policy = SurfaceSizePolicy::Fixed;
    }

    if let Some(height) = surface
        .and_then(|value| value.get("height"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
    {
        layout.height = height.max(1);
        layout.size_policy = SurfaceSizePolicy::Fixed;
    }

    if let Some(zone) = surface
        .and_then(|value| value.get("exclusive_zone"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
    {
        layout.exclusive_zone = zone;
    }

    if let Some(mode) = surface
        .and_then(|value| value.get("keyboard_mode"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_keyboard_mode)
    {
        layout.keyboard_mode = mode;
    }

    if let Some(visible_on_start) = surface
        .and_then(|value| value.get("visible_on_start"))
        .and_then(serde_json::Value::as_bool)
    {
        layout.visible_on_start = visible_on_start;
    }

    FrontendPluginSettingsState { raw, layout }
}

fn parse_surface_edge(value: &str) -> Option<Edge> {
    match value.trim().to_ascii_lowercase().as_str() {
        "top" => Some(Edge::Top),
        "bottom" => Some(Edge::Bottom),
        "left" => Some(Edge::Left),
        "right" => Some(Edge::Right),
        _ => None,
    }
}

fn parse_surface_layer(value: &str) -> Option<Layer> {
    match value.trim().to_ascii_lowercase().as_str() {
        "background" => Some(Layer::Background),
        "bottom" => Some(Layer::Bottom),
        "top" => Some(Layer::Top),
        "overlay" => Some(Layer::Overlay),
        _ => None,
    }
}

fn parse_keyboard_mode(value: &str) -> Option<KeyboardMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => Some(KeyboardMode::None),
        "exclusive" => Some(KeyboardMode::Exclusive),
        "on_demand" | "ondemand" | "on-demand" => Some(KeyboardMode::OnDemand),
        _ => None,
    }
}

fn load_active_theme(settings: &ShellSettings) -> (ThemeEngine, ThemeWatchState) {
    let theme_path = theme_path_for_id(&settings.theme.active);
    let theme = match load_theme_from_path(&theme_path) {
        Ok(theme) => theme,
        Err(err) => {
            tracing::warn!(
                "failed to load requested theme '{}' from {}: {err}; using default theme",
                settings.theme.active,
                theme_path.display()
            );
            default_theme()
        }
    };
    let modified_at = std::fs::metadata(&theme_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok());

    (
        ThemeEngine::new(theme),
        ThemeWatchState {
            path: theme_path,
            modified_at,
        },
    )
}

struct FrontendSurfaceComponent {
    compiled: CompiledFrontendPlugin,
    plugin_dir: PathBuf,
    plugin_settings_file: PathBuf,
    settings_json: serde_json::Value,
    surface_layout: SurfaceLayoutSettings,
    frontend_catalog: FrontendCatalog,
    visible: bool,
    dirty: bool,
    last_service_update: Option<String>,
    focused_key: Option<String>,
    pointer_down_key: Option<String>,
    active_slider_key: Option<String>,
    input_values: HashMap<String, String>,
    slider_values: HashMap<String, f32>,
    scroll_offsets: HashMap<String, ScrollOffsetState>,
    // Hover tracking for tooltip system
    hovered_key: Option<String>,
    hovered_pos: (f32, f32),
    hover_start: Option<std::time::Instant>,
    runtimes: RefCell<HashMap<String, EmbeddedFrontendRuntime>>,
    render_stack: RefCell<Vec<String>>,
    active_theme: RefCell<Theme>,
    measured_size: Option<(u32, u32)>,
    locale: LocaleEngine,
    interface_catalog: mesh_service::InterfaceCatalog,
    last_tree: Option<WidgetNode>,
}

#[derive(Debug, Clone, Copy, Default)]
struct ScrollOffsetState {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone)]
struct FrontendCatalog {
    plugins: HashMap<String, FrontendCatalogEntry>,
    slot_contributions: HashMap<String, Vec<ResolvedSlotContribution>>,
}

#[derive(Debug, Clone)]
struct FrontendCatalogEntry {
    plugin_dir: PathBuf,
    compiled: CompiledFrontendPlugin,
}

#[derive(Debug, Clone)]
struct ResolvedSlotContribution {
    source_plugin_id: String,
    widget_id: String,
    contribution_id: String,
    order: i64,
    props: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug)]
struct EmbeddedFrontendRuntime {
    plugin_id: String,
    script_ctx: ScriptContext,
}

impl FrontendCatalog {
    fn from_plugins(plugins: &HashMap<String, PluginInstance>) -> Result<Self, ShellRunError> {
        let mut plugin_ids: Vec<String> = plugins.keys().cloned().collect();
        plugin_ids.sort();

        let mut catalog = Self {
            plugins: HashMap::new(),
            slot_contributions: HashMap::new(),
        };

        for plugin_id in plugin_ids {
            let Some(plugin) = plugins.get(&plugin_id) else {
                continue;
            };

            if !is_frontend_plugin(&plugin.manifest) {
                continue;
            }

            let compiled =
                compile_frontend_plugin(&plugin.manifest, &plugin.path).map_err(|source| {
                    ShellRunError::FrontendCompile {
                        plugin_id: plugin_id.clone(),
                        source,
                    }
                })?;

            catalog.plugins.insert(
                plugin_id.clone(),
                FrontendCatalogEntry {
                    plugin_dir: plugin.path.clone(),
                    compiled,
                },
            );
        }

        for (plugin_id, entry) in &catalog.plugins {
            for (slot_id, contributions) in &entry.compiled.manifest.slot_contributions {
                let bucket = catalog
                    .slot_contributions
                    .entry(slot_id.clone())
                    .or_default();
                for (index, contribution) in contributions.iter().enumerate() {
                    bucket.push(ResolvedSlotContribution {
                        source_plugin_id: plugin_id.clone(),
                        widget_id: contribution
                            .widget
                            .clone()
                            .unwrap_or_else(|| plugin_id.clone()),
                        contribution_id: contribution
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("{plugin_id}:{slot_id}:{index}")),
                        order: contribution.order.unwrap_or(0),
                        props: contribution.props.clone(),
                    });
                }
            }
        }

        for contributions in catalog.slot_contributions.values_mut() {
            contributions.sort_by(|left, right| {
                left.order
                    .cmp(&right.order)
                    .then_with(|| left.widget_id.cmp(&right.widget_id))
                    .then_with(|| left.contribution_id.cmp(&right.contribution_id))
            });
        }

        for (plugin_id, entry) in &catalog.plugins {
            for component_tag in entry.compiled.referenced_component_tags() {
                catalog
                    .resolve_component_plugin_id(&entry.compiled.manifest, &component_tag)
                    .map_err(|message| ShellRunError::FrontendComposition {
                        message: format!(
                            "plugin '{plugin_id}' cannot resolve <{component_tag}>: {message}"
                        ),
                    })?;
            }
        }

        Ok(catalog)
    }

    fn slot_contributions_for(&self, slot_id: &str) -> &[ResolvedSlotContribution] {
        self.slot_contributions
            .get(slot_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn top_level_surfaces(&self) -> Vec<FrontendCatalogEntry> {
        let mut entries: Vec<FrontendCatalogEntry> = self
            .plugins
            .values()
            .filter(|entry| entry.compiled.manifest.package.plugin_type == PluginType::Surface)
            .cloned()
            .collect();
        entries.sort_by(|left, right| {
            left.compiled
                .manifest
                .package
                .id
                .cmp(&right.compiled.manifest.package.id)
        });
        entries
    }

    fn resolve_component_plugin_id(
        &self,
        host: &mesh_plugin::Manifest,
        tag: &str,
    ) -> Result<String, String> {
        let mut matches = Vec::new();

        for dependency_id in host.required_plugin_dependencies() {
            let Some(entry) = self.plugins.get(&dependency_id) else {
                continue;
            };

            if entry.compiled.manifest.package.plugin_type != PluginType::Widget {
                continue;
            }

            if entry.compiled.manifest.exported_component_tag() == Some(tag) {
                matches.push(dependency_id);
            }
        }

        match matches.len() {
            1 => Ok(matches.remove(0)),
            0 => Err(format!(
                "no required widget dependency exports that tag; add a plugin dependency whose plugin.json exports.component.tag is '{tag}'"
            )),
            _ => Err(format!(
                "multiple required widget dependencies export '{tag}': {matches:?}"
            )),
        }
    }
}

impl FrontendSurfaceComponent {
    fn new(
        compiled: CompiledFrontendPlugin,
        plugin_dir: PathBuf,
        frontend_catalog: FrontendCatalog,
        interface_catalog: mesh_service::InterfaceCatalog,
    ) -> Self {
        let surface_id = compiled.surface_id().to_string();
        let plugin_settings_file = plugin_dir.join("config/settings.json");
        let settings_state = load_frontend_plugin_settings(&plugin_settings_file, &surface_id);
        Self {
            compiled,
            plugin_dir,
            plugin_settings_file,
            settings_json: settings_state.raw,
            surface_layout: settings_state.layout.clone(),
            frontend_catalog,
            visible: settings_state.layout.visible_on_start,
            dirty: true,
            last_service_update: None,
            focused_key: None,
            pointer_down_key: None,
            active_slider_key: None,
            input_values: HashMap::new(),
            slider_values: HashMap::new(),
            scroll_offsets: HashMap::new(),
            hovered_key: None,
            hovered_pos: (0.0, 0.0),
            hover_start: None,
            runtimes: RefCell::new(HashMap::new()),
            render_stack: RefCell::new(Vec::new()),
            active_theme: RefCell::new(default_theme()),
            measured_size: None,
            locale: LocaleEngine::new("en"),
            interface_catalog,
            last_tree: None,
        }
    }

    fn render_layout(&self, surface: &mut dyn ShellSurface) {
        surface.anchor(self.surface_layout.edge);
        surface.set_layer(self.surface_layout.layer);
        let (width, height) = match self.surface_layout.size_policy {
            SurfaceSizePolicy::Fixed => (self.surface_layout.width, self.surface_layout.height),
            SurfaceSizePolicy::ContentMeasured => self
                .measured_size
                .unwrap_or((self.surface_layout.width, self.surface_layout.height)),
        };
        surface.set_size(width, height);
        surface.set_exclusive_zone(self.surface_layout.exclusive_zone);
        surface.set_keyboard_interactivity(self.surface_layout.keyboard_mode);
    }

    fn build_tree(&mut self, theme: &Theme, width: u32, height: u32) -> WidgetNode {
        self.active_theme.replace(theme.clone());
        let root_state = self.runtime_state(self.id()).unwrap_or_default();
        let bound = LocaleBoundState::new(&root_state, &self.locale);
        {
            let mut stack = self.render_stack.borrow_mut();
            stack.clear();
            stack.push(self.id().to_string());
        }
        let mut tree = self.compiled.build_tree_with_state(
            theme,
            width,
            height,
            Some(&bound),
            FrontendRenderMode::Surface,
            self.id(),
            Some(self),
        );
        self.render_stack.borrow_mut().clear();
        annotate_runtime_tree(
            &mut tree,
            "root".to_string(),
            &self.focused_key,
            &self.input_values,
            &self.slider_values,
            &self.scroll_offsets,
        );
        annotate_overflow_tree(&mut tree, "root", &mut self.scroll_offsets);
        tree
    }

    fn update_slider_from_position(&mut self, tree: &WidgetNode, slider_key: &str, x: f32) {
        let Some(node) = find_node_by_key(tree, slider_key) else {
            return;
        };
        let Some((left, _, right, _)) = find_node_bounds_by_key(tree, slider_key, 0.0, 0.0) else {
            return;
        };

        let min = node
            .attributes
            .get("min")
            .and_then(|value: &String| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let max = node
            .attributes
            .get("max")
            .and_then(|value: &String| value.parse::<f32>().ok())
            .unwrap_or(100.0);

        if max <= min {
            return;
        }

        let width = (right - left).max(1.0);
        let local_x = (x - left).clamp(0.0, width);
        let pct = (local_x / width).clamp(0.0, 1.0);
        let value = min + (max - min) * pct;
        self.slider_values.insert(slider_key.to_string(), value);
    }

    fn runtime_state(&self, instance_key: &str) -> Option<mesh_scripting::ScriptState> {
        self.runtimes
            .borrow()
            .get(instance_key)
            .map(|runtime| runtime.script_ctx.state().clone())
    }

    /// Load translation files from `config/i18n/{locale}.json` inside the plugin directory.
    fn load_plugin_i18n_from_dir(&mut self, plugin_dir: &Path) {
        let i18n_dir = plugin_dir.join("config/i18n");
        let entries = match std::fs::read_dir(&i18n_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let messages: HashMap<String, String> = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(_) => {
                    tracing::warn!(
                        "plugin '{}': failed to parse i18n file {}",
                        self.id(),
                        path.display()
                    );
                    continue;
                }
            };
            tracing::debug!(
                "plugin '{}': loaded {} translations for locale '{}'",
                self.id(),
                messages.len(),
                stem
            );
            self.locale.load_translations(mesh_locale::TranslationSet {
                locale: stem.to_string(),
                messages,
            });
        }
    }

    fn load_plugin_i18n(&mut self) {
        let plugin_dir = self.plugin_dir.clone();
        self.load_plugin_i18n_from_dir(&plugin_dir);
    }

    fn load_catalog_i18n(&mut self) {
        let plugin_dirs: Vec<PathBuf> = self
            .frontend_catalog
            .plugins
            .values()
            .map(|entry| entry.plugin_dir.clone())
            .collect();
        for plugin_dir in plugin_dirs {
            self.load_plugin_i18n_from_dir(&plugin_dir);
        }
    }

    fn create_runtime(
        &self,
        compiled: &CompiledFrontendPlugin,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<EmbeddedFrontendRuntime, ComponentError> {
        let component_id = compiled.manifest.package.id.clone();
        let mut script_ctx = ScriptContext::new(
            component_id.clone(),
            grant_capabilities_from_manifest(&compiled.manifest),
        )
        .map_err(|source| ComponentError::Script {
            component_id: component_id.clone(),
            source,
        })?;
        script_ctx.set_interface_catalog(self.interface_catalog.clone());
        let has_audio_read = manifest_grants_capability(&compiled.manifest, "service.audio.read");
        seed_service_state(script_ctx.state_mut(), has_audio_read);

        for (key, value) in props {
            script_ctx.state_mut().set(key.clone(), value.clone());
        }

        if let Some(script) = &compiled.component.script {
            script_ctx
                .load_script(&script.source)
                .map_err(|source| ComponentError::Script {
                    component_id: component_id.clone(),
                    source,
                })?;
            script_ctx
                .call_init()
                .map_err(|source| ComponentError::Script {
                    component_id: component_id.clone(),
                    source,
                })?;
        }

        Ok(EmbeddedFrontendRuntime {
            plugin_id: component_id,
            script_ctx,
        })
    }

    fn init_root_runtime(&self) -> Result<(), ComponentError> {
        let mut props = HashMap::new();
        props.insert("settings".into(), self.settings_json.clone());
        let runtime = self.create_runtime(&self.compiled, &props)?;
        self.runtimes
            .borrow_mut()
            .insert(self.id().to_string(), runtime);
        Ok(())
    }

    fn ensure_runtime(
        &self,
        instance_key: &str,
        plugin_id: &str,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ComponentError> {
        if !self.runtimes.borrow().contains_key(instance_key) {
            let Some(entry) = self.frontend_catalog.plugins.get(plugin_id) else {
                return Err(ComponentError::Failed {
                    component_id: self.id().to_string(),
                    message: format!("missing embedded frontend plugin '{plugin_id}'"),
                });
            };
            let runtime = self.create_runtime(&entry.compiled, props)?;
            self.runtimes
                .borrow_mut()
                .insert(instance_key.to_string(), runtime);
        }

        if let Some(runtime) = self.runtimes.borrow_mut().get_mut(instance_key) {
            for (key, value) in props {
                runtime
                    .script_ctx
                    .state_mut()
                    .set(key.clone(), value.clone());
            }
        }

        Ok(())
    }

    fn build_error_widget(&self, message: impl Into<String>) -> WidgetNode {
        let message = message.into();
        let mut node = WidgetNode::new("box");
        let mut text = WidgetNode::new("text");
        text.attributes.insert("content".into(), message.clone());
        node.attributes.insert("content".into(), message);
        node.children.push(text);
        node
    }

    fn render_embedded_instance(
        &self,
        instance_key: &str,
        plugin_id: &str,
        props: &HashMap<String, serde_json::Value>,
        container_width: f32,
        container_height: f32,
    ) -> WidgetNode {
        if self
            .render_stack
            .borrow()
            .iter()
            .filter(|ancestor| ancestor.as_str() == plugin_id)
            .count()
            >= 2
        {
            return self.build_error_widget(format!("composition cycle blocked for '{plugin_id}'"));
        }

        if let Err(err) = self.ensure_runtime(instance_key, plugin_id, props) {
            return self.build_error_widget(err.to_string());
        }

        let Some(entry) = self.frontend_catalog.plugins.get(plugin_id) else {
            return self.build_error_widget(format!("missing embedded plugin '{plugin_id}'"));
        };

        let state = self.runtime_state(instance_key).unwrap_or_default();
        let bound = LocaleBoundState::new(&state, &self.locale);
        let active_theme = self.active_theme.borrow().clone();
        self.render_stack.borrow_mut().push(plugin_id.to_string());
        let mut tree = entry.compiled.build_tree_with_state(
            &active_theme,
            container_width.max(0.0).ceil() as u32,
            container_height.max(0.0).ceil() as u32,
            Some(&bound),
            FrontendRenderMode::Embedded,
            instance_key,
            Some(self),
        );
        self.render_stack.borrow_mut().pop();
        namespace_event_handlers(&mut tree, instance_key);
        tree
    }

    fn call_namespaced_handler(&mut self, handler: &str) -> Result<Vec<CoreRequest>, ComponentError> {
        let (instance_key, handler_name, component_id) =
            if let Some((instance_key, handler_name)) = parse_namespaced_handler(handler) {
                let component_id = self
                    .runtimes
                    .borrow()
                    .get(instance_key)
                    .map(|runtime| runtime.plugin_id.clone())
                    .unwrap_or_else(|| self.id().to_string());
                (
                    instance_key.to_string(),
                    handler_name.to_string(),
                    component_id,
                )
            } else {
                (
                    self.id().to_string(),
                    handler.to_string(),
                    self.id().to_string(),
                )
            };

        let mut runtimes = self.runtimes.borrow_mut();
        let Some(runtime) = runtimes.get_mut(&instance_key) else {
            return Ok(Vec::new());
        };
        runtime
            .script_ctx
            .call_handler(&handler_name, &[])
            .map_err(|source| ComponentError::Script {
                component_id,
                source,
            })?;
        self.dirty = true;

        Ok(script_events_to_requests(
            runtime.script_ctx.drain_published_events(),
        ))
    }
}

impl FrontendCompositionResolver for FrontendSurfaceComponent {
    fn render_import(
        &self,
        host: &mesh_plugin::Manifest,
        host_instance_key: &str,
        alias: &str,
        props: &HashMap<String, String>,
        container_width: f32,
        container_height: f32,
    ) -> Option<WidgetNode> {
        let plugin_id = match self
            .frontend_catalog
            .resolve_component_plugin_id(host, alias)
        {
            Ok(plugin_id) => plugin_id,
            Err(message) => return Some(self.build_error_widget(message)),
        };
        let props_json: HashMap<String, serde_json::Value> = props
            .iter()
            .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
            .collect();
        let instance_key = format!("{host_instance_key}/import:{alias}");
        Some(self.render_embedded_instance(
            &instance_key,
            &plugin_id,
            &props_json,
            container_width,
            container_height,
        ))
    }

    fn render_slot(
        &self,
        host: &mesh_plugin::Manifest,
        host_instance_key: &str,
        slot_name: Option<&str>,
        container_width: f32,
        container_height: f32,
    ) -> Vec<WidgetNode> {
        let Some(slot_name) = slot_name else {
            return Vec::new();
        };

        let slot_id = format!("{}:{slot_name}", host.package.id);
        let accepts_widget = host
            .provides_slots
            .get(slot_name)
            .and_then(|definition| definition.accepts.as_deref())
            .map(|accepts| accepts == "widget")
            .unwrap_or(false);

        let mut nodes = Vec::new();
        for contribution in self.frontend_catalog.slot_contributions_for(&slot_id) {
            let Some(entry) = self.frontend_catalog.plugins.get(&contribution.widget_id) else {
                nodes.push(self.build_error_widget(format!(
                    "slot '{slot_id}' references missing plugin '{}'",
                    contribution.widget_id
                )));
                continue;
            };

            if accepts_widget && entry.compiled.manifest.package.plugin_type != PluginType::Widget {
                nodes.push(self.build_error_widget(format!(
                    "slot '{slot_id}' accepts widgets, but '{}' is {}",
                    contribution.widget_id, entry.compiled.manifest.package.plugin_type
                )));
                continue;
            }

            let props_json: HashMap<String, serde_json::Value> = contribution
                .props
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            let instance_key = format!(
                "{host_instance_key}/slot:{slot_name}/{}",
                contribution.contribution_id
            );
            let mut node = self.render_embedded_instance(
                &instance_key,
                &contribution.widget_id,
                &props_json,
                container_width,
                container_height,
            );
            node.attributes.insert(
                "_mesh_slot_source".into(),
                contribution.source_plugin_id.clone(),
            );
            nodes.push(node);
        }

        nodes
    }
}

impl ShellComponent for FrontendSurfaceComponent {
    fn id(&self) -> &str {
        &self.compiled.manifest.package.id
    }

    fn surface_id(&self) -> &str {
        self.compiled.surface_id()
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(self.surface_layout.visible_on_start)
    }

    fn mount(&mut self, _ctx: ComponentContext) -> Result<Vec<CoreRequest>, ComponentError> {
        self.load_plugin_i18n();
        self.load_catalog_i18n();
        self.init_root_runtime()?;
        self.dirty = true;
        Ok(vec![CoreRequest::PublishDiagnostics {
            message: format!(
                "mounted frontend component '{}' from {}",
                self.id(),
                self.compiled.source_path.display()
            ),
        }])
    }

    fn handle_core_event(&mut self, event: &CoreEvent) -> Result<Vec<CoreRequest>, ComponentError> {
        if let CoreEvent::SurfaceVisibilityChanged {
            surface_id,
            visible,
        } = event
        {
            if surface_id == self.surface_id() {
                self.visible = *visible;
                self.dirty = true;
            }
        }
        Ok(Vec::new())
    }

    fn handle_service_event(
        &mut self,
        event: &ServiceEvent,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let ServiceEvent::Updated {
            service,
            source_plugin,
            summary,
        } = event;
        self.last_service_update = Some(format!("{service}:{source_plugin}:{summary}"));
        for runtime in self.runtimes.borrow_mut().values_mut() {
            let has_audio_read = runtime
                .script_ctx
                .capabilities
                .is_granted(&Capability::new("service.audio.read"));
            apply_service_update(
                runtime.script_ctx.state_mut(),
                has_audio_read,
                service,
                source_plugin,
                summary,
            );
        }
        self.dirty = true;
        Ok(Vec::new())
    }

    fn tick(&mut self) -> Result<Vec<CoreRequest>, ComponentError> {
        // Trigger a repaint once the tooltip delay has elapsed so the tooltip appears.
        if let Some(start) = self.hover_start {
            if start.elapsed() >= Duration::from_millis(500) && !self.dirty {
                self.dirty = true;
            }
        }
        Ok(Vec::new())
    }

    fn wants_render(&self) -> bool {
        self.dirty
    }

    fn render(&mut self, surface: &mut dyn ShellSurface) -> Result<(), ComponentError> {
        self.render_layout(surface);

        if self.visible {
            surface.show();
        } else {
            surface.hide();
        }

        let template_nodes = self
            .compiled
            .component
            .template
            .as_ref()
            .map(|template| template.root.len())
            .unwrap_or(0);
        let role = root_accessibility_role(&self.compiled.manifest, &self.compiled.component)
            .unwrap_or_else(|| "unknown".into());

        tracing::debug!(
            "rendered frontend '{}' visible={} nodes={} role={}{}",
            self.id(),
            self.visible,
            template_nodes,
            role,
            self.last_service_update
                .as_deref()
                .map(|summary| format!(" service={summary}"))
                .unwrap_or_default()
        );

        self.dirty = false;
        Ok(())
    }

    fn paint(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        buffer: &mut PixelBuffer,
    ) -> Result<(), ComponentError> {
        let tree = self.build_tree(theme, width, height);
        let measured_size = measure_content_size(&tree, width, height, self.surface_id());
        if self.measured_size != Some(measured_size) {
            self.measured_size = Some(measured_size);
            self.dirty = true;
        }
        buffer.clear(tree.computed_style.background_color);
        let painter = Painter::new();
        painter.paint(&tree, buffer, 1.0);
        self.last_tree = Some(tree.clone());

        // Paint tooltip overlay when the hover delay has elapsed.
        if let (Some(start), Some(hovered_key)) = (self.hover_start, self.hovered_key.as_ref()) {
            if start.elapsed() >= Duration::from_millis(500) {
                if let Some(tooltip_text) = find_tooltip_text_by_key(&tree, hovered_key) {
                    let (cx, cy) = self.hovered_pos;
                    painter.paint_tooltip(&tooltip_text, cx, cy, buffer, 1.0);
                }
            }
        }

        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), ComponentError> {
        self.dirty = true;
        Ok(())
    }

    fn locale_changed(&mut self, locale: &LocaleEngine) -> Result<(), ComponentError> {
        self.locale.set_locale(locale.current());
        self.runtimes.borrow_mut().clear();
        self.init_root_runtime()?;
        self.dirty = true;
        Ok(())
    }

    fn source_path(&self) -> Option<&Path> {
        Some(self.compiled.source_path.as_path())
    }

    fn plugin_settings_path(&self) -> Option<&Path> {
        if self.plugin_settings_file.exists() {
            Some(self.plugin_settings_file.as_path())
        } else {
            None
        }
    }

    fn reload_plugin_settings(&mut self) -> Result<bool, ComponentError> {
        let settings_state =
            load_frontend_plugin_settings(&self.plugin_settings_file, self.surface_id());
        let layout_changed = self.surface_layout != settings_state.layout;
        let settings_changed = self.settings_json != settings_state.raw;

        self.surface_layout = settings_state.layout;
        self.settings_json = settings_state.raw;

        if settings_changed {
            if let Some(runtime) = self.runtimes.borrow_mut().get_mut(self.id()) {
                runtime
                    .script_ctx
                    .state_mut()
                    .set("settings", self.settings_json.clone());
            }
        }

        let Some(locale) = self
            .settings_json
            .get("i18n")
            .and_then(|i18n| i18n.get("default_locale"))
            .and_then(|l| l.as_str())
        else {
            if layout_changed || settings_changed {
                self.dirty = true;
            }
            return Ok(layout_changed || settings_changed);
        };

        if self.locale.current() != locale {
            tracing::info!(
                "plugin '{}': applying locale '{}' from plugin settings",
                self.id(),
                locale
            );
            self.locale.set_locale(locale);
        }

        if layout_changed || settings_changed {
            self.dirty = true;
        }
        Ok(layout_changed || settings_changed)
    }

    fn reload_source(&mut self) -> Result<bool, ComponentError> {
        let manifest = self.compiled.manifest.clone();
        let recompiled = compile_frontend_plugin(&manifest, &self.plugin_dir).map_err(|err| {
            ComponentError::Failed {
                component_id: self.id().to_string(),
                message: format!("frontend recompile failed: {err}"),
            }
        })?;

        let component_id = self.id().to_string();
        self.compiled = recompiled;
        if let Some(entry) = self.frontend_catalog.plugins.get_mut(&component_id) {
            entry.compiled = self.compiled.clone();
        }
        self.runtimes.borrow_mut().clear();
        self.init_root_runtime()?;
        self.dirty = true;
        Ok(true)
    }

    fn handle_input(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        if !self.visible {
            return Ok(Vec::new());
        }

        let tree = self.build_tree(theme, width, height);

        match input {
            ComponentInput::PointerButton { x, y, pressed } => {
                if pressed {
                    if let Some(node_key) = find_focusable_at(&tree, x, y) {
                        self.focused_key = Some(node_key.clone());
                        self.pointer_down_key = Some(node_key.clone());

                        if is_slider_key(&tree, &node_key) {
                            self.active_slider_key = Some(node_key.clone());
                            self.update_slider_from_position(&tree, &node_key, x);
                        } else {
                            self.active_slider_key = None;
                        }

                        self.dirty = true;
                    } else {
                        self.focused_key = None;
                        self.pointer_down_key = None;
                        self.active_slider_key = None;
                        self.dirty = true;
                    }
                } else {
                    if let Some(node_key) = find_focusable_at(&tree, x, y) {
                        if self.pointer_down_key.as_deref() == Some(node_key.as_str()) {
                            if let Some(handler) = find_click_handler(&tree, &node_key) {
                                return self.call_namespaced_handler(&handler);
                            }
                        }
                    }
                    self.pointer_down_key = None;
                    self.active_slider_key = None;
                }
            }
            ComponentInput::PointerMove { x, y } => {
                if let Some(slider_key) = self.active_slider_key.clone() {
                    self.update_slider_from_position(&tree, &slider_key, x);
                    self.dirty = true;
                }

                // Update hover state for tooltip system.
                self.hovered_pos = (x, y);
                let new_key = find_tooltip_at(&tree, x, y).map(|(k, _)| k);
                if new_key != self.hovered_key {
                    self.hovered_key = new_key.clone();
                    self.hover_start = new_key.map(|_| std::time::Instant::now());
                    self.dirty = true;
                }
            }
            ComponentInput::Scroll { x, y, dx, dy } => {
                if let Some(scroll_key) = find_scrollable_at(&tree, x, y) {
                    if let Some(node) = find_node_by_key(&tree, &scroll_key) {
                        let (max_x, max_y) = scroll_limits(node);
                        let current = self.scroll_offsets.entry(scroll_key).or_default();
                        let next_x = (current.x - dx * 28.0).clamp(0.0, max_x);
                        let next_y = (current.y - dy * 28.0).clamp(0.0, max_y);
                        if (next_x - current.x).abs() > f32::EPSILON
                            || (next_y - current.y).abs() > f32::EPSILON
                        {
                            current.x = next_x;
                            current.y = next_y;
                            self.dirty = true;
                        }
                    }
                }
            }
            ComponentInput::Char { ch } => {
                if let Some(focused_key) = self.focused_key.clone() {
                    if is_input_key(&tree, &focused_key) && !ch.is_control() {
                        self.input_values.entry(focused_key).or_default().push(ch);
                        self.dirty = true;
                    }
                }
            }
            ComponentInput::KeyPressed { key } => {
                if let Some(focused_key) = self.focused_key.clone() {
                    if is_input_key(&tree, &focused_key) {
                        let value = self.input_values.entry(focused_key).or_default();
                        match key.as_str() {
                            "Backspace" => {
                                value.pop();
                                self.dirty = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            ComponentInput::KeyReleased { .. } => {}
        }

        Ok(Vec::new())
    }

    fn last_widget_tree(&self) -> Option<&WidgetNode> {
        self.last_tree.as_ref()
    }
}

fn annotate_runtime_tree(
    node: &mut WidgetNode,
    key: String,
    focused_key: &Option<String>,
    input_values: &HashMap<String, String>,
    slider_values: &HashMap<String, f32>,
    scroll_offsets: &HashMap<String, ScrollOffsetState>,
) {
    node.attributes.insert("_mesh_key".into(), key.clone());

    if focused_key.as_deref() == Some(key.as_str()) {
        node.attributes
            .insert("_mesh_focused".into(), "true".into());
    }

    match node.tag.as_str() {
        "input" => {
            let value = input_values
                .get(&key)
                .cloned()
                .or_else(|| node.attributes.get("value").cloned())
                .unwrap_or_default();
            node.attributes.insert("value".into(), value);
        }
        "slider" => {
            let value = slider_values
                .get(&key)
                .copied()
                .or_else(|| {
                    node.attributes
                        .get("value")
                        .and_then(|value: &String| value.parse::<f32>().ok())
                })
                .unwrap_or(50.0);
            node.attributes
                .insert("value".into(), format!("{value:.2}"));
        }
        _ => {}
    }

    let offset = scroll_offsets.get(&key).copied().unwrap_or_default();
    node.attributes
        .insert("_mesh_scroll_x".into(), format!("{:.2}", offset.x));
    node.attributes
        .insert("_mesh_scroll_y".into(), format!("{:.2}", offset.y));

    for (index, child) in node.children.iter_mut().enumerate() {
        annotate_runtime_tree(
            child,
            format!("{key}/{index}"),
            focused_key,
            input_values,
            slider_values,
            scroll_offsets,
        );
    }
}

fn grant_capabilities_from_manifest(manifest: &mesh_plugin::Manifest) -> CapabilitySet {
    let mut granted = CapabilitySet::new();

    for capability in &manifest.capabilities.required {
        granted.grant(Capability::new(capability.clone()));
    }

    for capability in &manifest.capabilities.optional {
        granted.grant(Capability::new(capability.clone()));
    }

    granted
}

fn manifest_grants_capability(manifest: &mesh_plugin::Manifest, capability: &str) -> bool {
    manifest.capabilities.required.iter().any(|value| value == capability)
        || manifest
            .capabilities
            .optional
            .iter()
            .any(|value| value == capability)
}

fn seed_service_state(state: &mut ScriptState, has_audio_read: bool) {
    state.set(
        "last_service_update",
        serde_json::json!({
            "name": "",
            "source_plugin": "",
            "summary": "",
        }),
    );
    if has_audio_read {
        state.set("audio", build_audio_service_state(None, None));
    }
}

fn apply_service_update(
    state: &mut ScriptState,
    has_audio_read: bool,
    service: &str,
    source_plugin: &str,
    summary: &str,
) {
    state.set(
        "last_service_update",
        serde_json::json!({
            "name": service,
            "source_plugin": source_plugin,
            "summary": summary,
        }),
    );

    if service == "audio" && has_audio_read {
        state.set("audio", build_audio_service_state(Some(summary), Some(source_plugin)));
    }
}

fn build_audio_service_state(
    summary: Option<&str>,
    source_plugin: Option<&str>,
) -> serde_json::Value {
    let Some(summary) = summary else {
        return serde_json::json!({
            "available": false,
            "percent": 0,
            "label": "Unavailable",
            "glyph": "VOL",
            "tooltip": "Audio backend unavailable",
            "summary": "",
            "source_plugin": source_plugin.unwrap_or(""),
        });
    };

    let percent = summary
        .strip_prefix("volume=")
        .and_then(|value| value.strip_suffix('%'))
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);

    let label = if percent == 0 {
        "Muted".to_string()
    } else {
        format!("{percent}%")
    };
    let glyph = if percent == 0 {
        "MUTE"
    } else if percent >= 70 {
        "LOUD"
    } else {
        "VOL"
    };
    let tooltip = match source_plugin {
        Some(source_plugin) if !source_plugin.is_empty() => {
            format!("{source_plugin}: {label}")
        }
        _ => format!("Audio: {label}"),
    };

    serde_json::json!({
        "available": true,
        "percent": percent,
        "label": label,
        "glyph": glyph,
        "tooltip": tooltip,
        "summary": summary,
        "source_plugin": source_plugin.unwrap_or(""),
    })
}

fn script_events_to_requests(events: Vec<PublishedEvent>) -> Vec<CoreRequest> {
    let mut requests = Vec::new();

    for event in events {
        match event.channel.as_str() {
            "shell.toggle-quick-settings" => requests.push(CoreRequest::ToggleSurface {
                surface_id: "@mesh/quick-settings".into(),
            }),
            "shell.open-quick-settings" => requests.push(CoreRequest::ShowSurface {
                surface_id: "@mesh/quick-settings".into(),
            }),
            "shell.close-quick-settings" => requests.push(CoreRequest::HideSurface {
                surface_id: "@mesh/quick-settings".into(),
            }),
            "shell.toggle-launcher" => requests.push(CoreRequest::ToggleSurface {
                surface_id: "@mesh/launcher".into(),
            }),
            "shell.open-launcher" => requests.push(CoreRequest::ShowSurface {
                surface_id: "@mesh/launcher".into(),
            }),
            "shell.close-launcher" => requests.push(CoreRequest::HideSurface {
                surface_id: "@mesh/launcher".into(),
            }),
            "shell.toggle-notification-center" => requests.push(CoreRequest::ToggleSurface {
                surface_id: "@mesh/notification-center".into(),
            }),
            "shell.open-notification-center" => requests.push(CoreRequest::ShowSurface {
                surface_id: "@mesh/notification-center".into(),
            }),
            "shell.close-notification-center" => requests.push(CoreRequest::HideSurface {
                surface_id: "@mesh/notification-center".into(),
            }),
            _ => {}
        }
    }

    requests
}

fn find_node_by_key<'a>(node: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
    if node
        .attributes
        .get("_mesh_key")
        .is_some_and(|value| value == key)
    {
        return Some(node);
    }

    for child in &node.children {
        if let Some(found) = find_node_by_key(child, key) {
            return Some(found);
        }
    }

    None
}

fn find_node_bounds_by_key(
    node: &WidgetNode,
    key: &str,
    offset_x: f32,
    offset_y: f32,
) -> Option<ContentBounds> {
    if node
        .attributes
        .get("_mesh_key")
        .is_some_and(|value| value == key)
    {
        return Some(node_rect_with_offset(node, offset_x, offset_y));
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in &node.children {
        if let Some(bounds) = find_node_bounds_by_key(child, key, child_offset_x, child_offset_y) {
            return Some(bounds);
        }
    }

    None
}

fn find_focusable_at(node: &WidgetNode, x: f32, y: f32) -> Option<String> {
    find_focusable_at_with_offset(node, x, y, 0.0, 0.0)
}

fn find_scrollable_at(node: &WidgetNode, x: f32, y: f32) -> Option<String> {
    find_scrollable_at_with_offset(node, x, y, 0.0, 0.0)
}

/// Return (key, tooltip_text) for the deepest node under the cursor that has tooltip content.
fn find_tooltip_at(node: &WidgetNode, x: f32, y: f32) -> Option<(String, String)> {
    find_tooltip_at_offset(node, x, y, 0.0, 0.0)
}

fn find_tooltip_at_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<(String, String)> {
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside && node_clips_children(node) {
        return None;
    }

    let (child_ox, child_oy) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in node.children.iter().rev() {
        if let Some(result) = find_tooltip_at_offset(child, x, y, child_ox, child_oy) {
            return Some(result);
        }
    }

    if inside {
        let key = node.attributes.get("_mesh_key")?.clone();
        let tooltip = node_tooltip_text(node)?;
        return Some((key, tooltip));
    }

    None
}

/// Extract tooltip text from a node's attributes and accessibility metadata.
fn node_tooltip_text(node: &WidgetNode) -> Option<String> {
    node.attributes
        .get("title")
        .cloned()
        .or_else(|| node.attributes.get("aria-label").cloned())
        .or_else(|| node.attributes.get("description").cloned())
        .or_else(|| node.attributes.get("aria-description").cloned())
        .or_else(|| node.accessibility.label.clone())
        .or_else(|| node.accessibility.description.clone())
}

/// Find tooltip text for a specific node key in the tree.
fn find_tooltip_text_by_key(node: &WidgetNode, key: &str) -> Option<String> {
    if node.attributes.get("_mesh_key").is_some_and(|k| k == key) {
        return node_tooltip_text(node);
    }
    for child in &node.children {
        if let Some(text) = find_tooltip_text_by_key(child, key) {
            return Some(text);
        }
    }
    None
}

fn is_input_key(tree: &WidgetNode, key: &str) -> bool {
    find_node_by_key(tree, key).is_some_and(|node| node.tag == "input")
}

fn is_slider_key(tree: &WidgetNode, key: &str) -> bool {
    find_node_by_key(tree, key).is_some_and(|node| node.tag == "slider")
}

fn find_click_handler(tree: &WidgetNode, key: &str) -> Option<String> {
    find_node_by_key(tree, key)
        .and_then(|node| node.event_handlers.get("click"))
        .cloned()
}

fn namespace_event_handlers(node: &mut WidgetNode, instance_key: &str) {
    for handler in node.event_handlers.values_mut() {
        *handler = format!("__mesh_embed__::{instance_key}::{handler}");
    }

    for child in &mut node.children {
        namespace_event_handlers(child, instance_key);
    }
}

fn parse_namespaced_handler(handler: &str) -> Option<(&str, &str)> {
    let rest = handler.strip_prefix("__mesh_embed__::")?;
    rest.rsplit_once("::")
}

fn measure_content_size(
    tree: &WidgetNode,
    fallback_width: u32,
    fallback_height: u32,
    surface_id: &str,
) -> (u32, u32) {
    let bounds = if surface_prefers_content_sizing(surface_id) {
        content_children_bounds(tree, 0.0, 0.0).or_else(|| content_bounds(tree, 0.0, 0.0))
    } else {
        content_bounds(tree, 0.0, 0.0)
    };
    let width = bounds
        .map(|(_, _, right, _)| right.ceil().max(1.0) as u32)
        .unwrap_or(fallback_width);
    let height = bounds
        .map(|(_, _, _, bottom)| bottom.ceil().max(1.0) as u32)
        .unwrap_or(fallback_height);

    match surface_id {
        "@mesh/launcher" => (width.clamp(320, 640), height.clamp(180, 420)),
        "@mesh/quick-settings" => (width.clamp(320, 520), height.clamp(180, 520)),
        _ => (fallback_width, fallback_height),
    }
}

fn surface_prefers_content_sizing(surface_id: &str) -> bool {
    matches!(surface_id, "@mesh/launcher" | "@mesh/quick-settings")
}

type ContentBounds = (f32, f32, f32, f32);

fn annotate_overflow_tree(
    node: &mut WidgetNode,
    key: &str,
    scroll_offsets: &mut HashMap<String, ScrollOffsetState>,
) -> Option<ContentBounds> {
    let mut children_bounds: Option<ContentBounds> = None;

    for (index, child) in node.children.iter_mut().enumerate() {
        if let Some(child_bounds) =
            annotate_overflow_tree(child, &format!("{key}/{index}"), scroll_offsets)
        {
            children_bounds = Some(union_bounds(children_bounds, child_bounds));
        }
    }

    let content_origin_x = node.layout.x + node.computed_style.padding.left;
    let content_origin_y = node.layout.y + node.computed_style.padding.top;
    let viewport_width = (node.layout.width - node.computed_style.padding.horizontal()).max(0.0);
    let viewport_height = (node.layout.height - node.computed_style.padding.vertical()).max(0.0);

    let content_width = children_bounds
        .map(|(_, _, max_x, _)| (max_x - content_origin_x).max(0.0))
        .unwrap_or(0.0);
    let content_height = children_bounds
        .map(|(_, _, _, max_y)| (max_y - content_origin_y).max(0.0))
        .unwrap_or(0.0);

    let max_x = if node.computed_style.overflow_x.clips_contents() {
        (content_width - viewport_width).max(0.0)
    } else {
        0.0
    };
    let max_y = if node.computed_style.overflow_y.clips_contents() {
        (content_height - viewport_height).max(0.0)
    } else {
        0.0
    };

    let offset = scroll_offsets.entry(key.to_string()).or_default();
    offset.x = offset.x.clamp(0.0, max_x);
    offset.y = offset.y.clamp(0.0, max_y);

    node.attributes
        .insert("_mesh_content_width".into(), format!("{content_width:.2}"));
    node.attributes.insert(
        "_mesh_content_height".into(),
        format!("{content_height:.2}"),
    );
    node.attributes
        .insert("_mesh_scroll_max_x".into(), format!("{max_x:.2}"));
    node.attributes
        .insert("_mesh_scroll_max_y".into(), format!("{max_y:.2}"));
    node.attributes
        .insert("_mesh_scroll_x".into(), format!("{:.2}", offset.x));
    node.attributes
        .insert("_mesh_scroll_y".into(), format!("{:.2}", offset.y));

    let own_bounds = (
        node.layout.x,
        node.layout.y,
        node.layout.x + node.layout.width.max(0.0),
        node.layout.y + node.layout.height.max(0.0),
    );
    if node_clips_children(node) {
        Some(own_bounds)
    } else {
        Some(union_bounds(
            Some(own_bounds),
            children_bounds.unwrap_or(own_bounds),
        ))
    }
}

fn union_bounds(existing: Option<ContentBounds>, next: ContentBounds) -> ContentBounds {
    match existing {
        Some((min_x, min_y, max_x, max_y)) => (
            min_x.min(next.0),
            min_y.min(next.1),
            max_x.max(next.2),
            max_y.max(next.3),
        ),
        None => next,
    }
}

fn intersect_bounds(a: ContentBounds, b: ContentBounds) -> Option<ContentBounds> {
    let left = a.0.max(b.0);
    let top = a.1.max(b.1);
    let right = a.2.min(b.2);
    let bottom = a.3.min(b.3);
    if right <= left || bottom <= top {
        None
    } else {
        Some((left, top, right, bottom))
    }
}

fn node_rect_with_offset(node: &WidgetNode, offset_x: f32, offset_y: f32) -> ContentBounds {
    (
        node.layout.x + offset_x,
        node.layout.y + offset_y,
        node.layout.x + offset_x + node.layout.width.max(0.0),
        node.layout.y + offset_y + node.layout.height.max(0.0),
    )
}

fn node_scroll_offset(node: &WidgetNode) -> ScrollOffsetState {
    ScrollOffsetState {
        x: parse_node_attr_f32(node, "_mesh_scroll_x"),
        y: parse_node_attr_f32(node, "_mesh_scroll_y"),
    }
}

fn scroll_limits(node: &WidgetNode) -> (f32, f32) {
    (
        parse_node_attr_f32(node, "_mesh_scroll_max_x"),
        parse_node_attr_f32(node, "_mesh_scroll_max_y"),
    )
}

fn parse_node_attr_f32(node: &WidgetNode, key: &str) -> f32 {
    node.attributes
        .get(key)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0)
}

fn node_clips_children(node: &WidgetNode) -> bool {
    node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents()
}

fn node_is_scrollable(node: &WidgetNode) -> bool {
    let (max_x, max_y) = scroll_limits(node);
    max_x > f32::EPSILON || max_y > f32::EPSILON
}

fn child_offsets_with_scroll(node: &WidgetNode, offset_x: f32, offset_y: f32) -> (f32, f32) {
    let scroll = node_scroll_offset(node);
    (offset_x - scroll.x, offset_y - scroll.y)
}

fn content_children_bounds(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> Option<ContentBounds> {
    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    let child_clip = if node_clips_children(node) {
        Some(node_rect_with_offset(node, offset_x, offset_y))
    } else {
        None
    };

    let mut bounds: Option<ContentBounds> = None;
    for child in &node.children {
        if let Some(child_bounds) =
            content_bounds_with_clip(child, child_offset_x, child_offset_y, child_clip)
        {
            bounds = Some(union_bounds(bounds, child_bounds));
        }
    }

    bounds
}

fn content_bounds(node: &WidgetNode, offset_x: f32, offset_y: f32) -> Option<ContentBounds> {
    content_bounds_with_clip(node, offset_x, offset_y, None)
}

fn content_bounds_with_clip(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    clip: Option<ContentBounds>,
) -> Option<ContentBounds> {
    if node.computed_style.display == mesh_ui::style::Display::None {
        return None;
    }

    let rect = node_rect_with_offset(node, offset_x, offset_y);
    let own_bounds = match clip {
        Some(clip_bounds) => intersect_bounds(rect, clip_bounds),
        None => Some(rect),
    };
    if clip.is_some() && own_bounds.is_none() {
        return None;
    }
    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    let child_clip = if node_clips_children(node) {
        match clip {
            Some(clip_bounds) => intersect_bounds(rect, clip_bounds),
            None => Some(rect),
        }
    } else {
        clip
    };

    let mut bounds = own_bounds;
    for child in &node.children {
        if let Some(child_bounds) =
            content_bounds_with_clip(child, child_offset_x, child_offset_y, child_clip)
        {
            bounds = Some(union_bounds(bounds, child_bounds));
        }
    }

    bounds
}

fn find_focusable_at_with_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<String> {
    let inside_self = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside_self && node_clips_children(node) {
        return None;
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);

    for child in node.children.iter().rev() {
        if let Some(found) =
            find_focusable_at_with_offset(child, x, y, child_offset_x, child_offset_y)
        {
            return Some(found);
        }
    }

    if inside_self && matches!(node.tag.as_str(), "input" | "button" | "slider") {
        return node.attributes.get("_mesh_key").cloned();
    }

    None
}

fn find_scrollable_at_with_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<String> {
    let inside_self = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside_self && node_clips_children(node) {
        return None;
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);

    for child in node.children.iter().rev() {
        if let Some(found) =
            find_scrollable_at_with_offset(child, x, y, child_offset_x, child_offset_y)
        {
            return Some(found);
        }
    }

    if inside_self && node_is_scrollable(node) {
        return node.attributes.get("_mesh_key").cloned();
    }

    None
}

fn layout_contains_with_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> bool {
    let left = node.layout.x + offset_x;
    let top = node.layout.y + offset_y;
    x >= left && x < left + node.layout.width && y >= top && y < top + node.layout.height
}

async fn spawn_mock_backend_service(
    tx: mpsc::UnboundedSender<ShellMessage>,
    source_plugin: String,
    service: String,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    let mut step = 0u32;

    loop {
        tick.tick().await;
        step = step.wrapping_add(1);

        let summary = match service.as_str() {
            "audio" => format!("volume={}%", 25 + (step * 5 % 70)),
            "network" => {
                if step.is_multiple_of(2) {
                    "connected".to_string()
                } else {
                    "scanning".to_string()
                }
            }
            "power" => format!("battery={}%", 95 - (step % 40)),
            "media" => format!("session={step}"),
            other => format!("tick={step} service={other}"),
        };

        if tx
            .send(ShellMessage::Service(ServiceEvent::Updated {
                service: service.clone(),
                source_plugin: source_plugin.clone(),
                summary,
            }))
            .is_err()
        {
            break;
        }
    }
}

fn spawn_ipc_server(
    runtime: &Runtime,
    socket_path: PathBuf,
    tx: mpsc::UnboundedSender<ShellMessage>,
) -> Result<(), std::io::Error> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let _guard = runtime.enter();
    let listener = UnixListener::bind(&socket_path)?;
    tracing::info!("listening for ipc commands on {}", socket_path.display());

    runtime.spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(stream) => stream,
                Err(err) => {
                    tracing::warn!("ipc accept failed: {err}");
                    continue;
                }
            };

            let tx = tx.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_ipc_client(stream, tx).await {
                    tracing::warn!("ipc client failed: {err}");
                }
            });
        }
    });

    Ok(())
}

async fn handle_ipc_client(
    stream: tokio::net::UnixStream,
    tx: mpsc::UnboundedSender<ShellMessage>,
) -> Result<(), std::io::Error> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line).await?;
        if read == 0 {
            break;
        }

        let command = line.trim();
        if command.is_empty() {
            continue;
        }

        match parse_ipc_command(command) {
            Some(request) => {
                tx.send(ShellMessage::Ipc(request)).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::BrokenPipe, "shell is not running")
                })?;
                writer.write_all(b"ok\n").await?;
            }
            None => {
                writer
                    .write_all(format!("error unknown-command {command}\n").as_bytes())
                    .await?;
            }
        }
    }

    Ok(())
}

fn parse_ipc_command(command: &str) -> Option<CoreRequest> {
    match command {
        "shell:open_launcher" => Some(CoreRequest::ShowSurface {
            surface_id: "@mesh/launcher".into(),
        }),
        "shell:close_launcher" => Some(CoreRequest::HideSurface {
            surface_id: "@mesh/launcher".into(),
        }),
        "shell:toggle_launcher" => Some(CoreRequest::ToggleSurface {
            surface_id: "@mesh/launcher".into(),
        }),
        "shell:open_quick_settings" => Some(CoreRequest::ShowSurface {
            surface_id: "@mesh/quick-settings".into(),
        }),
        "shell:close_quick_settings" => Some(CoreRequest::HideSurface {
            surface_id: "@mesh/quick-settings".into(),
        }),
        "shell:toggle_quick_settings" => Some(CoreRequest::ToggleSurface {
            surface_id: "@mesh/quick-settings".into(),
        }),
        "shell:open_notification_center" => Some(CoreRequest::ShowSurface {
            surface_id: "@mesh/notification-center".into(),
        }),
        "shell:close_notification_center" => Some(CoreRequest::HideSurface {
            surface_id: "@mesh/notification-center".into(),
        }),
        "shell:toggle_notification_center" => Some(CoreRequest::ToggleSurface {
            surface_id: "@mesh/notification-center".into(),
        }),
        "shell:debug_overlay" => Some(CoreRequest::ToggleDebugOverlay),
        "shell:debug_cycle_tab" => Some(CoreRequest::CycleDebugTab),
        "shell:shutdown" => Some(CoreRequest::Shutdown),
        _ => {
            if let Some(surface_id) = command.strip_prefix("shell:show_surface:") {
                return Some(CoreRequest::ShowSurface {
                    surface_id: surface_id.to_string(),
                });
            }
            if let Some(surface_id) = command.strip_prefix("shell:hide_surface:") {
                return Some(CoreRequest::HideSurface {
                    surface_id: surface_id.to_string(),
                });
            }
            if let Some(surface_id) = command.strip_prefix("shell:toggle_surface:") {
                return Some(CoreRequest::ToggleSurface {
                    surface_id: surface_id.to_string(),
                });
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SurfaceSizePolicy, apply_service_update, build_audio_service_state,
        default_surface_layout, load_frontend_plugin_settings, measure_content_size,
        seed_service_state,
    };
    use mesh_scripting::ScriptState;
    use mesh_ui::{LayoutRect, VariableStore, WidgetNode};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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

    #[test]
    fn launcher_content_size_ignores_root_surface_bounds() {
        let mut root = node("root", 0.0, 0.0, 640.0, 360.0);
        root.children.push(node("column", 12.0, 12.0, 336.0, 332.0));

        assert_eq!(
            measure_content_size(&root, 640, 360, "@mesh/launcher"),
            (348, 344)
        );
    }

    #[test]
    fn non_widget_surfaces_keep_fallback_size() {
        let mut root = node("root", 0.0, 0.0, 1920.0, 32.0);
        root.children.push(node("row", 0.0, 0.0, 640.0, 32.0));

        assert_eq!(
            measure_content_size(&root, 1920, 32, "@mesh/panel"),
            (1920, 32)
        );
    }

    #[test]
    fn surface_layout_defaults_preserve_launcher_content_sizing() {
        let layout = default_surface_layout("@mesh/launcher");
        assert_eq!(layout.size_policy, SurfaceSizePolicy::ContentMeasured);
        assert!(!layout.visible_on_start);
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

        let settings = load_frontend_plugin_settings(&path, "@mesh/base-surface");
        fs::remove_file(&path).ok();

        assert_eq!(settings.layout.edge, mesh_wayland::Edge::Left);
        assert_eq!(settings.layout.layer, mesh_wayland::Layer::Overlay);
        assert_eq!(settings.layout.width, 960);
        assert_eq!(settings.layout.height, 640);
        assert_eq!(settings.layout.exclusive_zone, 12);
        assert_eq!(
            settings.layout.keyboard_mode,
            mesh_wayland::KeyboardMode::Exclusive
        );
        assert!(settings.layout.visible_on_start);
        assert_eq!(settings.layout.size_policy, SurfaceSizePolicy::Fixed);
    }

    #[test]
    fn audio_service_state_defaults_to_unavailable() {
        let audio = build_audio_service_state(None, None);

        assert_eq!(
            audio.get("label").and_then(|value| value.as_str()),
            Some("Unavailable")
        );
        assert_eq!(
            audio.get("available").and_then(|value| value.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn audio_service_update_populates_frontend_state() {
        let mut state = ScriptState::new();
        seed_service_state(&mut state, true);
        apply_service_update(
            &mut state,
            true,
            "audio",
            "@mesh/pipewire-audio",
            "volume=65%",
        );

        let audio = state.get("audio").expect("audio state should exist");
        assert_eq!(audio.get("label").and_then(|value| value.as_str()), Some("65%"));
        assert_eq!(audio.get("glyph").and_then(|value| value.as_str()), Some("VOL"));
        assert_eq!(
            audio.get("tooltip").and_then(|value| value.as_str()),
            Some("@mesh/pipewire-audio: 65%")
        );
    }

    fn unique_test_file(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("mesh-{prefix}-{nanos}.json"))
    }
}
