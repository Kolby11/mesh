/// The top-level Shell struct that owns shell coordination and plugin loading.
use mesh_component_backend::{CompiledFrontendPlugin, compile_frontend_plugin, is_frontend_plugin};
use mesh_capability::{Capability, CapabilitySet};
use mesh_config::{ShellConfig, ShellSettings, load_config, load_shell_settings};
use mesh_diagnostics::DiagnosticsCollector;
use mesh_events::EventBus;
use mesh_locale::LocaleEngine;
use mesh_plugin::lifecycle::{PluginInstance, PluginState};
use mesh_plugin::{PluginType, manifest};
use mesh_renderer::{DevWindowBackend, DevWindowEvent, DevWindowKeyEvent, Painter, PixelBuffer};
use mesh_scripting::{ScriptContext, ScriptError};
use mesh_service::ServiceRegistry;
use mesh_theme::{Theme, ThemeEngine, default_theme, load_theme_from_path, theme_path_for_id};
use mesh_ui::WidgetNode;
use mesh_wayland::{Edge, KeyboardMode, Layer, ShellSurface, StubSurface};

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::time::Duration;

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
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum CoreEvent {
    Started,
    SurfaceVisibilityChanged { surface_id: SurfaceId, visible: bool },
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
}

struct ComponentRuntime {
    surface_id: SurfaceId,
    component: Box<dyn ShellComponent>,
    source_path: Option<PathBuf>,
    source_modified_at: Option<SystemTime>,
}

impl ComponentRuntime {
    fn new(component: Box<dyn ShellComponent>) -> Self {
        let surface_id = component.surface_id().to_string();
        let source_path = component.source_path().map(PathBuf::from);
        let source_modified_at = source_path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).ok())
            .and_then(|metadata| metadata.modified().ok());
        Self {
            surface_id,
            component,
            source_path,
            source_modified_at,
        }
    }
}

#[derive(Debug, Clone)]
struct ThemeWatchState {
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

pub struct Shell {
    pub config: ShellConfig,
    pub settings: ShellSettings,
    pub theme: ThemeEngine,
    pub locale: LocaleEngine,
    pub events: EventBus,
    pub diagnostics: DiagnosticsCollector,
    pub services: ServiceRegistry,
    plugins: HashMap<String, PluginInstance>,
    plugin_dirs: Vec<PathBuf>,
    core: ShellCoreState,
    components: Vec<ComponentRuntime>,
    surfaces: HashMap<SurfaceId, StubSurface>,
    windows: DevWindowBackend,
    theme_watch: ThemeWatchState,
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

        Self {
            config,
            settings,
            theme,
            locale,
            events: EventBus::new(),
            diagnostics: DiagnosticsCollector::new(),
            services: ServiceRegistry::new(),
            plugins: HashMap::new(),
            plugin_dirs: default_plugin_dirs(),
            core: ShellCoreState::default(),
            components: Vec::new(),
            surfaces: HashMap::new(),
            windows: DevWindowBackend::new(),
            theme_watch,
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
        let manifest_path = dir.join("mesh.toml");
        if manifest_path.exists() {
            match manifest::load_manifest(dir) {
                Ok(manifest) => {
                    let id = manifest.package.id.clone();
                    tracing::info!(
                        "discovered plugin: {} v{} ({})",
                        id,
                        manifest.package.version,
                        manifest.package.plugin_type
                    );
                    self.plugins.insert(id, PluginInstance::new(manifest, dir.to_path_buf()));
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

    pub fn resolve_plugins(&mut self) {
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
    }

    pub fn plugin(&self, id: &str) -> Option<&PluginInstance> {
        self.plugins.get(id)
    }

    pub fn plugins(&self) -> impl Iterator<Item = (&str, PluginState)> {
        self.plugins
            .iter()
            .map(|(id, inst)| (id.as_str(), inst.state))
    }

    pub fn run(&mut self) -> Result<(), ShellRunError> {
        self.discover_plugins();
        self.resolve_plugins();
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
        pending.extend(self.broadcast_core_event(CoreEvent::Started)?);

        tracing::info!(
            "MESH shell core is running with {} frontend component(s)",
            self.components.len()
        );

        while !self.core.shutting_down {
            self.reload_theme_if_changed()?;
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

    fn load_frontend_components(&mut self) -> Result<(), ShellRunError> {
        if !self.components.is_empty() {
            return Ok(());
        }

        let mut plugin_ids: Vec<String> = self.plugins.keys().cloned().collect();
        plugin_ids.sort();

        for plugin_id in plugin_ids {
            let Some(plugin) = self.plugins.get(&plugin_id) else {
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

            self.register_component(Box::new(FrontendSurfaceComponent::new(
                compiled,
                plugin.path.clone(),
            )));
        }

        Ok(())
    }

    fn register_component(&mut self, component: Box<dyn ShellComponent>) {
        let surface_id = component.surface_id().to_string();
        self.core
            .surfaces
            .entry(surface_id.clone())
            .or_insert_with(|| SurfaceState {
                visible: default_surface_visibility(&surface_id),
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
            requests.extend(
                runtime
                    .component
                    .tick()
                    .map_err(ShellRunError::Component)?,
            );
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
            CoreRequest::ShowSurface { surface_id } => self.set_surface_visibility(surface_id, true),
            CoreRequest::HideSurface { surface_id } => {
                self.set_surface_visibility(surface_id, false)
            }
            CoreRequest::PublishDiagnostics { message } => {
                tracing::info!("diagnostic: {message}");
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

        self.broadcast_core_event(CoreEvent::SurfaceVisibilityChanged { surface_id, visible })
    }

    fn render_components(&mut self) -> Result<(), ShellRunError> {
        for runtime in &mut self.components {
            if !runtime.component.wants_render() {
                continue;
            }

            let surface = self
                .surfaces
                .get_mut(&runtime.surface_id)
                .ok_or_else(|| ShellRunError::MissingSurface(runtime.surface_id.clone()))?;
            runtime
                .component
                .render(surface)
                .map_err(ShellRunError::Component)?;

            let mut buffer = PixelBuffer::new(surface.width.max(1), surface.height.max(1));
            runtime
                .component
                .paint(self.theme.active(), surface.width.max(1), surface.height.max(1), &mut buffer)
                .map_err(ShellRunError::Component)?;

            let visible = self
                .core
                .surfaces
                .get(&runtime.surface_id)
                .map(|state| state.visible)
                .unwrap_or(surface.visible);
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

            let input = match event {
                DevWindowEvent::PointerMove { x, y, .. } => ComponentInput::PointerMove { x, y },
                DevWindowEvent::PointerButton { x, y, pressed, .. } => {
                    ComponentInput::PointerButton { x, y, pressed }
                }
                DevWindowEvent::Scroll { x, y, dx, dy, .. } => {
                    ComponentInput::Scroll { x, y, dx, dy }
                }
                DevWindowEvent::Key {
                    event: DevWindowKeyEvent::Pressed(key),
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

            let Some(service) = plugin.manifest.service.clone() else {
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
        workspace_root.join("plugins/backend/core"),
        workspace_root.join("plugins/frontend/core"),
        PathBuf::from("/usr/share/mesh/plugins/backend"),
        PathBuf::from("/usr/share/mesh/plugins/frontend"),
        PathBuf::from(&home).join(".local/share/mesh/plugins/backend"),
        PathBuf::from(&home).join(".local/share/mesh/plugins/frontend"),
        PathBuf::from(&home).join(".local/share/mesh/dev-plugins/backend"),
        PathBuf::from(&home).join(".local/share/mesh/dev-plugins/frontend"),
    ]
}

fn default_surface_visibility(surface_id: &str) -> bool {
    surface_id == "@mesh/launcher"
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
    visible: bool,
    dirty: bool,
    last_service_update: Option<String>,
    focused_key: Option<String>,
    pointer_down_key: Option<String>,
    active_slider_key: Option<String>,
    input_values: HashMap<String, String>,
    slider_values: HashMap<String, f32>,
    scroll_offsets: HashMap<String, f32>,
    script_ctx: ScriptContext,
    measured_size: Option<(u32, u32)>,
}

impl FrontendSurfaceComponent {
    fn new(compiled: CompiledFrontendPlugin, plugin_dir: PathBuf) -> Self {
        let surface_id = compiled.surface_id().to_string();
        let component_id = compiled.manifest.package.id.clone();
        let granted_capabilities = grant_capabilities_from_manifest(&compiled.manifest);
        Self {
            compiled,
            plugin_dir,
            visible: default_surface_visibility(&surface_id),
            dirty: true,
            last_service_update: None,
            focused_key: None,
            pointer_down_key: None,
            active_slider_key: None,
            input_values: HashMap::new(),
            slider_values: HashMap::new(),
            scroll_offsets: HashMap::new(),
            script_ctx: ScriptContext::new(
                component_id.clone(),
                granted_capabilities,
            )
            .unwrap_or_else(|err| panic!("failed to create script context for {component_id}: {err}")),
            measured_size: None,
        }
    }

    fn render_layout(&self, surface: &mut dyn ShellSurface) {
        match self.compiled.manifest.package.id.as_str() {
            "@mesh/panel" => {
                surface.anchor(Edge::Top);
                surface.set_layer(Layer::Top);
                surface.set_size(1920, 32);
                surface.set_exclusive_zone(32);
            }
            "@mesh/launcher" => {
                surface.anchor(Edge::Top);
                surface.set_layer(Layer::Overlay);
                let (width, height) = self.measured_size.unwrap_or((640, 360));
                surface.set_size(width, height);
                surface.set_exclusive_zone(0);
                surface.set_keyboard_interactivity(KeyboardMode::OnDemand);
            }
            "@mesh/notification-center" => {
                surface.anchor(Edge::Right);
                surface.set_layer(Layer::Overlay);
                surface.set_size(420, 720);
                surface.set_exclusive_zone(0);
                surface.set_keyboard_interactivity(KeyboardMode::OnDemand);
            }
            "@mesh/quick-settings" => {
                surface.anchor(Edge::Top);
                surface.set_layer(Layer::Overlay);
                surface.set_size(480, 420);
                surface.set_exclusive_zone(0);
                surface.set_keyboard_interactivity(KeyboardMode::OnDemand);
            }
            _ => {
                surface.anchor(Edge::Top);
                surface.set_layer(Layer::Top);
                surface.set_size(480, 240);
                surface.set_exclusive_zone(0);
            }
        }
    }

    fn build_tree(&self, theme: &Theme, width: u32, height: u32) -> WidgetNode {
        let mut tree = self.compiled.build_preview_tree_with_state(
            theme,
            width,
            height,
            Some(self.script_ctx.state()),
        );
        annotate_runtime_tree(
            &mut tree,
            "root".to_string(),
            &self.focused_key,
            &self.input_values,
            &self.slider_values,
            &self.scroll_offsets,
        );
        tree
    }

    fn update_slider_from_position(&mut self, tree: &WidgetNode, slider_key: &str, x: f32) {
        let Some(node) = find_node_by_key(tree, slider_key) else {
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

        let local_x = (x - node.layout.x).clamp(0.0, node.layout.width.max(1.0));
        let pct = (local_x / node.layout.width.max(1.0)).clamp(0.0, 1.0);
        let value = min + (max - min) * pct;
        self.slider_values.insert(slider_key.to_string(), value);
    }

    fn init_script(&mut self) -> Result<(), ComponentError> {
        self.script_ctx = ScriptContext::new(
            self.compiled.manifest.package.id.clone(),
            grant_capabilities_from_manifest(&self.compiled.manifest),
        )
        .map_err(|source| ComponentError::Script {
            component_id: self.id().to_string(),
            source,
        })?;

        if let Some(script) = &self.compiled.component.script {
            self.script_ctx
                .load_script(&script.source)
                .map_err(|source| ComponentError::Script {
                    component_id: self.id().to_string(),
                    source,
                })?;
            self.script_ctx
                .call_init()
                .map_err(|source| ComponentError::Script {
                    component_id: self.id().to_string(),
                    source,
                })?;
        }

        Ok(())
    }
}

impl ShellComponent for FrontendSurfaceComponent {
    fn id(&self) -> &str {
        &self.compiled.manifest.package.id
    }

    fn surface_id(&self) -> &str {
        self.compiled.surface_id()
    }

    fn mount(&mut self, _ctx: ComponentContext) -> Result<Vec<CoreRequest>, ComponentError> {
        self.init_script()?;
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
        if let CoreEvent::SurfaceVisibilityChanged { surface_id, visible } = event {
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
        self.dirty = true;
        Ok(Vec::new())
    }

    fn tick(&mut self) -> Result<Vec<CoreRequest>, ComponentError> {
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
        let role = self
            .compiled
            .component
            .meta
            .as_ref()
            .and_then(|meta| meta.role.as_ref().map(ToString::to_string))
            .unwrap_or_else(|| "unknown".into());

        tracing::info!(
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
        Painter::new().paint(&tree, buffer, 1.0);
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), ComponentError> {
        self.dirty = true;
        Ok(())
    }

    fn source_path(&self) -> Option<&Path> {
        Some(self.compiled.source_path.as_path())
    }

    fn reload_source(&mut self) -> Result<bool, ComponentError> {
        let manifest = self.compiled.manifest.clone();
        let recompiled = compile_frontend_plugin(&manifest, &self.plugin_dir).map_err(|err| {
            ComponentError::Failed {
                component_id: self.id().to_string(),
                message: format!("frontend recompile failed: {err}"),
            }
        })?;

        self.compiled = recompiled;
        self.init_script()?;
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
                                self.script_ctx
                                    .call_handler(&handler, &[])
                                    .map_err(|source| ComponentError::Script {
                                        component_id: self.id().to_string(),
                                        source,
                                    })?;
                                self.dirty = true;
                            }
                        }
                    }
                    self.pointer_down_key = None;
                    self.active_slider_key = None;
                }
            }
            ComponentInput::PointerMove { x, .. } => {
                if let Some(slider_key) = self.active_slider_key.clone() {
                    self.update_slider_from_position(&tree, &slider_key, x);
                    self.dirty = true;
                }
            }
            ComponentInput::Scroll { x, y, dy, .. } => {
                if let Some(scroll_key) = find_scrollable_at(&tree, x, y) {
                    let current = self.scroll_offsets.get(&scroll_key).copied().unwrap_or(0.0);
                    let next = (current - dy * 28.0).max(0.0);
                    self.scroll_offsets.insert(scroll_key, next);
                    self.dirty = true;
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
}

fn annotate_runtime_tree(
    node: &mut WidgetNode,
    key: String,
    focused_key: &Option<String>,
    input_values: &HashMap<String, String>,
    slider_values: &HashMap<String, f32>,
    scroll_offsets: &HashMap<String, f32>,
) {
    node.attributes.insert("_mesh_key".into(), key.clone());

    if focused_key.as_deref() == Some(key.as_str()) {
        node.attributes.insert("_mesh_focused".into(), "true".into());
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
            node.attributes.insert("value".into(), format!("{value:.2}"));
        }
        "scroll" => {
            let offset = scroll_offsets.get(&key).copied().unwrap_or(0.0);
            node.attributes
                .insert("_mesh_scroll_y".into(), format!("{offset:.2}"));
        }
        _ => {}
    }

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

fn find_node_by_key<'a>(node: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
    if node.attributes.get("_mesh_key").is_some_and(|value| value == key) {
        return Some(node);
    }

    for child in &node.children {
        if let Some(found) = find_node_by_key(child, key) {
            return Some(found);
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

fn measure_content_size(
    tree: &WidgetNode,
    fallback_width: u32,
    fallback_height: u32,
    surface_id: &str,
) -> (u32, u32) {
    let bounds = content_bounds(tree, 0.0, 0.0);
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

fn content_bounds(node: &WidgetNode, offset_x: f32, offset_y: f32) -> Option<(f32, f32, f32, f32)> {
    if node.computed_style.display == mesh_ui::style::Display::None {
        return None;
    }

    let left = node.layout.x + offset_x;
    let top = node.layout.y + offset_y;
    let right = left + node.layout.width.max(0.0);
    let bottom = top + node.layout.height.max(0.0);

    let child_offset_x = offset_x;
    let mut child_offset_y = offset_y;
    if node.tag == "scroll" {
        let scroll_y = node
            .attributes
            .get("_mesh_scroll_y")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        child_offset_y -= scroll_y;
    }

    let mut bounds = Some((left, top, right, bottom));
    for child in &node.children {
        if let Some(child_bounds) = content_bounds(child, child_offset_x, child_offset_y) {
            bounds = Some(match bounds {
                Some((min_x, min_y, max_x, max_y)) => (
                    min_x.min(child_bounds.0),
                    min_y.min(child_bounds.1),
                    max_x.max(child_bounds.2),
                    max_y.max(child_bounds.3),
                ),
                None => child_bounds,
            });
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
    if !layout_contains_with_offset(node, x, y, offset_x, offset_y) {
        return None;
    }

    let child_offset_x = offset_x;
    let mut child_offset_y = offset_y;
    if node.tag == "scroll" {
        let scroll_y = node
            .attributes
            .get("_mesh_scroll_y")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        child_offset_y -= scroll_y;
    }

    for child in node.children.iter().rev() {
        if let Some(found) = find_focusable_at_with_offset(child, x, y, child_offset_x, child_offset_y) {
            return Some(found);
        }
    }

    if matches!(node.tag.as_str(), "input" | "button" | "slider") {
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
    if !layout_contains_with_offset(node, x, y, offset_x, offset_y) {
        return None;
    }

    let child_offset_x = offset_x;
    let mut child_offset_y = offset_y;
    if node.tag == "scroll" {
        let scroll_y = node
            .attributes
            .get("_mesh_scroll_y")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        child_offset_y -= scroll_y;
    }

    for child in node.children.iter().rev() {
        if let Some(found) = find_scrollable_at_with_offset(child, x, y, child_offset_x, child_offset_y) {
            return Some(found);
        }
    }

    if node.tag == "scroll" {
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
