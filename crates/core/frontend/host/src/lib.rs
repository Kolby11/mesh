use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mesh_core_capability::CapabilitySet;
use mesh_core_debug::ProfilingStage;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_elements::WidgetNode;
pub use mesh_core_elements::{
    PopoverPlacement, PopoverPlacementDiagnostic, PopoverPlacementDiagnosticKind,
    PopoverPlacementField,
};
use mesh_core_locale::LocaleEngine;
use mesh_core_render::{DamageRect, DisplayPaintCommand, PixelBuffer};
use mesh_core_scripting::ScriptError;
use mesh_core_theme::Theme;
use mesh_core_wayland::{KeyboardMode, ShellSurface, WindowStates};

pub use mesh_core_frontend_abi::{
    DebugEffect, EffectRejection, EffectScope, EffectSource, FrontendEffect, FrontendEffectBatch,
    ScopedFrontendEffect, ServiceEffect, SurfaceEffect, SurfaceRole,
};

pub type SurfaceId = String;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceObservationSummary {
    pub update_services: Vec<String>,
    /// Retained before any live runtime observes a field, so lazily-created
    /// runtimes start from real state.
    pub cached_update_services: Vec<String>,
    pub interface_events: Vec<ServiceInterfaceEventSubscription>,
}

/// UTF-8 byte-indexed text state projected by a focused component to the
/// presentation backend. The host keeps this protocol-neutral so components
/// do not depend on a particular compositor protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInputState {
    pub surrounding_text: String,
    pub cursor: usize,
    pub anchor: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceInterfaceEventSubscription {
    pub service: String,
    pub event: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChildSurfaceRequest {
    pub node_key: String,
    pub kind: ChildSurfaceKind,
    pub anchor_rect: (i32, i32, i32, i32),
    pub content_size: (u32, u32),
    /// Padding beyond `content_size` reserved for `box-shadow`/`filter`
    /// overshoot, so shadows don't clip at the popup buffer edge.
    pub content_padding: (u32, u32, u32, u32),
    pub placement: PopoverPlacement,
    /// The authored trigger reference for an inline popover. The shell
    /// resolves this reference before promotion and carries the resulting
    /// surface relationship on the promoted target.
    pub popover_trigger: Option<PopoverTriggerReference>,
}

/// A stable authored reference to the element that opens/anchors an inline
/// popover. It intentionally remains a reference rather than a `NodeId`: the
/// shell's retained tree assigns runtime node identities after composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopoverTriggerReference {
    pub reference: String,
}

/// The cross-surface relationship created when an inline popover is promoted.
/// This is the observable seam shared by shell focus, dismissal, diagnostics,
/// and accessibility integrations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopoverSurfaceRelationship {
    pub trigger_surface_id: SurfaceId,
    pub trigger_reference: PopoverTriggerReference,
    pub popup_surface_id: SurfaceId,
    pub popup_node_key: String,
}

/// A typed child-surface validation failure attached to an authored popover
/// node. Invalid requests are not promoted, so the compositor never observes
/// a silently substituted placement or anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildSurfaceDiagnostic {
    Placement {
        node_key: String,
        diagnostic: PopoverPlacementDiagnostic,
    },
    MissingTrigger {
        node_key: String,
        reference: PopoverTriggerReference,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChildSurfaceKind {
    /// Authored `<popover>` state: placement, focus transfer, and dismissal
    /// follow the popover controller and its trigger relationship.
    Popover,
    /// Geometry-derived escape-bounds content: owned by the parent node,
    /// non-grabbing, non-focus-owning, and retried while it still escapes.
    Overflow,
    /// An embedded widget promoted into its own `xdg_toplevel` window.
    Window,
}

/// The two sizes a surface paints against, which are not the same size.
///
/// `content` is the UI's own extent: what the component lays out into, what the
/// shell records as the surface size, and what input coordinates are relative
/// to. `padded` is the buffer the compositor was actually configured with —
/// `content` plus whatever reserve the surface declared so tooltips and
/// descendant `box-shadow`/`filter` overshoot have pixels outside the content
/// box (`SurfacePadding` in `mesh-core-presentation`).
///
/// They are one argument because collapsing them into one number is how the
/// reserve leaked into layout: [`ShellComponent::paint`] used to take a single
/// extent and fall back to it whenever nothing was measured yet, so a shipped
/// bar laid its first frame out against a surface 200 logical pixels taller
/// than the bar and recorded that as its content size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceExtent {
    /// Logical size of the content itself. `0` on an axis means the shell has
    /// no size for it yet — the compositor has not configured the surface and
    /// nothing has been measured — and is NOT a request to lay out against
    /// zero. See [`SurfaceExtent::content_width_known`].
    pub content: (u32, u32),
    /// Logical size of the buffer, i.e. `content` plus its declared reserve.
    /// Never smaller than `content`.
    pub padded: (u32, u32),
    /// The content values are generous bounds for an intrinsic first layout,
    /// rather than sizes the shell is allowed to stamp onto the surface root.
    /// This is used while a promoted popup is measuring before its first
    /// `xdg_popup` configure.
    intrinsic_bound: bool,
}

impl SurfaceExtent {
    /// A surface whose buffer is exactly its content: windows, popups with no
    /// overshoot ring, the debug inspector, and any caller that never inflates.
    pub fn unpadded(width: u32, height: u32) -> Self {
        Self {
            content: (width, height),
            padded: (width, height),
            intrinsic_bound: false,
        }
    }

    /// `content` painted into a buffer of `padded`. The reserve is whatever
    /// `padded` has beyond `content`; a `padded` smaller than `content` is a
    /// caller bug and is clamped up rather than silently cropping the UI.
    pub fn padded(content: (u32, u32), padded: (u32, u32)) -> Self {
        Self {
            content,
            padded: (padded.0.max(content.0), padded.1.max(content.1)),
            intrinsic_bound: false,
        }
    }

    /// Lay out against `bound`, but keep both axes semantically unknown so an
    /// intrinsic root can measure its content instead of becoming permanently
    /// pinned to the bound. The bound is still large enough to avoid the
    /// `(1, 1)` placeholder collapsing cross-axis content on a popup's first
    /// frame.
    pub fn intrinsic(bound: (u32, u32)) -> Self {
        let bound = (bound.0.max(1), bound.1.max(1));
        Self {
            content: bound,
            padded: bound,
            intrinsic_bound: true,
        }
    }

    pub fn content_width(&self) -> u32 {
        self.content.0
    }

    pub fn content_height(&self) -> u32 {
        self.content.1
    }

    pub fn padded_width(&self) -> u32 {
        self.padded.0
    }

    pub fn padded_height(&self) -> u32 {
        self.padded.1
    }

    /// Whether the shell actually knows this axis, as opposed to having no
    /// size for it yet.
    ///
    /// An unknown axis must not become a definite layout box: a surface root
    /// pinned to a fabricated 1px collapses shrinkable content to 1px, and
    /// that collapsed measurement is what the surface then reports as its
    /// content size — permanently, since a nonzero size stops being dynamic.
    /// [`ShellComponent::paint`] lays an unknown axis out as `auto` instead,
    /// which is what content measurement means in the first place.
    pub fn content_width_known(&self) -> bool {
        self.content.0 > 0 && !self.intrinsic_bound
    }

    pub fn content_height_known(&self) -> bool {
        self.content.1 > 0 && !self.intrinsic_bound
    }

    /// Whether any of this surface is paint-only reserve.
    pub fn has_reserve(&self) -> bool {
        self.padded != self.content
    }
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
        /// Linux input-event code, for example `BTN_LEFT` (`0x110`).
        button: u32,
        pressed: bool,
    },
    Scroll {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    TwoFingerScroll {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    GestureSwipeBegin {
        fingers: u32,
    },
    GestureSwipeUpdate {
        dx: f32,
        dy: f32,
    },
    GestureSwipeEnd {
        cancelled: bool,
    },
    GesturePinchBegin {
        fingers: u32,
    },
    GesturePinchUpdate {
        dx: f32,
        dy: f32,
        scale: f32,
        rotation: f32,
    },
    GesturePinchEnd {
        cancelled: bool,
    },
    GestureHoldBegin {
        fingers: u32,
    },
    GestureHoldEnd {
        cancelled: bool,
    },
    TouchDown {
        id: i32,
        x: f32,
        y: f32,
    },
    TouchMove {
        id: i32,
        x: f32,
        y: f32,
    },
    TouchUp {
        id: i32,
    },
    TouchCancel,
    /// A committed text payload from the compositor or an input method.
    ///
    /// Unlike [`Self::Char`], this preserves the payload's commit boundary
    /// and can carry multiple Unicode scalars (for example a composed input
    /// sequence) without making the shell dispatch several partial values.
    TextInput {
        text: String,
    },
    /// Delete UTF-8 bytes around the focused input cursor.
    ///
    /// Text-input protocols report these lengths in bytes. The component
    /// runtime clamps them to Unicode scalar boundaries before mutating the
    /// value, so malformed or stale protocol payloads cannot split UTF-8.
    TextDelete {
        before_bytes: usize,
        after_bytes: usize,
    },
    /// One atomic text-input-v3 transaction. Preedit is optional because a
    /// compositor may send only a commit or deletion in a done group.
    TextInputEdit {
        preedit_present: bool,
        preedit: Option<String>,
        preedit_cursor_begin: i32,
        preedit_cursor_end: i32,
        commit: Option<String>,
        before_bytes: usize,
        after_bytes: usize,
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
    /// Move a live surface between shell chrome and an ordinary window. The
    /// component VM, retained tree, Lua state, and subscriptions all survive;
    /// only the compositor object is swapped.
    SetSurfaceRole {
        surface_id: SurfaceId,
        role: mesh_core_wayland::SurfaceRole,
    },
    /// Separate from [`Self::SetSurfaceRole`] because the current role lives in
    /// the shell, so a component cannot compute the target itself.
    ToggleSurfaceRole {
        surface_id: SurfaceId,
    },
    /// Move one embedded widget between its parent's retained tree and an
    /// independent `xdg_toplevel`. The owning surface and shared component VM
    /// remain alive in both states.
    SetChildSurfaceRole {
        surface_id: SurfaceId,
        node_key: String,
        role: mesh_core_wayland::SurfaceRole,
    },
    /// Reposition below a trigger element, anchored top-left.
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
    /// A Luau service-proxy invocation whose terminal result must be routed
    /// back to the originating context's ticket.
    ServiceCall {
        interface: String,
        command: String,
        payload: serde_json::Value,
        call_id: u64,
        source_instance_id: String,
        source_module_id: String,
        source_capabilities: CapabilitySet,
    },
    /// Caller-driven cancellation for a previously published service call.
    CancelServiceCall {
        interface: String,
        call_id: u64,
        source_instance_id: String,
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
    SetIconTheme {
        theme_id: String,
    },
    SetFontFamily {
        family: String,
    },
    SetProvider {
        interface: String,
        provider_id: String,
    },
    SetModuleEnabled {
        module_id: String,
        enabled: bool,
    },
    /// Install a module from a local directory or Git source, then make it
    /// available to the installed graph. Frontends are added to the selected
    /// profile unless `available_only` is set.
    InstallModule {
        source: String,
        profile_id: Option<String>,
        available_only: bool,
        allow_elevated: bool,
        allow_high: bool,
    },
    /// Remove an installed module and its lock entry. Without `force`, active
    /// profile references and dependent lock entries refuse the operation.
    UninstallModule {
        module_id: String,
        force: bool,
    },
    /// `instance_id = None` targets `props.global`; otherwise
    /// `props.instances.<instance-id>`.
    SetModuleProp {
        module_id: String,
        instance_id: Option<String>,
        prop: String,
        value: serde_json::Value,
    },
    /// Remove an override so the declaration default applies again.
    UnsetModuleProp {
        module_id: String,
        instance_id: Option<String>,
        prop: String,
    },
    /// Atomically replace one author-declared customizable slot list.
    ApplyNodeSlot {
        profile_id: String,
        root_instance: String,
        slot: String,
        nodes: serde_json::Value,
        expected_generation: String,
    },
    /// Remove the sparse override so composition/author defaults apply.
    ResetNodeSlot {
        profile_id: String,
        root_instance: String,
        slot: String,
        expected_generation: String,
    },
    /// Transactional: candidates are prepared before the active pointer and
    /// visible roots change.
    SwitchProfile {
        profile_id: String,
    },
    /// The shell records `(trigger_surface, trigger_key)` so Tab from that key
    /// transfers focus into the popover.
    ActivatePopover {
        surface_id: SurfaceId,
        trigger_surface: SurfaceId,
        trigger_key: String,
        /// Insert into the focus chain immediately, with the trigger as the
        /// return target.
        focus: bool,
    },
    TransferTabFocus {
        from_surface: SurfaceId,
        to_surface: SurfaceId,
        target: TabFocusTarget,
        /// Where the popover sends focus back on exit.
        return_target: Option<(SurfaceId, String)>,
        /// Hide the popover when Tab or Shift+Tab leaves its chain.
        target_closes_on_leave: bool,
        /// Surface to hide as part of this transfer.
        close_source: Option<SurfaceId>,
    },
    ToggleDebugOverlay,
    ToggleDebugLayoutBounds,
    ToggleDebugElementPicker,
    OpenDebugSource {
        path: String,
        line: u32,
    },
    ToggleDebugProfiling,
    RunDebugBenchmark {
        scenario_id: String,
    },
    CycleDebugTab,
    Shutdown,
}

/// Shell-side lowering for renderer-neutral frontend effects.
///
/// Frontend components and runtime integrations can exchange
/// [`ScopedFrontendEffect`] without importing Wayland, render buffers, or
/// shell storage. This adapter is the only place where those effects become
/// the legacy shell request vocabulary.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShellEffectAdapter;

#[derive(Debug, thiserror::Error)]
pub enum ShellEffectError {
    #[error("frontend effect rejected: {0}")]
    Rejected(#[from] EffectRejection),
}

impl ShellEffectAdapter {
    pub fn lower(effect: ScopedFrontendEffect) -> Result<CoreRequest, ShellEffectError> {
        effect.authorize()?;
        let ScopedFrontendEffect { scope, effect } = effect;
        let source_module_id = scope.source().module_id.clone();
        let source_capabilities = scope.capabilities().clone();
        Ok(match effect {
            FrontendEffect::Surface(effect) => match effect {
                SurfaceEffect::Toggle { surface_id } => CoreRequest::ToggleSurface { surface_id },
                SurfaceEffect::Show { surface_id } => CoreRequest::ShowSurface { surface_id },
                SurfaceEffect::Hide { surface_id } => CoreRequest::HideSurface { surface_id },
                SurfaceEffect::HidePopover {
                    surface_id,
                    defer_for_hover_bridge,
                } => CoreRequest::HidePopover {
                    surface_id,
                    defer_for_hover_bridge,
                },
                SurfaceEffect::SetRole { surface_id, role } => CoreRequest::SetSurfaceRole {
                    surface_id,
                    role: match role {
                        SurfaceRole::Layer => mesh_core_wayland::SurfaceRole::Layer,
                        SurfaceRole::Window => mesh_core_wayland::SurfaceRole::Window,
                    },
                },
                SurfaceEffect::ToggleRole { surface_id } => {
                    CoreRequest::ToggleSurfaceRole { surface_id }
                }
                SurfaceEffect::SetChildRole {
                    surface_id,
                    node_key,
                    role,
                } => CoreRequest::SetChildSurfaceRole {
                    surface_id,
                    node_key,
                    role: match role {
                        SurfaceRole::Layer => mesh_core_wayland::SurfaceRole::Layer,
                        SurfaceRole::Window => mesh_core_wayland::SurfaceRole::Window,
                    },
                },
                SurfaceEffect::Position {
                    surface_id,
                    margin_top,
                    margin_left,
                } => CoreRequest::PositionSurface {
                    surface_id,
                    margin_top,
                    margin_left,
                },
                SurfaceEffect::ActivatePopover {
                    surface_id,
                    trigger_surface,
                    trigger_key,
                    focus,
                } => CoreRequest::ActivatePopover {
                    surface_id,
                    trigger_surface,
                    trigger_key,
                    focus,
                },
            },
            FrontendEffect::Service(effect) => match effect {
                ServiceEffect::Command {
                    interface,
                    command,
                    payload,
                } => CoreRequest::ServiceCommand {
                    interface,
                    command,
                    payload,
                    source_module_id,
                    source_capabilities,
                },
                ServiceEffect::Call {
                    interface,
                    command,
                    payload,
                    call_id,
                    instance_id,
                } => CoreRequest::ServiceCall {
                    interface,
                    command,
                    payload,
                    call_id,
                    source_instance_id: instance_id,
                    source_module_id,
                    source_capabilities,
                },
                ServiceEffect::Cancel {
                    interface,
                    call_id,
                    instance_id,
                } => CoreRequest::CancelServiceCall {
                    interface,
                    call_id,
                    source_instance_id: instance_id,
                    source_module_id,
                    source_capabilities,
                },
            },
            FrontendEffect::SetLocale { locale } => CoreRequest::SetLocale { locale },
            FrontendEffect::WriteClipboard { text } => CoreRequest::WriteClipboard { text },
            FrontendEffect::Debug(effect) => match effect {
                DebugEffect::ToggleOverlay => CoreRequest::ToggleDebugOverlay,
                DebugEffect::ToggleLayoutBounds => CoreRequest::ToggleDebugLayoutBounds,
                DebugEffect::ToggleElementPicker => CoreRequest::ToggleDebugElementPicker,
                DebugEffect::OpenSource { path, line } => {
                    CoreRequest::OpenDebugSource { path, line }
                }
                DebugEffect::ToggleProfiling => CoreRequest::ToggleDebugProfiling,
                DebugEffect::RunBenchmark { scenario_id } => {
                    CoreRequest::RunDebugBenchmark { scenario_id }
                }
            },
        })
    }

    pub fn lower_batch(batch: FrontendEffectBatch) -> Result<Vec<CoreRequest>, ShellEffectError> {
        batch.into_scoped().map(Self::lower).collect()
    }
}

#[cfg(test)]
mod effect_adapter_tests {
    use super::*;
    use mesh_core_capability::Capability;
    use serde_json::json;

    fn scope(capability: &str) -> EffectScope {
        let mut capabilities = mesh_core_capability::CapabilitySet::new();
        capabilities.grant(Capability::new(capability));
        EffectScope::new(
            EffectSource::new("@mesh/test", Some("instance".into())),
            capabilities,
        )
    }

    #[test]
    fn adapter_rejects_effects_without_their_capability_scope() {
        let effect = ScopedFrontendEffect::new(
            EffectScope::new(
                EffectSource::new("@mesh/test", Some("instance".into())),
                mesh_core_capability::CapabilitySet::new(),
            ),
            FrontendEffect::Service(ServiceEffect::Command {
                interface: "mesh.audio".into(),
                command: "set_volume".into(),
                payload: json!({ "percent": 50 }),
            }),
        );

        assert!(matches!(
            ShellEffectAdapter::lower(effect),
            Err(ShellEffectError::Rejected(
                EffectRejection::MissingCapability { .. }
            ))
        ));
    }

    #[test]
    fn adapter_lowers_service_effects_with_source_identity() {
        let effect = ScopedFrontendEffect::new(
            scope("service.audio.control"),
            FrontendEffect::Service(ServiceEffect::Call {
                interface: "mesh.audio".into(),
                command: "set_volume".into(),
                payload: json!({ "percent": 50 }),
                call_id: 7,
                instance_id: "instance".into(),
            }),
        );

        assert!(matches!(
            ShellEffectAdapter::lower(effect),
            Ok(CoreRequest::ServiceCall {
                interface,
                command,
                call_id: 7,
                source_instance_id,
                source_module_id,
                source_capabilities,
                ..
            }) if interface == "mesh.audio"
                && command == "set_volume"
                && source_instance_id == "instance"
                && source_module_id == "@mesh/test"
                && source_capabilities.is_granted(&Capability::new("service.audio.control"))
        ));
    }
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
        snapshot: mesh_core_theme::ThemeSnapshot,
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
    /// Deliver a terminal correlated service result to the matching frontend
    /// instance. Returns true when this component owns the instance.
    fn deliver_service_call_result(
        &mut self,
        _instance_id: &str,
        _call_id: u64,
        _status: &str,
        _result: &serde_json::Value,
    ) -> bool {
        false
    }
    fn observes_service_event(&self, _event: &ServiceEvent) -> bool {
        true
    }
    /// Record a payload for a declared-but-unread service without the full
    /// update path. A surface with no tree yet reads no field, so it is not a
    /// delivery target, but the payload must still seed its runtime later.
    fn cache_service_payload(&mut self, _event: &ServiceEvent) {}
    fn service_observation_summary(&self) -> Option<ServiceObservationSummary> {
        None
    }
    fn wants_tick(&self) -> bool {
        true
    }
    /// Next `tick()` deadline, or `None` with no pending timer. The default
    /// keeps the roughly-60Hz contract for components that never opted in.
    fn next_tick_deadline(&self) -> Option<Instant> {
        Some(Instant::now() + Duration::from_millis(16))
    }
    fn tick(&mut self) -> Result<Vec<CoreRequest>, ComponentError>;
    fn wants_render(&self) -> bool;
    /// A paint-only frame, implying no script, layout, or style change.
    fn request_paint(&mut self) {}
    fn surface_size_changed(&mut self, _width: u32, _height: u32) -> bool {
        false
    }
    /// Returns whether a restyle is owed. Only window surfaces ever see
    /// anything but the default.
    fn surface_window_states_changed(&mut self, _states: WindowStates) -> bool {
        false
    }
    /// Returns whether a restyle is owed. The role is a CSS state
    /// (`:windowed`) and inverts sizing, so both directions invalidate.
    fn surface_role_changed(&mut self, _role: mesh_core_wayland::SurfaceRole) -> bool {
        false
    }
    /// The role this component's surface is currently realized under.
    fn surface_role(&self) -> mesh_core_wayland::SurfaceRole {
        mesh_core_wayland::SurfaceRole::Layer
    }
    /// Whether [`CoreRequest::SetSurfaceRole`] applies to this surface.
    fn surface_promotable(&self) -> bool {
        false
    }
    /// Promote or demote an embedded widget identified by its retained node
    /// key. The default is a no-op for non-frontend components.
    fn set_child_surface_promoted(&mut self, _node_key: &str, _promoted: bool) -> bool {
        false
    }
    fn render(&mut self, surface: &mut dyn ShellSurface) -> Result<(), ComponentError>;
    /// Paint one frame. `extent` carries the content size and the padded buffer
    /// size separately — see [`SurfaceExtent`]; laying out against the padded
    /// size is the bug that type exists to prevent.
    fn paint(
        &mut self,
        theme: &Theme,
        extent: SurfaceExtent,
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
    /// Input in child-surface-local coordinates. `content_offset` is where the
    /// subtree's box starts inside the padded child buffer and must match the
    /// offset it was painted at, or hit testing is skewed by the padding.
    fn handle_child_surface_input(
        &mut self,
        _node_key: &str,
        theme: &Theme,
        width: u32,
        height: u32,
        _content_offset: (f32, f32),
        input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        self.handle_input(theme, width, height, input)
    }
    /// Return the focused editable field's UTF-8 byte-indexed surrounding
    /// text, or `None` when this component has no focused input.
    fn text_input_state(&self) -> Option<TextInputState> {
        None
    }
    /// Whether the current pointer hover target should use an interactive cursor.
    fn hovered_target_is_interactive(&self) -> bool {
        false
    }
    /// Cross-surface Tab transfer; a no-op for non-frontend components.
    fn receive_focus_transfer(
        &mut self,
        _target: &TabFocusTarget,
        _return_focus: Option<(SurfaceId, String)>,
        _close_on_focus_leave: bool,
    ) {
    }
    /// Drop focus state on a component that just transferred focus away.
    fn release_focus_for_transfer(&mut self) {}
    /// Record that `trigger_key` activated the popover at `popover_surface`.
    fn register_popover_trigger(&mut self, _trigger_key: String, _popover_surface: SurfaceId) {}
    /// Drop a previously-registered popover trigger when the popover hides.
    fn unregister_popover_trigger(&mut self, _popover_surface: &str) {}
    /// Override the surface's effective keyboard_mode at runtime.
    fn set_keyboard_mode_override(&mut self, _mode: Option<KeyboardMode>) {}
    /// While promoted, the `xdg_positioner` places the surface, so the host
    /// skips the layer-surface anchor/margin/size path.
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
    /// Damage from the most recent paint; empty means skip the present.
    fn take_present_damage(&mut self) -> Vec<DamageRect> {
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
    /// Adopt a newly published frontend-module catalog generation.
    ///
    /// Components that do not compose installable frontend modules can ignore
    /// this notification. Returns whether the component invalidated render
    /// state in response.
    fn frontend_catalog_changed(&mut self) -> bool {
        false
    }
    /// Adopt a freshly loaded settings store.
    ///
    /// Every user decision lives in one file, so the shell reloads it once and
    /// hands the same store to every component rather than each component
    /// stat-ing and parsing its own file. Returns whether anything this
    /// component actually reads changed.
    fn apply_settings(
        &mut self,
        _settings: &Arc<mesh_core_config::SettingsStore>,
    ) -> Result<bool, ComponentError> {
        Ok(false)
    }
    /// Return the retained display list paint commands from the most recent paint,
    /// for opaque region computation.
    fn display_list_paint_commands(&self) -> &[DisplayPaintCommand] {
        &[]
    }
    /// Stable compositor blur regions for the parent surface, derived from the
    /// full widget tree (not the scoped paint-command selection) so partial
    /// retained repaints never collapse them to a whole-surface blur.
    fn display_list_blur_regions(&self) -> &[DamageRect] {
        &[]
    }
    fn display_list_generation(&self) -> u64 {
        0
    }
    /// Authoritative paint generation for a promoted child subtree. Returning
    /// `None` keeps conservative eager child repainting.
    fn child_surface_paint_generation(&self, _node_key: &str) -> Option<u64> {
        None
    }
    /// Return logical child-local damage produced by the most recent
    /// `paint_child_surface` call. `None` keeps the conservative full-surface
    /// present used by components without retained child damage tracking.
    fn child_surface_present_damage(&self, _node_key: &str) -> Option<Vec<DamageRect>> {
        None
    }
    /// Regions of the child subtree's nodes with an active
    /// `backdrop-filter`, in child-local logical coordinates (including the
    /// content padding offset passed to `paint_child_surface`). Drives the
    /// compositor blur region (org_kde_kwin_blur) for promoted popups, the
    /// same way the parent surface's display list drives its blur region.
    fn child_surface_blur_regions(&self, _node_key: &str) -> Vec<DamageRect> {
        Vec::new()
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
    /// for debug layout overlays on promoted popups. `content_offset` must be
    /// the same `(pad_left, pad_top)` passed to `paint_child_surface` for
    /// this child, or the debug boxes land at a constant offset from the
    /// real (padding-shifted) rendered content.
    fn child_surface_debug_tree(
        &self,
        _node_key: &str,
        _content_offset: (f32, f32),
    ) -> Option<WidgetNode> {
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
    /// Borrowed variant for hot reconciliation paths. Implementations can
    /// compare against their existing state before allocating an owned set.
    fn set_closing_child_keys_from_slice(&mut self, keys: &[&str]) {
        self.set_closing_child_keys(keys.iter().map(|key| (*key).to_owned()).collect());
    }
    /// Tell the component which newly opened child popovers should be painted
    /// in their authored entrance state. The shell maps the child from this
    /// paint, then clears the keys so normal CSS transitions animate it to its
    /// resting state instead of exposing the resting frame first.
    fn set_entering_child_keys(&mut self, _keys: std::collections::HashSet<String>) {}
    /// Borrowed variant for hot reconciliation paths. Implementations can
    /// compare against their existing state before allocating an owned set.
    fn set_entering_child_keys_from_slice(&mut self, keys: &[&str]) {
        self.set_entering_child_keys(keys.iter().map(|key| (*key).to_owned()).collect());
    }
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
    /// Mark the surface config (size, anchor, margins, keyboard mode) as needing
    /// to be re-emitted on the next `render`.
    ///
    /// `render` refreshes the shell's surface record only while that config is
    /// dirty, which is what keeps a quiet component from re-deriving its
    /// placement every frame. The shell calls this when it takes a second render
    /// pass specifically to correct a configure — the paint in between has just
    /// produced the measurement the first pass lacked, and without this the
    /// corrective pass would recompute the configure from the same unmeasured
    /// dimensions and send them.
    fn invalidate_surface_config(&mut self) {}
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
