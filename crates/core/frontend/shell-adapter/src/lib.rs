use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mesh_core_capability::CapabilitySet;
use mesh_core_debug::{ProfilingInvalidationSnapshot, ProfilingStage};
use mesh_core_diagnostics::{DiagnosticIssue, Diagnostics};
use mesh_core_elements::{FrameSnapshot, WidgetNode};
pub use mesh_core_elements::{
    PopoverPlacement, PopoverPlacementDiagnostic, PopoverPlacementDiagnosticKind,
    PopoverPlacementField,
};
use mesh_core_locale::LocaleEngine;
use mesh_core_render::{DamageRect, DisplayPaintCommand, PixelBuffer};
use mesh_core_scripting::ScriptError;
use mesh_core_theme::Theme;
use mesh_core_wayland::{KeyboardMode, ShellSurface, WindowStates};

pub use mesh_core_frontend_host::{
    DebugEffect, EffectRejection, EffectScope, EffectSource, FrontendEffect, FrontendEffectBatch,
    FrontendEffectRevision, ScopedFrontendEffect, ServiceEffect, SurfaceEffect, SurfaceRole,
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

/// The revisions that must agree for one frontend frame to be consumed as a
/// coherent snapshot. `frame` identifies the publication, while the other
/// revisions identify the inputs that produced it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrontendFrameRevisions {
    pub frame: u64,
    pub catalog: u64,
    pub runtime: u64,
    pub tree: u64,
    pub services: u64,
}

/// Singular compatibility name for callers that treat the revision tuple as
/// the frame's revision token.
pub type FrontendFrameRevision = FrontendFrameRevisions;

impl FrontendFrameRevisions {
    pub const fn new(frame: u64, catalog: u64, runtime: u64, tree: u64, services: u64) -> Self {
        Self {
            frame,
            catalog,
            runtime,
            tree,
            services,
        }
    }

    pub const fn effect_revision(&self) -> FrontendEffectRevision {
        FrontendEffectRevision::new(self.catalog, self.runtime)
    }
}

/// Service information that is safe to publish with a frontend frame.
/// Payloads remain in capability-filtered runtime state; the frame carries
/// only the observation/index data needed by the shell and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendServiceSnapshot {
    revision: u64,
    observations: ServiceObservationSummary,
}

impl FrontendServiceSnapshot {
    pub fn new(revision: u64, observations: ServiceObservationSummary) -> Self {
        Self {
            revision,
            observations,
        }
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn observations(&self) -> &ServiceObservationSummary {
        &self.observations
    }
}

/// Invalidation output associated with the frame that was just published.
/// The profiling detail is optional because a component may be invalidated
/// before it has completed a paint pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontendInvalidation {
    revision: u64,
    profiling: Option<ProfilingInvalidationSnapshot>,
}

impl FrontendInvalidation {
    pub fn new(revision: u64, profiling: Option<ProfilingInvalidationSnapshot>) -> Self {
        Self {
            revision,
            profiling,
        }
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn profiling(&self) -> Option<&ProfilingInvalidationSnapshot> {
        self.profiling.as_ref()
    }
}

/// Paint metadata that remains renderer-neutral while allowing a shell
/// adapter to correlate damage and retained-display-list work with the same
/// tree/catalog/runtime snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontendPaintMetadata {
    display_list_generation: u64,
    present_damage: Vec<DamageRect>,
}

impl FrontendPaintMetadata {
    pub fn new(display_list_generation: u64, present_damage: Vec<DamageRect>) -> Self {
        Self {
            display_list_generation,
            present_damage,
        }
    }

    pub const fn display_list_generation(&self) -> u64 {
        self.display_list_generation
    }

    pub fn present_damage(&self) -> &[DamageRect] {
        &self.present_damage
    }
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
    SetThemeMode {
        mode: String,
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

/// Effects produced while preparing one frontend frame. Host requests are
/// retained for the existing shell adapter, while typed effects provide the
/// renderer-neutral ABI for callers that already use `mesh-core-frontend-abi`.
#[derive(Debug, Clone, Default)]
pub struct FrontendFrameEffects {
    host_requests: Vec<CoreRequest>,
    host_request_revisions: Vec<Option<FrontendEffectRevision>>,
    typed_effects: Vec<ScopedFrontendEffect>,
}

impl FrontendFrameEffects {
    pub fn from_host_requests(requests: Vec<CoreRequest>) -> Self {
        let request_count = requests.len();
        Self {
            host_requests: requests,
            host_request_revisions: vec![None; request_count],
            typed_effects: Vec::new(),
        }
    }

    pub fn from_host_requests_at(
        requests: Vec<CoreRequest>,
        revision: FrontendEffectRevision,
    ) -> Self {
        Self {
            host_request_revisions: vec![Some(revision); requests.len()],
            host_requests: requests,
            typed_effects: Vec::new(),
        }
    }

    pub fn from_typed_effects(batch: FrontendEffectBatch) -> Self {
        Self {
            host_requests: Vec::new(),
            host_request_revisions: Vec::new(),
            typed_effects: batch.into_scoped().collect(),
        }
    }

    pub fn extend_host_requests<I>(&mut self, requests: I)
    where
        I: IntoIterator<Item = CoreRequest>,
    {
        self.host_requests.extend(requests);
        self.host_request_revisions.extend(
            std::iter::repeat(None)
                .take(self.host_requests.len() - self.host_request_revisions.len()),
        );
    }

    pub fn extend_host_requests_at<I>(&mut self, requests: I, revision: FrontendEffectRevision)
    where
        I: IntoIterator<Item = CoreRequest>,
    {
        let requests = requests.into_iter().collect::<Vec<_>>();
        self.host_requests.extend(requests.iter().cloned());
        self.host_request_revisions
            .extend(std::iter::repeat(Some(revision)).take(requests.len()));
    }

    pub fn extend_typed_effects<I>(&mut self, effects: I)
    where
        I: IntoIterator<Item = ScopedFrontendEffect>,
    {
        self.typed_effects.extend(effects);
    }

    /// Stamp typed effects with the catalog/runtime revision that produced the
    /// frame. An already stamped effect must agree; silently retagging it would
    /// turn an obsolete capability-bearing request into a current one.
    pub fn bind_revision(
        &mut self,
        revision: FrontendEffectRevision,
    ) -> Result<(), EffectRejection> {
        for actual in self.host_request_revisions.iter().flatten() {
            if *actual != revision {
                return Err(EffectRejection::StaleRevision {
                    effect: "host-request".to_owned(),
                    expected_catalog: revision.catalog(),
                    expected_runtime: revision.runtime(),
                    actual_catalog: actual.catalog(),
                    actual_runtime: actual.runtime(),
                });
            }
        }
        self.host_request_revisions.fill(Some(revision));
        for effect in &self.typed_effects {
            if let Some(actual) = effect.revision()
                && actual != revision
            {
                return Err(EffectRejection::StaleRevision {
                    effect: effect.effect.kind().to_owned(),
                    expected_catalog: revision.catalog(),
                    expected_runtime: revision.runtime(),
                    actual_catalog: actual.catalog(),
                    actual_runtime: actual.runtime(),
                });
            }
        }
        self.typed_effects = self
            .typed_effects
            .drain(..)
            .map(|effect| effect.with_revision(revision))
            .collect();
        Ok(())
    }

    pub fn host_requests(&self) -> &[CoreRequest] {
        &self.host_requests
    }

    pub fn host_request_revisions(&self) -> &[Option<FrontendEffectRevision>] {
        &self.host_request_revisions
    }

    pub fn typed_effects(&self) -> &[ScopedFrontendEffect] {
        &self.typed_effects
    }

    pub fn is_empty(&self) -> bool {
        self.host_requests.is_empty() && self.typed_effects.is_empty()
    }
}

/// The immutable frontend-to-shell publication boundary for one completed
/// frontend evaluation. A frame owns one tree snapshot and the exact catalog,
/// runtime, service, invalidation, diagnostic, paint, and effect metadata that
/// was current when that tree was published.
#[derive(Debug, Clone)]
pub struct FrontendFrame {
    source: EffectSource,
    revisions: FrontendFrameRevisions,
    tree: Option<FrameSnapshot>,
    services: FrontendServiceSnapshot,
    invalidation: FrontendInvalidation,
    diagnostics: Vec<DiagnosticIssue>,
    effects: FrontendFrameEffects,
    child_surface_requests: Vec<ChildSurfaceRequest>,
    paint: FrontendPaintMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrontendFrameError {
    #[error("frontend tree revision {actual} does not match frame tree revision {expected}")]
    TreeRevisionMismatch { expected: u64, actual: u64 },
    #[error("frontend service revision {actual} does not match frame service revision {expected}")]
    ServiceRevisionMismatch { expected: u64, actual: u64 },
    #[error("frontend effect revision does not match frame revision: {message}")]
    EffectRevisionMismatch { message: String },
}

impl FrontendFrame {
    /// Construct a frame after all source, runtime, tree, and side-channel
    /// work has completed. The fallible constructor keeps accidental mixing of
    /// independently revised trees and service snapshots visible to callers.
    pub fn try_new(
        source: EffectSource,
        revisions: FrontendFrameRevisions,
        tree: Option<FrameSnapshot>,
        services: FrontendServiceSnapshot,
        invalidation: FrontendInvalidation,
        diagnostics: Vec<DiagnosticIssue>,
        mut effects: FrontendFrameEffects,
        child_surface_requests: Vec<ChildSurfaceRequest>,
        paint: FrontendPaintMetadata,
    ) -> Result<Self, FrontendFrameError> {
        if let Some(tree) = &tree
            && tree.revision() != revisions.tree
        {
            return Err(FrontendFrameError::TreeRevisionMismatch {
                expected: revisions.tree,
                actual: tree.revision(),
            });
        }
        if services.revision() != revisions.services {
            return Err(FrontendFrameError::ServiceRevisionMismatch {
                expected: revisions.services,
                actual: services.revision(),
            });
        }
        effects
            .bind_revision(revisions.effect_revision())
            .map_err(|error| FrontendFrameError::EffectRevisionMismatch {
                message: error.to_string(),
            })?;
        Ok(Self {
            source,
            revisions,
            tree,
            services,
            invalidation,
            diagnostics,
            effects,
            child_surface_requests,
            paint,
        })
    }

    /// Infallible convenience for trusted host publication code. Untrusted or
    /// independently assembled snapshots should use [`Self::try_new`].
    pub fn new(
        source: EffectSource,
        revisions: FrontendFrameRevisions,
        tree: Option<FrameSnapshot>,
        services: FrontendServiceSnapshot,
        invalidation: FrontendInvalidation,
        diagnostics: Vec<DiagnosticIssue>,
        effects: FrontendFrameEffects,
        child_surface_requests: Vec<ChildSurfaceRequest>,
        paint: FrontendPaintMetadata,
    ) -> Self {
        Self::try_new(
            source,
            revisions,
            tree,
            services,
            invalidation,
            diagnostics,
            effects,
            child_surface_requests,
            paint,
        )
        .expect("frontend frame revisions must agree")
    }

    pub fn source(&self) -> &EffectSource {
        &self.source
    }

    pub const fn revisions(&self) -> FrontendFrameRevisions {
        self.revisions
    }

    pub const fn revision(&self) -> u64 {
        self.revisions.frame
    }

    pub const fn catalog_revision(&self) -> u64 {
        self.revisions.catalog
    }

    pub const fn runtime_revision(&self) -> u64 {
        self.revisions.runtime
    }

    pub const fn tree_revision(&self) -> u64 {
        self.revisions.tree
    }

    pub const fn service_revision(&self) -> u64 {
        self.revisions.services
    }

    pub fn tree(&self) -> Option<&FrameSnapshot> {
        self.tree.as_ref()
    }

    pub fn services(&self) -> &FrontendServiceSnapshot {
        &self.services
    }

    pub fn invalidation(&self) -> &FrontendInvalidation {
        &self.invalidation
    }

    pub fn diagnostics(&self) -> &[DiagnosticIssue] {
        &self.diagnostics
    }

    pub fn effects(&self) -> &FrontendFrameEffects {
        &self.effects
    }

    pub fn child_surface_requests(&self) -> &[ChildSurfaceRequest] {
        &self.child_surface_requests
    }

    pub fn paint(&self) -> &FrontendPaintMetadata {
        &self.paint
    }
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
        let revision = effect.revision().ok_or_else(|| {
            ShellEffectError::Rejected(EffectRejection::MissingRevision {
                effect: effect.effect.kind().to_owned(),
            })
        })?;
        Self::lower_at(effect, revision)
    }

    /// Compatibility escape hatch for legacy event fixtures that are not tied
    /// to a live frontend frame. Runtime/frontend publication must use
    /// [`Self::lower_at`] or [`Self::lower_frame`].
    pub fn lower_unchecked(effect: ScopedFrontendEffect) -> Result<CoreRequest, ShellEffectError> {
        effect.authorize()?;
        Self::lower_authorized(effect)
    }

    /// Lower an effect only when it was produced against the current catalog
    /// and runtime revisions. This is the required path for frame effects.
    pub fn lower_at(
        effect: ScopedFrontendEffect,
        revision: FrontendEffectRevision,
    ) -> Result<CoreRequest, ShellEffectError> {
        effect.authorize_at(revision)?;
        Self::lower_authorized(effect)
    }

    /// Lower all typed effects from a frame after checking the frame itself and
    /// every effect against the adapter's current revisions.
    pub fn lower_frame(
        frame: &FrontendFrame,
        revision: FrontendEffectRevision,
    ) -> Result<Vec<CoreRequest>, ShellEffectError> {
        let frame_revision =
            FrontendEffectRevision::new(frame.catalog_revision(), frame.runtime_revision());
        if frame_revision != revision {
            return Err(ShellEffectError::Rejected(EffectRejection::StaleRevision {
                effect: "frontend-frame".to_owned(),
                expected_catalog: revision.catalog(),
                expected_runtime: revision.runtime(),
                actual_catalog: frame_revision.catalog(),
                actual_runtime: frame_revision.runtime(),
            }));
        }
        for actual in frame.effects().host_request_revisions().iter().flatten() {
            if *actual != revision {
                return Err(ShellEffectError::Rejected(EffectRejection::StaleRevision {
                    effect: "host-request".to_owned(),
                    expected_catalog: revision.catalog(),
                    expected_runtime: revision.runtime(),
                    actual_catalog: actual.catalog(),
                    actual_runtime: actual.runtime(),
                }));
            }
        }
        let mut requests = frame.effects().host_requests().to_vec();
        requests.extend(
            frame
                .effects()
                .typed_effects()
                .iter()
                .cloned()
                .map(|effect| Self::lower_at(effect, revision))
                .collect::<Result<Vec<_>, _>>()?,
        );
        Ok(requests)
    }

    fn lower_authorized(effect: ScopedFrontendEffect) -> Result<CoreRequest, ShellEffectError> {
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

impl mesh_core_frontend_host::FrontendEffectSink for ShellEffectAdapter {
    type Request = CoreRequest;
    type Error = ShellEffectError;

    fn publish(&mut self, effect: ScopedFrontendEffect) -> Result<Self::Request, Self::Error> {
        Self::lower(effect)
    }
}

#[cfg(test)]
mod effect_adapter_tests {
    use super::*;
    use mesh_core_capability::Capability;
    use serde_json::json;

    fn scope(capability: &str) -> EffectScope {
        let capabilities = mesh_core_capability::CapabilitySet::from_ids([capability]);
        EffectScope::new(
            EffectSource::new("@mesh/test", Some("instance".into())),
            capabilities,
        )
        .with_revision(FrontendEffectRevision::new(4, 9))
    }

    #[test]
    fn adapter_rejects_effects_without_their_capability_scope() {
        let effect = ScopedFrontendEffect::new(
            EffectScope::new(
                EffectSource::new("@mesh/test", Some("instance".into())),
                mesh_core_capability::CapabilitySet::default(),
            )
            .with_revision(FrontendEffectRevision::new(4, 9)),
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

#[cfg(test)]
mod frontend_frame_tests {
    use super::*;
    use mesh_core_capability::Capability;
    use mesh_core_elements::WidgetNode;

    fn effect_scope(capability: &str) -> EffectScope {
        let capabilities = mesh_core_capability::CapabilitySet::from_ids([capability]);
        EffectScope::new(
            EffectSource::new("@mesh/test", Some("surface".into())),
            capabilities,
        )
    }

    fn frame_tree(revision: u64) -> FrameSnapshot {
        let root = WidgetNode::new("box");
        FrameSnapshot::complete(&root, revision, None).expect("valid frame tree")
    }

    #[test]
    fn frontend_frame_keeps_all_publication_inputs_at_one_revision_boundary() {
        let tree = frame_tree(7);
        let observations = ServiceObservationSummary {
            update_services: vec!["audio".into()],
            cached_update_services: vec!["power".into()],
            interface_events: vec![ServiceInterfaceEventSubscription {
                service: "audio".into(),
                event: "changed".into(),
            }],
        };
        let frame = FrontendFrame::try_new(
            EffectSource::new("@mesh/test", Some("surface".into())),
            FrontendFrameRevisions::new(11, 4, 9, 7, 3),
            Some(tree),
            FrontendServiceSnapshot::new(3, observations.clone()),
            FrontendInvalidation::default(),
            Vec::new(),
            FrontendFrameEffects::from_host_requests(vec![CoreRequest::ShowSurface {
                surface_id: "@mesh/test".into(),
            }]),
            Vec::new(),
            FrontendPaintMetadata::new(12, Vec::new()),
        )
        .expect("matching frame revisions");

        assert_eq!(frame.revisions().frame, 11);
        assert_eq!(frame.revisions().catalog, 4);
        assert_eq!(frame.revisions().runtime, 9);
        assert_eq!(frame.tree().map(FrameSnapshot::revision), Some(7));
        assert_eq!(frame.services().revision(), 3);
        assert_eq!(frame.services().observations(), &observations);
        assert_eq!(frame.effects().host_requests().len(), 1);
        assert_eq!(
            frame.effects().host_request_revisions(),
            &[Some(FrontendEffectRevision::new(4, 9))]
        );
        assert_eq!(frame.paint().display_list_generation(), 12);
    }

    #[test]
    fn frontend_frame_rejects_mixed_tree_and_service_revisions() {
        let tree = frame_tree(7);
        let error = FrontendFrame::try_new(
            EffectSource::new("@mesh/test", None),
            FrontendFrameRevisions::new(1, 2, 3, 8, 5),
            Some(tree),
            FrontendServiceSnapshot::new(5, ServiceObservationSummary::default()),
            FrontendInvalidation::default(),
            Vec::new(),
            FrontendFrameEffects::default(),
            Vec::new(),
            FrontendPaintMetadata::default(),
        )
        .expect_err("tree revision mismatch must be rejected");

        assert_eq!(
            error,
            FrontendFrameError::TreeRevisionMismatch {
                expected: 8,
                actual: 7,
            }
        );

        let tree = frame_tree(7);
        let error = FrontendFrame::try_new(
            EffectSource::new("@mesh/test", None),
            FrontendFrameRevisions::new(1, 2, 3, 7, 5),
            Some(tree),
            FrontendServiceSnapshot::new(4, ServiceObservationSummary::default()),
            FrontendInvalidation::default(),
            Vec::new(),
            FrontendFrameEffects::default(),
            Vec::new(),
            FrontendPaintMetadata::default(),
        )
        .expect_err("service revision mismatch must be rejected");

        assert_eq!(
            error,
            FrontendFrameError::ServiceRevisionMismatch {
                expected: 5,
                actual: 4,
            }
        );
    }

    #[test]
    fn shell_adapter_rejects_typed_effects_from_obsolete_frame_revisions() {
        let frame = FrontendFrame::try_new(
            EffectSource::new("@mesh/test", Some("surface".into())),
            FrontendFrameRevisions::new(1, 4, 9, 0, 0),
            None,
            FrontendServiceSnapshot::new(0, ServiceObservationSummary::default()),
            FrontendInvalidation::default(),
            Vec::new(),
            FrontendFrameEffects::from_typed_effects(FrontendEffectBatch::new(
                effect_scope("shell.surface"),
                vec![FrontendEffect::Surface(SurfaceEffect::Show {
                    surface_id: "@mesh/test".into(),
                })],
            )),
            Vec::new(),
            FrontendPaintMetadata::default(),
        )
        .expect("frame effects should be bound to the frame revisions");

        assert!(matches!(
            ShellEffectAdapter::lower_frame(&frame, FrontendEffectRevision::new(5, 9)),
            Err(ShellEffectError::Rejected(
                EffectRejection::StaleRevision { .. }
            ))
        ));
        assert!(matches!(
            ShellEffectAdapter::lower_frame(
                &frame,
                FrontendEffectRevision::new(4, 9)
            ),
            Ok(requests) if matches!(requests.as_slice(), [CoreRequest::ShowSurface { .. }])
        ));
    }

    #[test]
    fn frontend_frame_rejects_stale_host_requests() {
        let frame = FrontendFrame::try_new(
            EffectSource::new("@mesh/test", Some("surface".into())),
            FrontendFrameRevisions::new(1, 5, 9, 0, 0),
            None,
            FrontendServiceSnapshot::new(0, ServiceObservationSummary::default()),
            FrontendInvalidation::default(),
            Vec::new(),
            FrontendFrameEffects::from_host_requests_at(
                vec![CoreRequest::ShowSurface {
                    surface_id: "@mesh/test".into(),
                }],
                FrontendEffectRevision::new(4, 9),
            ),
            Vec::new(),
            FrontendPaintMetadata::default(),
        )
        .expect_err("host requests from an older catalog must be rejected");

        assert!(matches!(
            frame,
            FrontendFrameError::EffectRevisionMismatch { message }
                if message.contains("host-request")
                    && message.contains("expected catalog/runtime 5/9")
        ));
    }

    #[test]
    fn frontend_frame_rejects_host_requests_from_an_old_runtime() {
        let frame = FrontendFrame::try_new(
            EffectSource::new("@mesh/test", Some("surface".into())),
            FrontendFrameRevisions::new(1, 5, 9, 0, 0),
            None,
            FrontendServiceSnapshot::new(0, ServiceObservationSummary::default()),
            FrontendInvalidation::default(),
            Vec::new(),
            FrontendFrameEffects::from_host_requests_at(
                vec![CoreRequest::ShowSurface {
                    surface_id: "@mesh/test".into(),
                }],
                FrontendEffectRevision::new(5, 8),
            ),
            Vec::new(),
            FrontendPaintMetadata::default(),
        )
        .expect_err("host requests from an older runtime must be rejected");

        assert!(matches!(
            frame,
            FrontendFrameError::EffectRevisionMismatch { message }
                if message.contains("host-request")
                    && message.contains("expected catalog/runtime 5/9")
                    && message.contains("got 5/8")
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
        /// The complete committed selection, including its fallback chain and
        /// revision. Subscribers must not reconstruct locale state from the
        /// active tag alone.
        selection: mesh_core_locale::LocaleSelection,
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
    /// Return the last atomically published frontend frame. The snapshot is
    /// immutable and remains valid until the component publishes a newer one.
    fn frontend_frame(&self) -> Option<FrontendFrame> {
        None
    }
    fn initial_visibility(&self) -> Option<bool> {
        None
    }
    fn mount(&mut self, ctx: ComponentContext) -> Result<Vec<CoreRequest>, ComponentError>;
    /// Contain a failure raised at the shell/component boundary. Frontend
    /// implementations can retain their last-known-good tree or install a
    /// bounded placeholder; simpler components may leave this as a no-op while
    /// the shell still records and supervises the failure.
    fn isolate_runtime_failure(&mut self, _phase: &str, _message: &str) -> bool {
        false
    }
    /// Clear a failure placeholder after a successful source replacement.
    fn clear_runtime_failure(&mut self) {}
    /// Tear down this component and run authored frontend lifecycle cleanup.
    ///
    /// Implementations should make this idempotent: the shell may call it
    /// before a reload, deactivation, replacement, or shutdown, and a drop
    /// guard may subsequently release the component.
    fn unmount(&mut self) -> Result<Vec<CoreRequest>, ComponentError> {
        Ok(Vec::new())
    }
    fn handle_core_event(&mut self, event: &CoreEvent) -> Result<Vec<CoreRequest>, ComponentError>;
    fn handle_service_event(
        &mut self,
        event: &ServiceEvent,
    ) -> Result<Vec<CoreRequest>, ComponentError>;
    /// Deliver a service update with the Rust-owned shell snapshot generation
    /// that admitted it. Implementations may use this to reject an older
    /// snapshot that was queued behind a newer provider state.
    fn handle_service_event_with_generation(
        &mut self,
        event: &ServiceEvent,
        _generation: u64,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        self.handle_service_event(event)
    }
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
    /// Cache a service update together with its authoritative Rust snapshot
    /// generation. Components without a cache can use the legacy hook.
    fn cache_service_payload_with_generation(&mut self, event: &ServiceEvent, _generation: u64) {
        self.cache_service_payload(event);
    }
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
    /// Revision of the normalized surface policy accepted by this component.
    /// The shell uses it to seed presentation configs without publishing a
    /// speculative generation before the compositor accepts the change.
    fn surface_policy_revision(&self) -> u64 {
        0
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
    /// Return a settings-selected role that still needs the shell's
    /// transactional transition supervisor. Components stage this separately
    /// from their realized role so the supervisor can tear down child
    /// surfaces, focus state, and compositor bookkeeping atomically.
    fn pending_surface_role_change(&self) -> Option<mesh_core_wayland::SurfaceRole> {
        None
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
