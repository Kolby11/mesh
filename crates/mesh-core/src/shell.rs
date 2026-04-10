/// The top-level Shell struct that owns shell coordination and plugin loading.
use mesh_component_backend::{CompiledFrontendPlugin, compile_frontend_plugin, is_frontend_plugin};
use mesh_config::{ShellConfig, ShellSettings, load_config, load_shell_settings};
use mesh_diagnostics::DiagnosticsCollector;
use mesh_events::EventBus;
use mesh_locale::LocaleEngine;
use mesh_plugin::lifecycle::{PluginInstance, PluginState};
use mesh_plugin::{PluginType, manifest};
use mesh_renderer::{DevWindowBackend, Painter, PixelBuffer};
use mesh_service::ServiceRegistry;
use mesh_theme::{Theme, ThemeEngine, default_theme};
use mesh_wayland::{Edge, Layer, ShellSurface, StubSurface};

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
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

#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    #[error("component '{component_id}' failed: {message}")]
    Failed {
        component_id: String,
        message: String,
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
        &self,
        theme: &Theme,
        width: u32,
        height: u32,
        buffer: &mut PixelBuffer,
    ) -> Result<(), ComponentError>;
}

struct ComponentRuntime {
    surface_id: SurfaceId,
    component: Box<dyn ShellComponent>,
}

impl ComponentRuntime {
    fn new(component: Box<dyn ShellComponent>) -> Self {
        let surface_id = component.surface_id().to_string();
        Self {
            surface_id,
            component,
        }
    }
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
        let theme = ThemeEngine::new(default_theme());
        if theme.active().id != settings.theme.active {
            tracing::warn!(
                "requested theme '{}' is not registered yet; using '{}'",
                settings.theme.active,
                theme.active().id
            );
        }
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

            self.register_component(Box::new(FrontendSurfaceComponent::new(compiled)));
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
        for runtime in &self.components {
            let surface = self
                .surfaces
                .get(&runtime.surface_id)
                .ok_or_else(|| ShellRunError::MissingSurface(runtime.surface_id.clone()))?;
            tracing::trace!(
                "dispatching surface '{}' visible={}",
                runtime.surface_id,
                surface.visible
            );
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
    !matches!(surface_id, "@mesh/notification-center" | "@mesh/quick-settings")
}

struct FrontendSurfaceComponent {
    compiled: CompiledFrontendPlugin,
    visible: bool,
    dirty: bool,
    last_service_update: Option<String>,
}

impl FrontendSurfaceComponent {
    fn new(compiled: CompiledFrontendPlugin) -> Self {
        let surface_id = compiled.surface_id().to_string();
        Self {
            compiled,
            visible: default_surface_visibility(&surface_id),
            dirty: true,
            last_service_update: None,
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
                surface.set_size(640, 480);
                surface.set_exclusive_zone(0);
            }
            "@mesh/notification-center" => {
                surface.anchor(Edge::Right);
                surface.set_layer(Layer::Overlay);
                surface.set_size(420, 720);
                surface.set_exclusive_zone(0);
            }
            "@mesh/quick-settings" => {
                surface.anchor(Edge::Top);
                surface.set_layer(Layer::Overlay);
                surface.set_size(480, 420);
                surface.set_exclusive_zone(0);
            }
            _ => {
                surface.anchor(Edge::Top);
                surface.set_layer(Layer::Top);
                surface.set_size(480, 240);
                surface.set_exclusive_zone(0);
            }
        }
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
        &self,
        theme: &Theme,
        width: u32,
        height: u32,
        buffer: &mut PixelBuffer,
    ) -> Result<(), ComponentError> {
        let tree = self.compiled.build_preview_tree(theme, width, height);
        buffer.clear(tree.computed_style.background_color);
        Painter::new().paint(&tree, buffer, 1.0);
        Ok(())
    }
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
