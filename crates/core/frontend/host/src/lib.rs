use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mesh_core_capability::CapabilitySet;
use mesh_core_debug::ProfilingStage;
use mesh_core_diagnostics::Diagnostics;
pub use mesh_core_elements::PopoverPlacement;
use mesh_core_elements::WidgetNode;
use mesh_core_locale::LocaleEngine;
use mesh_core_render::{DamageRect, DisplayPaintCommand, PixelBuffer};
use mesh_core_scripting::ScriptError;
use mesh_core_theme::Theme;
use mesh_core_wayland::{KeyboardMode, ShellSurface};

pub type SurfaceId = String;

#[derive(Debug, Clone, PartialEq)]
pub struct ChildSurfaceRequest {
    pub node_key: String,
    pub kind: ChildSurfaceKind,
    pub anchor_rect: (i32, i32, i32, i32),
    pub content_size: (u32, u32),
    /// Extra buffer padding (left, top, right, bottom) beyond `content_size`
    /// reserved for `box-shadow`/`filter` overshoot in the popover subtree,
    /// so shadows don't clip at the popup buffer edge. All zero when the
    /// subtree has no such overshoot.
    pub content_padding: (u32, u32, u32, u32),
    pub placement: PopoverPlacement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildSurfaceKind {
    Popover,
    Overflow,
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
    HidePopover {
        surface_id: SurfaceId,
        defer_for_hover_bridge: bool,
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
    SetLocale {
        locale: String,
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
    InterfaceEvent {
        service: String,
        source_module: String,
        name: String,
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
    fn observes_service_event(&self, _event: &ServiceEvent) -> bool {
        true
    }
    fn wants_tick(&self) -> bool {
        true
    }
    /// Return the next time this component needs `tick()` for timer-driven
    /// work. The default preserves the old roughly-60Hz tick contract for
    /// components that have not opted into explicit deadlines. Implementations
    /// can return `None` when they have no pending timer.
    fn next_tick_deadline(&self) -> Option<Instant> {
        Some(Instant::now() + Duration::from_millis(16))
    }
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
        scale: f32,
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
    /// Handle input delivered to a promoted child surface using coordinates
    /// local to that child surface. Implementations that paint keyed subtrees
    /// into child surfaces can translate the local input back into their
    /// retained tree before dispatch.
    fn handle_child_surface_input(
        &mut self,
        _node_key: &str,
        theme: &Theme,
        width: u32,
        height: u32,
        input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        self.handle_input(theme, width, height, input)
    }
    /// Whether the current pointer hover target should use an interactive cursor.
    fn hovered_target_is_interactive(&self) -> bool {
        false
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
    /// Mark the surface as promoted to (or demoted from) an `xdg_popup`. While
    /// promoted, the surface is positioned by its `xdg_positioner`, so the
    /// host skips the layer-surface anchor/margin/size configuration path.
    fn set_popup_promoted(&mut self, _promoted: bool) {}
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
    /// Return the damage rects from the most recent paint for partial presentation.
    /// An empty Vec means no changed pixels — the caller should skip the present.
    fn take_present_damage(&mut self) -> Vec<DamageRect> {
        Vec::new()
    }
    /// Return the most recent parent-paint damage translated into a promoted
    /// child surface's local coordinates. The shell calls this after presenting
    /// the parent surface, so implementations that derive child surfaces from
    /// the same retained tree should keep a non-draining copy of frame damage.
    fn child_surface_present_damage(
        &self,
        _node_key: &str,
        _content_offset: (u32, u32),
        _surface_size: (u32, u32),
    ) -> Vec<DamageRect> {
        Vec::new()
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
    /// Return the retained display list paint commands from the most recent paint,
    /// for opaque region computation.
    fn display_list_paint_commands(&self) -> &[DisplayPaintCommand] {
        &[]
    }
    fn display_list_generation(&self) -> u64 {
        0
    }
    /// The interactive content size, excluding any tooltip-overlay buffer padding.
    /// Used to confine the surface's pointer input region to the real content so
    /// clicks over the padding fall through to the windows beneath. `None` leaves
    /// the input region at the whole-surface default.
    fn content_input_size(&self) -> Option<(u32, u32)> {
        None
    }
    /// Return the last widget tree built by `paint`, for the debug layout inspector.
    fn last_widget_tree(&self) -> Option<&WidgetNode> {
        None
    }
    /// Return a child-surface subtree normalized to child-local coordinates,
    /// for debug layout overlays on promoted popups.
    fn child_surface_debug_tree(&self, _node_key: &str) -> Option<WidgetNode> {
        None
    }
    /// Return child surfaces that should be auto-derived from the last painted
    /// tree. Authors still write normal inline UI; the shell uses these
    /// requests to realize escape-bounds nodes as compositor child surfaces.
    fn child_surface_requests(&self) -> Vec<ChildSurfaceRequest> {
        Vec::new()
    }
    /// Paint a keyed subtree into a child-surface buffer at local origin,
    /// offset by `content_offset` (the left/top padding reserved for
    /// shadow/filter overshoot; see `ChildSurfaceRequest::content_padding`).
    /// When `exiting` is set, the painted subtree gets the same
    /// `mesh-surface-exiting` class treatment top-level surfaces get while
    /// playing their hide transition, so a closing popover's CSS exit
    /// animation (opacity/transform) has pixels to animate before teardown.
    /// Returns `true` when the node existed and pixels were painted.
    fn paint_child_surface(
        &self,
        _node_key: &str,
        _buffer: &mut PixelBuffer,
        _scale: f32,
        _content_offset: (u32, u32),
        _exiting: bool,
    ) -> Result<bool, ComponentError> {
        Ok(false)
    }
    /// Duration in milliseconds to keep a closing child popover's surface
    /// alive so its own CSS exit transition can play, read from the popover
    /// subtree's own resolved style rather than the component root. Mirrors
    /// `hide_transition_ms` for the in-tree child-surface path.
    fn child_hide_transition_ms(&self, _node_key: &str) -> u64 {
        0
    }
    /// Tell the component which in-tree child popovers (by `_mesh_key`) are
    /// currently playing their exit transition. The component scopes
    /// `mesh-surface-exiting` to just these subtrees on its next tree build,
    /// so the popover's own CSS transition resolves and advances through the
    /// normal per-node transition engine instead of a one-shot style snap.
    fn set_closing_child_keys(&mut self, _keys: std::collections::HashSet<String>) {}
    /// Tell the component which newly opened child popovers should be painted
    /// in their authored entrance state. The shell maps the child from this
    /// paint, then clears the keys so normal CSS transitions animate it to its
    /// resting state instead of exposing the resting frame first.
    fn set_entering_child_keys(&mut self, _keys: std::collections::HashSet<String>) {}
    /// Best-known logical content size for surface sizing: the measured content
    /// size once a paint has produced one, otherwise the manifest-declared
    /// width/height. Never returns a zero/`1x1` placeholder, so popup creation
    /// can size the surface correctly on first open before any paint exists.
    fn declared_or_measured_size(&self) -> (u32, u32) {
        (0, 0)
    }
    /// True for a content-measured surface that has not yet produced a measured
    /// size from a paint. The shell uses this to defer creating a promoted
    /// popover's `xdg_popup` by one render iteration — letting the loop's first
    /// real paint measure the content — so the popup is created at its true size
    /// instead of a declared placeholder that grows on the next open.
    fn needs_content_measure(&self) -> bool {
        false
    }
    /// Bounds `(left, top, right, bottom)` of a node in this surface's last
    /// painted tree, in surface-local logical coordinates. Used to anchor a
    /// promoted popover to its real trigger rect so the compositor can center
    /// and constrain it without the component hardcoding its own width.
    fn node_bounds_by_key(&self, _key: &str) -> Option<(f32, f32, f32, f32)> {
        None
    }
    /// Override this surface's position for popover placement.
    fn apply_position(&mut self, _margin_top: i32, _margin_left: i32) {}
    /// The margin-left currently stored in the surface layout (set by the most
    /// recent `apply_position` call). Used to derive the `xdg_popup` anchor
    /// rect's x-offset at `ActivatePopover` time, before the next render frame
    /// updates the `StubSurface` via `render_layout`.
    fn popover_margin_left(&self) -> i32 {
        0
    }
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
