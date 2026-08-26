//! Compiled surface contracts and shared metadata for fields whose meaning
//! depends on a surface role.
//!
//! Manifest diagnostics, settings validation/ejection, and presentation
//! lowering all consume the same table and normalized policy products. The
//! crate intentionally contains no manifest, settings, or Wayland types so
//! those boundary crates can depend on it without forming a cycle.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRoleKind {
    Layer,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceFieldScope {
    Layer,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRoleField {
    Title,
    AppId,
    Resizable,
    Decorations,
    Anchor,
    Layer,
    ExclusiveZone,
    KeyboardMode,
    Margins,
    Blur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceRoleFieldMetadata {
    pub field: SurfaceRoleField,
    pub scope: SurfaceFieldScope,
    /// Canonical author-facing key in `mesh.surface`.
    pub manifest_key: &'static str,
    /// Canonical sparse settings keys that represent this field.
    pub settings_keys: &'static [&'static str],
}

impl SurfaceRoleFieldMetadata {
    pub const fn applies_to(self, role: SurfaceRoleKind, promotable: bool) -> bool {
        promotable
            || matches!(
                (self.scope, role),
                (SurfaceFieldScope::Layer, SurfaceRoleKind::Layer)
                    | (SurfaceFieldScope::Window, SurfaceRoleKind::Window)
            )
    }
}

const TITLE_SETTINGS_KEYS: &[&str] = &["title"];
const APP_ID_SETTINGS_KEYS: &[&str] = &["app_id"];
const RESIZABLE_SETTINGS_KEYS: &[&str] = &["resizable"];
const DECORATIONS_SETTINGS_KEYS: &[&str] = &["decorations"];
const ANCHOR_SETTINGS_KEYS: &[&str] = &["anchor"];
const LAYER_SETTINGS_KEYS: &[&str] = &["layer"];
const EXCLUSIVE_ZONE_SETTINGS_KEYS: &[&str] = &["exclusive_zone"];
const KEYBOARD_MODE_SETTINGS_KEYS: &[&str] = &["keyboard_mode"];
const MARGIN_SETTINGS_KEYS: &[&str] =
    &["margin_top", "margin_right", "margin_bottom", "margin_left"];
const BLUR_SETTINGS_KEYS: &[&str] = &["blur"];

/// The one role-field vocabulary shared by authoring, settings, ejection, and
/// presentation. Keep entries ordered by the surface schema so diagnostics
/// and ejected settings remain deterministic.
pub const SURFACE_ROLE_FIELD_METADATA: &[SurfaceRoleFieldMetadata] = &[
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Title,
        scope: SurfaceFieldScope::Window,
        manifest_key: "title",
        settings_keys: TITLE_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::AppId,
        scope: SurfaceFieldScope::Window,
        manifest_key: "appId",
        settings_keys: APP_ID_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Resizable,
        scope: SurfaceFieldScope::Window,
        manifest_key: "resizable",
        settings_keys: RESIZABLE_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Decorations,
        scope: SurfaceFieldScope::Window,
        manifest_key: "decorations",
        settings_keys: DECORATIONS_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Anchor,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "anchor",
        settings_keys: ANCHOR_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Layer,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "layer",
        settings_keys: LAYER_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::ExclusiveZone,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "exclusiveZone",
        settings_keys: EXCLUSIVE_ZONE_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::KeyboardMode,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "keyboardMode",
        settings_keys: KEYBOARD_MODE_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Margins,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "margins",
        settings_keys: MARGIN_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Blur,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "blur",
        settings_keys: BLUR_SETTINGS_KEYS,
    },
];

pub fn surface_role_field_metadata(field: SurfaceRoleField) -> &'static SurfaceRoleFieldMetadata {
    SURFACE_ROLE_FIELD_METADATA
        .iter()
        .find(|metadata| metadata.field == field)
        .expect("every SurfaceRoleField has metadata")
}

pub fn role_field_applies(
    field: SurfaceRoleField,
    role: SurfaceRoleKind,
    promotable: bool,
) -> bool {
    surface_role_field_metadata(field).applies_to(role, promotable)
}

/// Where a semantic policy value came from. A declared contract is the
/// immutable author boundary; settings and runtime overrides are layered on
/// top by [`SurfacePolicyCompiler`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyValueSource {
    Declared,
    Settings,
    Runtime,
    Derived,
}

/// Coarse provenance for the effective policy. Grouping fields by the
/// protocol boundary keeps provenance useful without duplicating every
/// snapshot member in a second parallel structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfacePolicyProvenance {
    pub role: SurfacePolicyValueSource,
    pub window: SurfacePolicyValueSource,
    pub placement: SurfacePolicyValueSource,
    pub keyboard: SurfacePolicyValueSource,
    pub visibility: SurfacePolicyValueSource,
}

impl Default for SurfacePolicyProvenance {
    fn default() -> Self {
        Self {
            role: SurfacePolicyValueSource::Declared,
            window: SurfacePolicyValueSource::Declared,
            placement: SurfacePolicyValueSource::Declared,
            keyboard: SurfacePolicyValueSource::Declared,
            visibility: SurfacePolicyValueSource::Declared,
        }
    }
}

/// The normalized, compositor-independent values that participate in a live
/// surface policy decision.
///
/// This is deliberately owned by the policy crate rather than by either the
/// settings resolver or a Wayland backend. Callers lower their local surface
/// types into this snapshot once, and every consumer then uses the same typed
/// diff. In particular, a new field cannot be added to the shell's reload
/// comparison without also being considered by presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfacePolicySnapshot {
    /// Monotonic generation assigned by [`SurfacePolicyGenerator`]. The
    /// revision is metadata and is intentionally ignored when comparing the
    /// semantic values below.
    pub revision: u64,
    pub role: SurfaceRoleKind,
    pub promotable: bool,
    pub visible: bool,
    /// The base compositor namespace. `blur` is kept separately so the
    /// snapshot records the author intent even before a backend lowers it.
    pub namespace: String,
    pub blur: bool,
    pub window_title: Option<String>,
    pub window_app_id: Option<String>,
    pub window_resizable: bool,
    pub window_decorations: SurfacePolicyDecorations,
    pub edge: Option<SurfacePolicyEdge>,
    pub layer: SurfacePolicyLayer,
    pub size_policy: SurfacePolicySizePolicy,
    pub content_size: Option<(u32, u32)>,
    pub surface_size: Option<(u32, u32)>,
    pub width_spans_output: bool,
    pub height_spans_output: bool,
    pub exclusive_zone: i32,
    pub keyboard_mode: SurfacePolicyKeyboardMode,
    pub margins: [i32; 4],
    pub padding: [u32; 4],
}

impl Default for SurfacePolicySnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            role: SurfaceRoleKind::Layer,
            promotable: false,
            visible: false,
            namespace: String::new(),
            blur: false,
            window_title: None,
            window_app_id: None,
            window_resizable: true,
            window_decorations: SurfacePolicyDecorations::Client,
            edge: Some(SurfacePolicyEdge::Top),
            layer: SurfacePolicyLayer::Top,
            size_policy: SurfacePolicySizePolicy::Fixed,
            content_size: None,
            surface_size: None,
            width_spans_output: false,
            height_spans_output: false,
            exclusive_zone: 0,
            keyboard_mode: SurfacePolicyKeyboardMode::None,
            margins: [0; 4],
            padding: [0; 4],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyDecorations {
    Client,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyEdge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyLayer {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicySizePolicy {
    Fixed,
    Flexible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyKeyboardMode {
    None,
    Exclusive,
    OnDemand,
}

/// The validated author contract produced from a module's declared surface
/// block. The snapshot contains normalized declared defaults and manifest
/// values; runtime geometry and input padding are intentionally unresolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredSurfaceContract {
    pub snapshot: SurfacePolicySnapshot,
    pub promotable: bool,
    pub provenance: SurfacePolicyProvenance,
}

impl DeclaredSurfaceContract {
    pub fn from_snapshot(mut snapshot: SurfacePolicySnapshot) -> Self {
        snapshot.revision = 0;
        snapshot.content_size = None;
        snapshot.surface_size = None;
        snapshot.width_spans_output = false;
        snapshot.height_spans_output = false;
        snapshot.padding = [0; 4];
        Self {
            promotable: snapshot.promotable,
            snapshot,
            provenance: SurfacePolicyProvenance::default(),
        }
    }

    pub fn role_change_allowed(&self, requested: SurfaceRoleKind) -> bool {
        self.snapshot.role == requested || self.promotable
    }
}

/// Sparse normalized settings/runtime values layered over a declared
/// contract. `None` means that the declared value remains effective; a
/// missing value is therefore different from a meaningful zero or `false`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SurfacePolicyPatch {
    pub role: Option<SurfaceRoleKind>,
    pub visible: Option<bool>,
    pub namespace: Option<String>,
    pub blur: Option<bool>,
    pub window_title: Option<String>,
    pub window_app_id: Option<String>,
    pub window_resizable: Option<bool>,
    pub window_decorations: Option<SurfacePolicyDecorations>,
    pub edge: Option<SurfacePolicyEdge>,
    pub layer: Option<SurfacePolicyLayer>,
    pub size_policy: Option<SurfacePolicySizePolicy>,
    pub content_size: Option<(u32, u32)>,
    pub surface_size: Option<(u32, u32)>,
    pub width_spans_output: Option<bool>,
    pub height_spans_output: Option<bool>,
    pub exclusive_zone: Option<i32>,
    pub keyboard_mode: Option<SurfacePolicyKeyboardMode>,
    pub margins: Option<[i32; 4]>,
    pub padding: Option<[u32; 4]>,
}

impl SurfacePolicyPatch {
    /// Build the sparse patch represented by the values that differ from a
    /// declared baseline. This is the bridge used by settings resolution: the
    /// resolver retains its existing validation/diagnostics, while the policy
    /// crate owns the actual precedence and revisioned product.
    pub fn between(base: &SurfacePolicySnapshot, target: &SurfacePolicySnapshot) -> Self {
        Self {
            role: (base.role != target.role).then_some(target.role),
            visible: (base.visible != target.visible).then_some(target.visible),
            namespace: (base.namespace != target.namespace).then(|| target.namespace.clone()),
            blur: (base.blur != target.blur).then_some(target.blur),
            window_title: (base.window_title != target.window_title)
                .then(|| target.window_title.clone())
                .flatten(),
            window_app_id: (base.window_app_id != target.window_app_id)
                .then(|| target.window_app_id.clone())
                .flatten(),
            window_resizable: (base.window_resizable != target.window_resizable)
                .then_some(target.window_resizable),
            window_decorations: (base.window_decorations != target.window_decorations)
                .then_some(target.window_decorations),
            edge: (base.edge != target.edge).then_some(target.edge).flatten(),
            layer: (base.layer != target.layer).then_some(target.layer),
            size_policy: (base.size_policy != target.size_policy).then_some(target.size_policy),
            content_size: (base.content_size != target.content_size)
                .then_some(target.content_size)
                .flatten(),
            surface_size: (base.surface_size != target.surface_size)
                .then_some(target.surface_size)
                .flatten(),
            width_spans_output: (base.width_spans_output != target.width_spans_output)
                .then_some(target.width_spans_output),
            height_spans_output: (base.height_spans_output != target.height_spans_output)
                .then_some(target.height_spans_output),
            exclusive_zone: (base.exclusive_zone != target.exclusive_zone)
                .then_some(target.exclusive_zone),
            keyboard_mode: (base.keyboard_mode != target.keyboard_mode)
                .then_some(target.keyboard_mode),
            margins: (base.margins != target.margins).then_some(target.margins),
            padding: (base.padding != target.padding).then_some(target.padding),
        }
    }

    fn apply_to(
        &self,
        contract: &DeclaredSurfaceContract,
    ) -> (
        SurfacePolicySnapshot,
        SurfacePolicyProvenance,
        Vec<SurfacePolicyDiagnostic>,
    ) {
        let mut snapshot = contract.snapshot.clone();
        let mut provenance = contract.provenance;
        let mut diagnostics = Vec::new();

        if let Some(requested) = self.role {
            if contract.role_change_allowed(requested) {
                snapshot.role = requested;
                provenance.role = SurfacePolicyValueSource::Settings;
            } else if requested != snapshot.role {
                diagnostics.push(SurfacePolicyDiagnostic::RoleChangeRejected {
                    declared: snapshot.role,
                    requested,
                });
            }
        }
        if let Some(value) = self.visible {
            snapshot.visible = value;
            provenance.visibility = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = &self.namespace {
            snapshot.namespace = value.clone();
            provenance.placement = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = self.blur {
            snapshot.blur = value;
            provenance.placement = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = &self.window_title {
            snapshot.window_title = Some(value.clone());
            provenance.window = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = &self.window_app_id {
            snapshot.window_app_id = Some(value.clone());
            provenance.window = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = self.window_resizable {
            snapshot.window_resizable = value;
            provenance.window = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = self.window_decorations {
            snapshot.window_decorations = value;
            provenance.window = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = self.edge {
            snapshot.edge = Some(value);
            provenance.placement = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = self.layer {
            snapshot.layer = value;
            provenance.placement = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = self.size_policy {
            snapshot.size_policy = value;
            provenance.placement = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = self.content_size {
            snapshot.content_size = Some(value);
            provenance.placement = SurfacePolicyValueSource::Derived;
        }
        if let Some(value) = self.surface_size {
            snapshot.surface_size = Some(value);
            provenance.placement = SurfacePolicyValueSource::Derived;
        }
        if let Some(value) = self.width_spans_output {
            snapshot.width_spans_output = value;
            provenance.placement = SurfacePolicyValueSource::Derived;
        }
        if let Some(value) = self.height_spans_output {
            snapshot.height_spans_output = value;
            provenance.placement = SurfacePolicyValueSource::Derived;
        }
        if let Some(value) = self.exclusive_zone {
            snapshot.exclusive_zone = value;
            provenance.placement = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = self.keyboard_mode {
            snapshot.keyboard_mode = value;
            provenance.keyboard = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = self.margins {
            snapshot.margins = value;
            provenance.placement = SurfacePolicyValueSource::Settings;
        }
        if let Some(value) = self.padding {
            snapshot.padding = value;
            provenance.placement = SurfacePolicyValueSource::Derived;
        }

        (snapshot, provenance, diagnostics)
    }
}

/// Diagnostics emitted while compiling normalized policy layers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfacePolicyDiagnostic {
    RoleChangeRejected {
        declared: SurfaceRoleKind,
        requested: SurfaceRoleKind,
    },
}

/// The complete effective product of policy compilation. The snapshot is the
/// immutable value consumed by shell/presentation; the other fields preserve
/// the contract, source provenance, sparse overrides, and non-fatal policy
/// diagnostics that led to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveSurfacePolicy {
    pub contract: DeclaredSurfaceContract,
    pub snapshot: SurfacePolicySnapshot,
    pub overrides: SurfacePolicyPatch,
    pub provenance: SurfacePolicyProvenance,
    pub diagnostics: Vec<SurfacePolicyDiagnostic>,
}

impl EffectiveSurfacePolicy {
    pub fn snapshot(&self) -> &SurfacePolicySnapshot {
        &self.snapshot
    }
}

/// The semantic work required to move from one accepted surface policy to the
/// next. The values are ordered from the most destructive transition to the
/// least destructive one by [`SurfacePolicySnapshot::diff`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyChange {
    Noop,
    InputRegionOnly,
    MeasureAgain,
    LayerConfigure,
    LayerRecreate,
    WindowLive,
    WindowRecreate,
    RoleTransition,
    VisibilityOnly,
}

/// A revisioned semantic policy diff shared by shell reload and presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfacePolicyDiff {
    pub change: SurfacePolicyChange,
    pub from_revision: Option<u64>,
    pub to_revision: u64,
}

impl SurfacePolicyDiff {
    pub const fn is_noop(self) -> bool {
        matches!(self.change, SurfacePolicyChange::Noop)
    }

    pub const fn requires_recreation(self) -> bool {
        matches!(
            self.change,
            SurfacePolicyChange::LayerRecreate
                | SurfacePolicyChange::WindowRecreate
                | SurfacePolicyChange::RoleTransition
        )
    }

    pub const fn requires_fresh_configure(self) -> bool {
        matches!(
            self.change,
            SurfacePolicyChange::LayerConfigure
                | SurfacePolicyChange::LayerRecreate
                | SurfacePolicyChange::WindowRecreate
                | SurfacePolicyChange::RoleTransition
        )
    }

    pub const fn transition_plan(self) -> SurfaceTransitionPlan {
        SurfaceTransitionPlan::from_diff(self)
    }
}

/// The shell-level operation selected by a semantic policy diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceTransitionAction {
    Reject,
    Keep,
    UpdateInputRegion,
    MeasureAgain,
    ConfigureLayer,
    RecreateLayer,
    UpdateWindow,
    RecreateWindow,
    TransitionRole,
    UpdateVisibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceChildTransition {
    Preserve,
    Recreate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceFocusTransition {
    Preserve,
    ClearAndReacquire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceInputTransition {
    Preserve,
    RecomputeRegion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePresentationTransition {
    Noop,
    ApplyLive,
    AwaitMeasurement,
    AwaitConfigure,
    RecreateObject,
}

/// A typed plan for carrying one effective policy from the shell into
/// presentation. The plan deliberately includes lifecycle-adjacent effects
/// so callers do not need another field list to decide what happens to child
/// surfaces, focus, input regions, or compositor readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceTransitionPlan {
    pub diff: SurfacePolicyDiff,
    pub action: SurfaceTransitionAction,
    pub children: SurfaceChildTransition,
    pub focus: SurfaceFocusTransition,
    pub input: SurfaceInputTransition,
    pub presentation: SurfacePresentationTransition,
}

impl SurfaceTransitionPlan {
    pub const fn rejected(diff: SurfacePolicyDiff) -> Self {
        Self {
            diff,
            action: SurfaceTransitionAction::Reject,
            children: SurfaceChildTransition::Preserve,
            focus: SurfaceFocusTransition::Preserve,
            input: SurfaceInputTransition::Preserve,
            presentation: SurfacePresentationTransition::Noop,
        }
    }

    pub const fn from_diff(diff: SurfacePolicyDiff) -> Self {
        let (action, children, focus, input, presentation) = match diff.change {
            SurfacePolicyChange::Noop => (
                SurfaceTransitionAction::Keep,
                SurfaceChildTransition::Preserve,
                SurfaceFocusTransition::Preserve,
                SurfaceInputTransition::Preserve,
                SurfacePresentationTransition::Noop,
            ),
            SurfacePolicyChange::InputRegionOnly => (
                SurfaceTransitionAction::UpdateInputRegion,
                SurfaceChildTransition::Preserve,
                SurfaceFocusTransition::Preserve,
                SurfaceInputTransition::RecomputeRegion,
                SurfacePresentationTransition::ApplyLive,
            ),
            SurfacePolicyChange::MeasureAgain => (
                SurfaceTransitionAction::MeasureAgain,
                SurfaceChildTransition::Preserve,
                SurfaceFocusTransition::Preserve,
                SurfaceInputTransition::Preserve,
                SurfacePresentationTransition::AwaitMeasurement,
            ),
            SurfacePolicyChange::LayerConfigure => (
                SurfaceTransitionAction::ConfigureLayer,
                SurfaceChildTransition::Preserve,
                SurfaceFocusTransition::Preserve,
                SurfaceInputTransition::RecomputeRegion,
                SurfacePresentationTransition::AwaitConfigure,
            ),
            SurfacePolicyChange::LayerRecreate => (
                SurfaceTransitionAction::RecreateLayer,
                SurfaceChildTransition::Recreate,
                SurfaceFocusTransition::ClearAndReacquire,
                SurfaceInputTransition::RecomputeRegion,
                SurfacePresentationTransition::RecreateObject,
            ),
            SurfacePolicyChange::WindowLive => (
                SurfaceTransitionAction::UpdateWindow,
                SurfaceChildTransition::Preserve,
                SurfaceFocusTransition::Preserve,
                SurfaceInputTransition::Preserve,
                SurfacePresentationTransition::ApplyLive,
            ),
            SurfacePolicyChange::WindowRecreate => (
                SurfaceTransitionAction::RecreateWindow,
                SurfaceChildTransition::Recreate,
                SurfaceFocusTransition::ClearAndReacquire,
                SurfaceInputTransition::RecomputeRegion,
                SurfacePresentationTransition::RecreateObject,
            ),
            SurfacePolicyChange::RoleTransition => (
                SurfaceTransitionAction::TransitionRole,
                SurfaceChildTransition::Recreate,
                SurfaceFocusTransition::ClearAndReacquire,
                SurfaceInputTransition::RecomputeRegion,
                SurfacePresentationTransition::RecreateObject,
            ),
            SurfacePolicyChange::VisibilityOnly => (
                SurfaceTransitionAction::UpdateVisibility,
                SurfaceChildTransition::Preserve,
                SurfaceFocusTransition::Preserve,
                SurfaceInputTransition::Preserve,
                SurfacePresentationTransition::ApplyLive,
            ),
        };
        Self {
            diff,
            action,
            children,
            focus,
            input,
            presentation,
        }
    }
}

impl SurfacePolicySnapshot {
    /// Compare semantic policy values while carrying the revisions through the
    /// result for stale-generation diagnostics and accepted-cache assertions.
    pub fn diff(&self, next: &Self) -> SurfacePolicyDiff {
        let change = if self.role != next.role {
            SurfacePolicyChange::RoleTransition
        } else if self.role == SurfaceRoleKind::Layer {
            if self.namespace != next.namespace || self.blur != next.blur {
                SurfacePolicyChange::LayerRecreate
            } else if self.layer_geometry_changed(next) {
                SurfacePolicyChange::LayerConfigure
            } else if self.padding != next.padding || self.keyboard_mode != next.keyboard_mode {
                SurfacePolicyChange::InputRegionOnly
            } else if self.measurement_changed(next) {
                SurfacePolicyChange::MeasureAgain
            } else if self.visible != next.visible {
                SurfacePolicyChange::VisibilityOnly
            } else {
                SurfacePolicyChange::Noop
            }
        } else if self.window_decorations != next.window_decorations {
            SurfacePolicyChange::WindowRecreate
        } else if self.window_live_state_changed(next) {
            SurfacePolicyChange::WindowLive
        } else if self.padding != next.padding {
            SurfacePolicyChange::InputRegionOnly
        } else if self.measurement_changed(next) {
            SurfacePolicyChange::MeasureAgain
        } else if self.visible != next.visible {
            SurfacePolicyChange::VisibilityOnly
        } else {
            SurfacePolicyChange::Noop
        };

        SurfacePolicyDiff {
            change,
            from_revision: Some(self.revision),
            to_revision: next.revision,
        }
    }

    fn layer_geometry_changed(&self, next: &Self) -> bool {
        self.edge != next.edge
            || self.layer != next.layer
            || self.layer_wire_size() != next.layer_wire_size()
            || self.width_spans_output != next.width_spans_output
            || self.height_spans_output != next.height_spans_output
            || self.exclusive_zone != next.exclusive_zone
            || self.margins != next.margins
    }

    /// The size a layer surface actually puts on the wire, per axis.
    ///
    /// An axis that spans its output is sent as the protocol's `0` no matter
    /// what the resolved extent is, so its resolved size is invisible to the
    /// compositor. Comparing the resolved size instead would classify the
    /// first output-size resolution (a placeholder width becoming the real
    /// output width) as a geometry change, and `LayerConfigure` invalidates
    /// the surface's configured state while it waits for a fresh configure —
    /// one the compositor has no reason to send, because every request it
    /// received was byte-identical to the last. The surface then never
    /// presents again: no hover restyle, no service-driven repaint, no clock
    /// tick. A resolved-size change with an unchanged wire size is a
    /// measurement change, not a reconfigure.
    fn layer_wire_size(&self) -> (Option<u32>, Option<u32>) {
        let (width, height) = match self.surface_size {
            Some((width, height)) => (Some(width), Some(height)),
            None => (None, None),
        };
        (
            width.filter(|_| !self.width_spans_output),
            height.filter(|_| !self.height_spans_output),
        )
    }

    fn window_live_state_changed(&self, next: &Self) -> bool {
        self.window_title != next.window_title
            || self.window_app_id != next.window_app_id
            || self.window_resizable != next.window_resizable
            || self.content_size != next.content_size
            || self.surface_size != next.surface_size
    }

    /// Whether the resolved measurement changed at all — including a change
    /// that carries no protocol request with it, such as a spanning axis
    /// resolving from its pre-output placeholder to the real output size.
    /// Those frames still have to repaint at the new extent; they just must
    /// not wait on a configure. See [`Self::layer_wire_size`].
    fn measurement_changed(&self, next: &Self) -> bool {
        self.content_size != next.content_size || self.surface_size != next.surface_size
    }
}

/// Assigns monotonically increasing revisions to accepted policy snapshots.
///
/// Candidates are not visible until [`Self::update`] returns; a failed
/// configure can therefore be retried with the same revision instead of
/// advancing the accepted generation speculatively.
#[derive(Debug, Clone, Default)]
pub struct SurfacePolicyGenerator {
    current: Option<SurfacePolicySnapshot>,
    next_revision: u64,
}

#[derive(Debug, Clone)]
pub struct SurfacePolicyUpdate {
    pub previous: Option<SurfacePolicySnapshot>,
    pub current: SurfacePolicySnapshot,
    pub diff: SurfacePolicyDiff,
}

impl SurfacePolicyGenerator {
    pub fn current(&self) -> Option<&SurfacePolicySnapshot> {
        self.current.as_ref()
    }

    pub fn update(&mut self, mut candidate: SurfacePolicySnapshot) -> SurfacePolicyUpdate {
        let previous = self.current.clone();
        let candidate_change = previous
            .as_ref()
            .map(|previous| previous.diff(&candidate).change)
            .unwrap_or(SurfacePolicyChange::MeasureAgain);
        let revision = if let Some(previous) = previous.as_ref()
            && matches!(candidate_change, SurfacePolicyChange::Noop)
        {
            previous.revision
        } else {
            self.next_revision = self.next_revision.saturating_add(1).max(1);
            self.next_revision
        };
        candidate.revision = revision;
        let diff = previous.as_ref().map_or(
            SurfacePolicyDiff {
                change: SurfacePolicyChange::MeasureAgain,
                from_revision: None,
                to_revision: revision,
            },
            |previous| previous.diff(&candidate),
        );
        self.current = Some(candidate.clone());
        SurfacePolicyUpdate {
            previous,
            current: candidate,
            diff,
        }
    }
}

/// Compiles one declared contract plus a sparse override patch into the
/// effective policy consumed by the shell. One compiler belongs to one live
/// surface, which gives it a stable revision stream across reloads and
/// runtime overrides.
#[derive(Debug, Clone, Default)]
pub struct SurfacePolicyCompiler {
    generator: SurfacePolicyGenerator,
}

impl SurfacePolicyCompiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn compile(
        &mut self,
        contract: &DeclaredSurfaceContract,
        overrides: &SurfacePolicyPatch,
    ) -> EffectiveSurfacePolicy {
        let (candidate, provenance, diagnostics) = overrides.apply_to(contract);
        let update = self.generator.update(candidate);
        EffectiveSurfacePolicy {
            contract: contract.clone(),
            snapshot: update.current,
            overrides: overrides.clone(),
            provenance,
            diagnostics,
        }
    }

    pub fn current(&self) -> Option<&SurfacePolicySnapshot> {
        self.generator.current()
    }

    pub fn plan(
        previous: Option<&EffectiveSurfacePolicy>,
        next: &EffectiveSurfacePolicy,
    ) -> SurfaceTransitionPlan {
        let diff = previous.map_or(
            SurfacePolicyDiff {
                change: SurfacePolicyChange::MeasureAgain,
                from_revision: None,
                to_revision: next.snapshot.revision,
            },
            |previous| previous.snapshot.diff(&next.snapshot),
        );
        if next.diagnostics.is_empty() {
            diff.transition_plan()
        } else {
            SurfaceTransitionPlan::rejected(diff)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_field_metadata_covers_every_role_specific_field() {
        assert_eq!(SURFACE_ROLE_FIELD_METADATA.len(), 10);
        for metadata in SURFACE_ROLE_FIELD_METADATA {
            assert!(!metadata.manifest_key.is_empty());
            assert!(!metadata.settings_keys.is_empty());
            assert!(metadata.applies_to(SurfaceRoleKind::Layer, true));
            assert!(metadata.applies_to(SurfaceRoleKind::Window, true));
        }
    }

    #[test]
    fn role_field_metadata_distinguishes_layer_and_window_fields() {
        assert!(role_field_applies(
            SurfaceRoleField::Margins,
            SurfaceRoleKind::Layer,
            false
        ));
        assert!(!role_field_applies(
            SurfaceRoleField::Margins,
            SurfaceRoleKind::Window,
            false
        ));
        assert!(role_field_applies(
            SurfaceRoleField::Decorations,
            SurfaceRoleKind::Window,
            false
        ));
        assert!(!role_field_applies(
            SurfaceRoleField::Decorations,
            SurfaceRoleKind::Layer,
            false
        ));
    }

    #[test]
    fn semantic_diff_classifies_creation_geometry_and_input_changes() {
        let previous = SurfacePolicySnapshot {
            revision: 4,
            namespace: "panel".into(),
            content_size: Some((800, 40)),
            surface_size: Some((800, 40)),
            ..SurfacePolicySnapshot::default()
        };

        let mut input = previous.clone();
        input.revision = 5;
        input.padding = [0, 0, 0, 8];
        assert_eq!(
            previous.diff(&input).change,
            SurfacePolicyChange::InputRegionOnly
        );

        let mut geometry = previous.clone();
        geometry.revision = 5;
        geometry.margins = [1, 0, 0, 0];
        assert_eq!(
            previous.diff(&geometry).change,
            SurfacePolicyChange::LayerConfigure
        );

        let mut creation = previous.clone();
        creation.revision = 5;
        creation.blur = true;
        assert_eq!(
            previous.diff(&creation).change,
            SurfacePolicyChange::LayerRecreate
        );
    }

    #[test]
    fn spanning_layer_axis_resolving_its_output_size_is_a_measurement_not_a_configure() {
        // A top bar spans its output: the wire width is the protocol's `0`
        // before and after, so the compositor sees no change and will not send
        // a fresh configure. Classifying this as `LayerConfigure` invalidated
        // the surface's configured state permanently and froze every later
        // frame.
        let previous = SurfacePolicySnapshot {
            revision: 1,
            namespace: "panel".into(),
            content_size: Some((1, 56)),
            surface_size: Some((1, 256)),
            width_spans_output: true,
            exclusive_zone: 56,
            ..SurfacePolicySnapshot::default()
        };
        let resolved = SurfacePolicySnapshot {
            revision: 2,
            content_size: Some((2880, 56)),
            surface_size: Some((2880, 256)),
            ..previous.clone()
        };

        let diff = previous.diff(&resolved);
        assert_eq!(diff.change, SurfacePolicyChange::MeasureAgain);
        assert!(!diff.requires_fresh_configure());
        assert!(!diff.is_noop(), "the new extent still has to reach paint");
    }

    #[test]
    fn fixed_layer_axis_resizing_still_requires_a_fresh_configure() {
        let previous = SurfacePolicySnapshot {
            revision: 1,
            namespace: "panel".into(),
            content_size: Some((320, 56)),
            surface_size: Some((320, 56)),
            ..SurfacePolicySnapshot::default()
        };
        let resized = SurfacePolicySnapshot {
            revision: 2,
            content_size: Some((480, 56)),
            surface_size: Some((480, 56)),
            ..previous.clone()
        };

        let diff = previous.diff(&resized);
        assert_eq!(diff.change, SurfacePolicyChange::LayerConfigure);
        assert!(diff.requires_fresh_configure());
    }

    #[test]
    fn generator_reuses_noop_revision_and_advances_only_on_change() {
        let mut generator = SurfacePolicyGenerator::default();
        let first = generator.update(SurfacePolicySnapshot::default());
        assert_eq!(first.current.revision, 1);
        assert_eq!(first.diff.change, SurfacePolicyChange::MeasureAgain);
        assert_eq!(first.diff.from_revision, None);
        assert_eq!(first.diff.to_revision, 1);

        let same = generator.update(SurfacePolicySnapshot::default());
        assert_eq!(same.current.revision, 1);
        assert_eq!(same.diff.change, SurfacePolicyChange::Noop);
        assert_eq!(same.diff.from_revision, Some(1));
        assert_eq!(same.diff.to_revision, 1);

        let mut changed = SurfacePolicySnapshot::default();
        changed.padding = [0, 0, 0, 4];
        let update = generator.update(changed);
        assert_eq!(update.current.revision, 2);
        assert_eq!(update.diff.change, SurfacePolicyChange::InputRegionOnly);
        assert_eq!(update.diff.from_revision, Some(1));
        assert_eq!(update.diff.to_revision, 2);
    }

    #[test]
    fn compiler_produces_effective_policy_and_role_transition_plan() {
        let declared = DeclaredSurfaceContract::from_snapshot(SurfacePolicySnapshot {
            promotable: true,
            ..SurfacePolicySnapshot::default()
        });
        let first_patch = SurfacePolicyPatch {
            blur: Some(true),
            ..SurfacePolicyPatch::default()
        };
        let mut compiler = SurfacePolicyCompiler::new();
        let first = compiler.compile(&declared, &first_patch);
        assert_eq!(first.snapshot.revision, 1);
        assert!(first.diagnostics.is_empty());
        assert_eq!(
            first.provenance.placement,
            SurfacePolicyValueSource::Settings
        );

        let second_patch = SurfacePolicyPatch {
            role: Some(SurfaceRoleKind::Window),
            ..first_patch
        };
        let second = compiler.compile(&declared, &second_patch);
        let plan = SurfacePolicyCompiler::plan(Some(&first), &second);
        assert_eq!(plan.diff.change, SurfacePolicyChange::RoleTransition);
        assert_eq!(plan.action, SurfaceTransitionAction::TransitionRole);
        assert_eq!(plan.children, SurfaceChildTransition::Recreate);
        assert_eq!(plan.focus, SurfaceFocusTransition::ClearAndReacquire);
        assert_eq!(
            plan.presentation,
            SurfacePresentationTransition::RecreateObject
        );
    }

    #[test]
    fn compiler_rejects_unauthorized_role_patch_without_changing_effective_role() {
        let declared = DeclaredSurfaceContract::from_snapshot(SurfacePolicySnapshot::default());
        let patch = SurfacePolicyPatch {
            role: Some(SurfaceRoleKind::Window),
            ..SurfacePolicyPatch::default()
        };
        let effective = SurfacePolicyCompiler::new().compile(&declared, &patch);

        assert_eq!(effective.snapshot.role, SurfaceRoleKind::Layer);
        assert_eq!(
            effective.diagnostics,
            vec![SurfacePolicyDiagnostic::RoleChangeRejected {
                declared: SurfaceRoleKind::Layer,
                requested: SurfaceRoleKind::Window,
            }]
        );
        let plan = SurfacePolicyCompiler::plan(None, &effective);
        assert_eq!(plan.action, SurfaceTransitionAction::Reject);
    }
}
