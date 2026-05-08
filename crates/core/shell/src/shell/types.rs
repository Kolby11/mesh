use mesh_core_capability::CapabilitySet;
use mesh_core_debug::ProfilingStage;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_elements::WidgetNode;
use mesh_core_locale::LocaleEngine;
use mesh_core_render::PixelBuffer;
use mesh_core_scripting::ScriptError;
use mesh_core_theme::Theme;
use mesh_core_wayland::{KeyboardMode, ShellSurface};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

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
    /// Transfer keyboard focus across surfaces. Used for popover Tab flow:
    /// a button in surface A with `popover_target="X"` opens popover X and
    /// (on Tab) hands focus into X's first tabbable. Tab past X's last
    /// element (or Shift+Tab past its first) returns focus to A's element
    /// after/at the trigger and closes X if `close_source` is set.
    /// Show a surface as a popover triggered by a specific element. The
    /// shell records `(trigger_surface, trigger_key)` on the trigger's
    /// component so Tab from that key transfers focus into the popover.
    /// When `focus` is true, activation also immediately transfers focus
    /// into the popover and records the trigger as the return target.
    /// Origin: the Lua helper `mesh.popover.activate(surface_id, event,
    /// { focus = true })`.
    ActivatePopover {
        surface_id: SurfaceId,
        trigger_surface: SurfaceId,
        trigger_key: String,
        /// If true, activation immediately inserts the popover into the
        /// keyboard focus chain and records the trigger as its return target.
        focus: bool,
    },
    TransferTabFocus {
        from_surface: SurfaceId,
        to_surface: SurfaceId,
        target: TabFocusTarget,
        /// When focus enters a popover, the trigger location is recorded
        /// here so the popover knows where to send focus back on exit.
        return_target: Option<(SurfaceId, String)>,
        /// True on entry into a popover that should hide itself when Tab/
        /// Shift+Tab leaves its chain. The shell stamps this as the
        /// target's `close_on_focus_leave` flag.
        target_closes_on_leave: bool,
        /// Surface to hide as part of *this* transfer (typically the
        /// popover the user is leaving). Translates into a HideSurface.
        close_source: Option<SurfaceId>,
    },
    ToggleDebugOverlay,
    ToggleDebugProfiling,
    CycleDebugTab,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabFocusTarget {
    /// First tabbable in the target surface's tree.
    First,
    /// Last tabbable in the target surface's tree.
    Last,
    /// The named key itself (used when Shift+Tab leaves a popover and
    /// lands on the trigger button).
    AtKey(String),
    /// Tabbable that immediately follows `key` in the target surface's
    /// tree order (used when forward Tab leaves a popover and skips past
    /// the trigger).
    AfterKey(String),
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

#[derive(Debug, Clone)]
pub struct ComponentProfilingRecord {
    pub stage: ProfilingStage,
    pub duration: Duration,
    pub module_id: Option<String>,
    pub trigger_kind: Option<String>,
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
    /// Receive a cross-surface Tab focus transfer initiated from another
    /// component. The default implementation is a no-op so non-frontend
    /// components (e.g. test stubs) can ignore this.
    fn receive_focus_transfer(
        &mut self,
        _target: &TabFocusTarget,
        _return_focus: Option<(SurfaceId, String)>,
        _close_on_focus_leave: bool,
    ) {
    }
    /// Drop focus state on a component that just transferred focus away.
    fn release_focus_for_transfer(&mut self) {}
    /// Record that `trigger_key` in this component's surface activated
    /// the popover at `popover_surface`. Tab forward on that key will
    /// transfer focus into the popover.
    fn register_popover_trigger(&mut self, _trigger_key: String, _popover_surface: SurfaceId) {}
    /// Drop a previously-registered popover trigger when the popover
    /// hides (so a stale Tab doesn't try to re-enter a closed surface).
    fn unregister_popover_trigger(&mut self, _popover_surface: &str) {}
    /// Override the surface's effective keyboard_mode at runtime,
    /// shadowing the configured value from the manifest until cleared.
    /// Used during cross-surface Tab transfer.
    fn set_keyboard_mode_override(&mut self, _mode: Option<KeyboardMode>) {}
    fn set_profiling_enabled(&mut self, _enabled: bool) {}
    fn take_profiling_records(&mut self) -> Vec<ComponentProfilingRecord> {
        Vec::new()
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
    fn allows_shrink_to_fit(&self) -> bool {
        false
    }
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

/// Per-(interface, command) leading+trailing throttle state for coalescable
/// service commands. Leading edge fires immediately; subsequent calls within
/// the interval park as `pending` (last-wins) and are flushed by the main
/// loop on the next tick after the interval elapses.
#[derive(Debug, Clone)]
pub(super) struct CommandThrottleState {
    pub(super) last_send: std::time::Instant,
    pub(super) pending: Option<serde_json::Value>,
}

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
    BackendServiceUpdate {
        interface: String,
        provider_id: String,
        event: ServiceEvent,
    },
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
