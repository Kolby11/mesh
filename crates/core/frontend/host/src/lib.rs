use std::path::{Path, PathBuf};
use std::time::Duration;

use mesh_core_capability::CapabilitySet;
use mesh_core_debug::ProfilingStage;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_elements::WidgetNode;
use mesh_core_locale::LocaleEngine;
use mesh_core_render::{DamageRect, PixelBuffer};
use mesh_core_scripting::ScriptError;
use mesh_core_theme::Theme;
use mesh_core_wayland::{KeyboardMode, ShellSurface};

pub type SurfaceId = String;

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
    PointerLeave,
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
    /// Show a surface as a popover triggered by a specific element.
    ///
    /// The shell records `(trigger_surface, trigger_key)` on the trigger's
    /// component so Tab from that key transfers focus into the popover. When
    /// `focus` is true, activation also immediately transfers focus into the
    /// popover and records the trigger as the return target.
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
        /// Surface to hide as part of this transfer.
        close_source: Option<SurfaceId>,
    },
    ToggleDebugOverlay,
    ToggleDebugLayoutBounds,
    ToggleDebugProfiling,
    RunDebugBenchmark {
        scenario_id: String,
    },
    CycleDebugTab,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabFocusTarget {
    /// First tabbable in the target surface's tree.
    First,
    /// Last tabbable in the target surface's tree.
    Last,
    /// The named key itself.
    AtKey(String),
    /// Tabbable that immediately follows `key` in the target surface's tree order.
    AfterKey(String),
}

#[derive(Debug, Clone)]
pub enum CoreEvent {
    Started,
    SurfaceVisibilityChanged {
        surface_id: SurfaceId,
        visible: bool,
    },
    ThemeChanged {
        theme_id: String,
        is_dark: bool,
    },
    LocaleChanged {
        locale: String,
    },
    ShuttingDown,
}

#[derive(Debug, Clone)]
pub enum ServiceEvent {
    Updated {
        service: String,
        source_module: String,
        /// Structured state emitted by the backend module.
        payload: serde_json::Value,
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
    /// components can ignore this.
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
    /// the popover at `popover_surface`.
    fn register_popover_trigger(&mut self, _trigger_key: String, _popover_surface: SurfaceId) {}
    /// Drop a previously-registered popover trigger when the popover hides.
    fn unregister_popover_trigger(&mut self, _popover_surface: &str) {}
    /// Override the surface's effective keyboard_mode at runtime.
    fn set_keyboard_mode_override(&mut self, _mode: Option<KeyboardMode>) {}
    fn debug_keybinds(&self) -> Vec<mesh_core_debug::DebugKeybindEntry> {
        Vec::new()
    }
    fn set_profiling_enabled(&mut self, _enabled: bool) {}
    fn take_profiling_records(&mut self) -> Vec<ComponentProfilingRecord> {
        Vec::new()
    }
    fn take_invalidation_snapshot(
        &mut self,
    ) -> Option<mesh_core_debug::ProfilingInvalidationSnapshot> {
        None
    }
    /// Return the damage from the most recent paint for partial presentation.
    fn take_present_damage(&mut self) -> Option<DamageRect> {
        None
    }
    /// Whether pending dirtiness should be resolved in the same render pass.
    fn wants_immediate_rerender(&self) -> bool {
        self.wants_render()
    }
    fn source_path(&self) -> Option<&Path> {
        None
    }
    /// Every source path that should trigger a recompile when modified.
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
    fn apply_position(&mut self, _margin_top: i32, _margin_left: i32) {}
    /// Duration in milliseconds to keep a surface mapped while it exits.
    fn hide_transition_ms(&self) -> u64 {
        0
    }
    /// Mark whether the surface is currently playing its hide transition.
    fn set_surface_exiting(&mut self, _exiting: bool) {}
    fn allows_shrink_to_fit(&self) -> bool {
        false
    }
}
