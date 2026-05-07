use mesh_core_capability::CapabilitySet;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_elements::WidgetNode;
use mesh_core_locale::LocaleEngine;
use mesh_core_render::PixelBuffer;
use mesh_core_scripting::ScriptError;
use mesh_core_theme::Theme;
use mesh_core_wayland::ShellSurface;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub type SurfaceId = String;

#[derive(Debug, Clone)]
pub enum CoreRequest {
    ToggleSurface {
        surface_id: SurfaceId,
    },
    ShowSurface {
        surface_id: SurfaceId,
    },
    HideSurface {
        surface_id: SurfaceId,
    },
    /// Reposition a surface to appear below a trigger element.
    /// Uses top-left anchor; margin_left/top position the surface precisely.
    PositionSurface {
        surface_id: SurfaceId,
        margin_top: i32,
        margin_left: i32,
    },
    PublishDiagnostics {
        message: String,
    },
    ServiceCommand {
        interface: String,
        command: String,
        payload: serde_json::Value,
        source_module_id: String,
        source_capabilities: CapabilitySet,
    },
    WriteClipboard {
        text: String,
    },
    SetTheme {
        theme_id: String,
    },
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
        source_module: String,
        /// Structured state emitted by the backend module.
        /// Stored directly into `state[service]` on all frontend components.
        payload: serde_json::Value,
    },
}

#[derive(Debug, Clone)]
pub struct ComponentContext {
    pub component_id: String,
    pub surface_id: SurfaceId,
    pub diagnostics: Diagnostics,
}

#[derive(Debug, Clone)]
pub enum ComponentInput {
    PointerMove {
        x: f32,
        y: f32,
    },
    PointerButton {
        x: f32,
        y: f32,
        pressed: bool,
    },
    Scroll {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    KeyPressed {
        key: String,
        modifiers: KeyModifiers,
    },
    KeyReleased {
        key: String,
        modifiers: KeyModifiers,
    },
    Char {
        ch: char,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
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
    fn surface_size_changed(&mut self, _width: u32, _height: u32) -> bool {
        false
    }
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
    /// Every source path that should trigger a recompile when modified.
    /// Defaults to `[source_path()]` for components that don't import
    /// sub-components; frontends that import local components override
    /// this to return the entrypoint plus every imported `.mesh` file.
    fn watched_source_paths(&self) -> Vec<PathBuf> {
        self.source_path().map(PathBuf::from).into_iter().collect()
    }
    fn reload_source(&mut self) -> Result<bool, ComponentError> {
        Ok(false)
    }
    fn module_settings_path(&self) -> Option<&Path> {
        None
    }
    fn reload_module_settings(&mut self) -> Result<bool, ComponentError> {
        Ok(false)
    }
    /// Return the last widget tree built by `paint`, for the debug layout inspector.
    fn last_widget_tree(&self) -> Option<&WidgetNode> {
        None
    }
    /// Override this surface's position for popover placement.
    /// Switches to top-left anchor and sets margins so the surface appears
    /// at (margin_left, margin_top) in screen coordinates.
    fn apply_position(&mut self, _margin_top: i32, _margin_left: i32) {}
}

pub(super) struct ComponentRuntime {
    pub(super) surface_id: SurfaceId,
    pub(super) component: Box<dyn ShellComponent>,
    /// Every `.mesh` source path that contributes to this component
    /// (entrypoint + locally imported sub-components), with each file's
    /// last-seen mtime. The hot-reload watcher recompiles when *any* of
    /// these changes — editing a sub-component triggers a reload even
    /// though the entrypoint mtime is unchanged.
    pub(super) source_paths: Vec<(PathBuf, Option<SystemTime>)>,
    pub(super) module_settings_path: Option<PathBuf>,
    pub(super) module_settings_modified_at: Option<SystemTime>,
}

impl ComponentRuntime {
    pub(super) fn new(component: Box<dyn ShellComponent>) -> Self {
        let surface_id = component.surface_id().to_string();
        let source_paths: Vec<(PathBuf, Option<SystemTime>)> = component
            .watched_source_paths()
            .into_iter()
            .map(|path| {
                let mtime = std::fs::metadata(&path)
                    .ok()
                    .and_then(|metadata| metadata.modified().ok());
                (path, mtime)
            })
            .collect();
        let module_settings_path = component.module_settings_path().map(PathBuf::from);
        Self {
            surface_id,
            component,
            source_paths,
            module_settings_path,
            module_settings_modified_at: None,
        }
    }
}

pub(super) type ServiceCommandMsg = mesh_core_backend::BackendServiceCommand;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct LatestServiceState {
    pub(super) interface: String,
    pub(super) provider_id: String,
    pub(super) state: serde_json::Value,
}

#[derive(Debug, Clone)]
pub(super) struct ThemeWatchState {
    pub(super) path: PathBuf,
    pub(super) modified_at: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub(super) struct SettingsWatchState {
    pub(super) path: PathBuf,
    pub(super) modified_at: Option<SystemTime>,
}

#[derive(Debug)]
pub(super) enum ShellMessage {
    Service(ServiceEvent),
    BackendLifecycle {
        interface: String,
        provider_id: String,
        stage: String,
        status: String,
        message: String,
    },
    Ipc(CoreRequest),
}

#[derive(Debug, Default)]
pub(super) struct ShellCoreState {
    pub(super) surfaces: HashMap<SurfaceId, SurfaceState>,
    pub(super) shutting_down: bool,
}

#[derive(Debug, Clone)]
pub(super) struct SurfaceState {
    pub(super) visible: bool,
}

impl Default for SurfaceState {
    fn default() -> Self {
        Self { visible: true }
    }
}
