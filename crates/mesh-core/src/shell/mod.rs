use mesh_config::{
    ShellConfig, ShellSettings, default_settings_path, load_config, load_shell_settings,
};
use mesh_diagnostics::DiagnosticsCollector;
use mesh_events::EventBus;
use mesh_locale::LocaleEngine;
use mesh_plugin::lifecycle::{PluginInstance, PluginState};
use mesh_plugin::{DependencyGraphError, PluginType, validate_plugin_dependency_graph};
use mesh_debug::{DebugOverlayState, DebugSnapshot, HealthEntry, InterfaceEntry, PluginEntry, ProviderEntry};
use mesh_renderer::{
    DebugOverlay, DevWindowBackend, DevWindowEvent, DevWindowKeyEvent, LayerShellBackend,
    LayerSurfaceConfig, Painter, PixelBuffer,
};
use mesh_service::{
    InterfaceProvider, InterfaceRegistry, ServiceRegistry, canonical_interface_name,
    load_interface_contract,
};
use mesh_theme::ThemeEngine;
use mesh_wayland::{Layer, StubSurface};

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::sync::mpsc;

mod backend;
mod component;
mod ipc;
mod layout;
mod service;
mod sounds;
mod surface_layout;
mod types;

pub use types::{
    ComponentContext, ComponentError, ComponentInput, CoreEvent, CoreRequest,
    ServiceEvent, ShellComponent, SurfaceId,
};
use types::{ComponentRuntime, ServiceCommandMsg, ShellCoreState, ShellMessage, SurfaceState, ThemeWatchState, SettingsWatchState};
use surface_layout::{
    default_surface_visibility, load_active_theme,
};
use component::{BackendServiceCandidate, FrontendCatalog, FrontendSurfaceComponent};
use backend::spawn_backend_service;
use ipc::spawn_ipc_server;
use sounds::{SoundKind, play_shell_sound};

use service::service_name_from_interface;

thread_local! {
    static FRONTEND_PAINTER: RefCell<Painter> = RefCell::new(mesh_renderer::Painter::new());
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
    service_handlers: HashMap<String, mpsc::UnboundedSender<ServiceCommandMsg>>,
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

fn coalesce_pointer_moves(events: Vec<DevWindowEvent>) -> Vec<DevWindowEvent> {
    if events.len() < 2 {
        return events;
    }

    let mut output = Vec::with_capacity(events.len());
    let mut pending_moves: HashMap<String, DevWindowEvent> = HashMap::new();

    for event in events {
        match event {
            DevWindowEvent::PointerMove { surface_id, x, y } => {
                pending_moves.insert(
                    surface_id.clone(),
                    DevWindowEvent::PointerMove { surface_id, x, y },
                );
            }
            event => {
                let surface_id = dev_window_event_surface_id(&event).to_string();
                if let Some(pointer_move) = pending_moves.remove(&surface_id) {
                    output.push(pointer_move);
                }
                output.push(event);
            }
        }
    }

    output.extend(pending_moves.into_values());
    output
}

fn dev_window_event_surface_id(event: &DevWindowEvent) -> &str {
    match event {
        DevWindowEvent::PointerMove { surface_id, .. }
        | DevWindowEvent::PointerButton { surface_id, .. }
        | DevWindowEvent::Scroll { surface_id, .. }
        | DevWindowEvent::Key { surface_id, .. }
        | DevWindowEvent::Char { surface_id, .. } => surface_id,
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
            service_handlers: HashMap::new(),
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
            match mesh_plugin::manifest::load_manifest(dir) {
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

        let theme = mesh_theme::load_theme_from_path(&self.theme_watch.path).map_err(ShellRunError::Theme)?;
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

        let old_theme = self.settings.theme.clone();
        let old_i18n = self.settings.i18n.clone();
        let new_i18n = &new_settings.i18n;
        let locale_changed = old_i18n.locale != new_i18n.locale
            || old_i18n.fallback_locale != new_i18n.fallback_locale;

        let theme_changed = old_theme.active != new_settings.theme.active;
        if theme_changed {
            let (theme, theme_watch) = load_active_theme(&new_settings);
            tracing::info!(
                "active theme changed: {} -> {}",
                old_theme.active,
                new_settings.theme.active
            );
            self.theme = theme;
            self.theme_watch = theme_watch;
            self.mark_components_theme_changed()?;
        }

        if locale_changed
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
            self.mark_components_locale_changed()?;
        }

        self.settings = new_settings;

        Ok(())
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
            CoreRequest::PositionSurface { surface_id, margin_top, margin_left } => {
                if let Some(runtime) = self.components.iter_mut().find(|r| r.surface_id == surface_id) {
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
            CoreRequest::ServiceCommand { interface, command, payload } => {
                self.dispatch_service_command(&interface, &command, &payload);
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

    fn dispatch_service_command(&self, interface: &str, command: &str, payload: &serde_json::Value) {
        if let Some(tx) = self.service_handlers.get(interface) {
            let _ = tx.send(ServiceCommandMsg {
                command: command.to_string(),
                payload: payload.clone(),
            });
        } else {
            tracing::debug!("no handler registered for service: {interface}");
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
                self.windows.configure(&runtime.surface_id, cfg);

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
                        self.debug_overlay.paint_layout_bounds(tree, &mut buffer, 1.0);
                    }
                }
                self.debug_overlay.paint_panel(snapshot, self.debug.active_tab, &mut buffer, 1.0);
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
        let events = coalesce_pointer_moves(self.windows.poll_events());
        for event in events {
            tracing::trace!(
                "[hover] dispatch_wayland: got event {:?}",
                std::mem::discriminant(&event)
            );
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

    fn spawn_backend_plugins(&mut self, runtime: &Runtime, tx: mpsc::UnboundedSender<ShellMessage>) {
        let mut plugin_ids: Vec<String> = self.plugins.keys().cloned().collect();
        plugin_ids.sort();
        let mut services: HashMap<String, Vec<BackendServiceCandidate>> = HashMap::new();

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

            let service_name = service_name_from_interface(&service.provides);
            services
                .entry(service_name.clone())
                .or_default()
                .push(BackendServiceCandidate {
                    plugin_id: plugin.manifest.package.id.clone(),
                    priority: service.priority,
                });
        }

        for (service_name, mut candidates) in services {
            candidates.sort_by(|a, b| {
                b.priority
                    .cmp(&a.priority)
                    .then_with(|| a.plugin_id.cmp(&b.plugin_id))
            });

            let Some(candidate) = candidates.into_iter().next() else {
                continue;
            };

            let interface = format!("mesh.{service_name}");
            let script_source = self.plugins.get(&candidate.plugin_id).and_then(|plugin| {
                plugin
                    .manifest
                    .entrypoints
                    .main
                    .as_deref()
                    .map(|entry| plugin.path.join(entry))
            }).and_then(|path| std::fs::read_to_string(&path).ok());

            let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
            self.service_handlers.insert(interface.clone(), cmd_tx);

            match script_source {
                Some(source) => {
                    runtime.spawn(spawn_backend_service(
                        candidate.plugin_id,
                        service_name,
                        source,
                        tx.clone(),
                        cmd_rx,
                    ));
                }
                None => {
                    tracing::warn!("backend plugin {} has no readable script", candidate.plugin_id);
                }
            }
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

#[cfg(test)]
mod tests {
    use super::{
        surface_layout::{SurfaceSizePolicy, load_frontend_plugin_settings},
        layout::measure_content_size,
        service::{apply_service_update, seed_service_state, service_name_from_interface},
    };
    use mesh_plugin::manifest::{
        Manifest, PackageSection, PluginType, SurfaceLayoutSection,
        CapabilitiesSection, CompatibilitySection, DependenciesSection, EntrypointsSection,
        ExportsSection,
    };
    use mesh_scripting::ScriptState;
    use mesh_ui::{LayoutRect, VariableStore, WidgetNode};
    use std::collections::HashMap;
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
            translations: HashMap::new(),
            surface_layout: None,
        }
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

        assert_eq!(
            measure_content_size(&root, 1920, 32, None),
            (1920, 32)
        );
    }

    #[test]
    fn launcher_plugin_json_declares_content_measured_policy() {
        let workspace_root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let launcher_plugin_dir =
            workspace_root.join("plugins/frontend/core/launcher");
        let plugin_json =
            std::fs::read_to_string(launcher_plugin_dir.join("plugin.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&plugin_json).unwrap();
        assert_eq!(
            value.pointer("/surface_layout/size_policy").and_then(|v| v.as_str()),
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
