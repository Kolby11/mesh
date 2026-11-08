pub mod accessibility;
pub mod attributes;
pub mod bounds_index;
pub mod composition;
pub mod control_parts;
pub mod element;
pub mod frame;
pub mod frame_dirty;
pub mod frame_work;
pub mod interaction_contract;
pub mod layout;
pub mod lru;
pub mod popover;
pub mod prop;
pub mod pseudo_state;
pub mod shape;
pub mod style;
/// Element model and UI algorithms for MESH.
///
/// This crate owns the shared frontend intermediate representation:
/// core element definitions, `WidgetNode`, computed style data, layout
/// computation, event primitives, and accessibility tree data. It represents
/// what should be on screen but does not compile `.mesh` files, execute
/// scripts, paint pixels, or present Wayland/dev-window surfaces.
///
/// **Separation boundary**: this crate does NOT depend on `mesh-core-service`,
/// `mesh-core-wayland`, `mesh-core-render`, or `mesh-core-scripting`. Frontend
/// rendering orchestration lives in `mesh-core-render`; core shell wiring
/// lives in `mesh-core-shell`.
pub mod tree;

pub use accessibility::{
    AccessibilityInfo, AccessibilityRelationships, AccessibilityRole, AccessibilityState,
    AccessibilityTree, AccessibilityTreeNode, live_accessibility_focus, normalize_accessibility,
    normalize_accessibility_before_finalization, normalize_accessibility_finalized_memo,
    normalize_accessibility_with_validated_structure, try_normalize_accessibility,
};
pub use attributes::{AttrKey, AttributeMap};
pub use composition::{CallbackHandle, ComponentCompositionProps, HandlerTarget};
pub use control_parts::{ControlPart, control_part_names, control_part_nodes};
pub use element::{
    BASE_ELEMENT_FIELDS, ELEMENT_CONTRACT_DEFS, ELEMENT_TYPE_DEFS, ElementAttributeDef,
    ElementAttributeType, ElementContractDef, ElementDiagnostic, ElementDiagnosticKind,
    ElementEventDef, ElementFamily, ElementFieldDef, ElementFieldType, ElementKind, ElementRect,
    ElementSnapshot, ElementStateFlag, ElementStateSnapshot, ElementTypeDef, common_state_flags,
    element_contract_for_tag, element_contract_tags, element_default_attributes_for_tag,
    element_input_type_for_tag, element_runtime_tag_for_tag, element_snapshot,
    element_snapshot_json, element_type_for_tag, validate_element_attribute,
    validate_element_event,
};
pub use frame::{
    FrameNode, FramePhase, FramePhaseStamps, FrameSemanticNode, FrameSemanticRelationships,
    FrameSnapshot, FrameSnapshotError, PhaseStamp, SemanticChange, SemanticChangeKind,
    SemanticDiff, SemanticField, StableNodeIdentity,
};
pub use frame_dirty::FrameDirty;
pub use interaction_contract::{
    AffineClip, AffineClipStack, AffineTransform, HOVER_INTENT_CLOSE_DELAY, InteractionTarget,
    NodeEligibility, child_eligibility, child_transform, local_transform, node_can_receive_target,
    node_clip, node_eligibility, node_layout_bounds, node_transform, root_transform,
    transformed_layout_at, transformed_layout_for, transformed_offset,
};
pub use layout::{
    IntrinsicLayoutCache, LayoutEngine, LayoutRect, PerSurfaceLayoutState, TextMeasureContext,
    TextMeasureRevisions, TextMeasurer,
};
pub use popover::{
    PopoverAnchor, PopoverConstraintAdjustment, PopoverGrab, PopoverGravity, PopoverOpenOn,
    PopoverPlacement, PopoverPlacementDiagnostic, PopoverPlacementDiagnosticKind,
    PopoverPlacementField,
};
pub use prop::{ChangeClass, Prop, PropType};
pub use shape::{ShapeGeometry, ShapeKind, ShapeOp};
pub use pseudo_state::{
    PSEUDO_STATE_TABLE, PseudoState, PseudoStateKind, PseudoStateSpec, authored_element_state,
    pseudo_state_mask, pseudo_state_specs,
};
pub use style::{
    AlignContent, AlignItems, AlignSelf, BlendMode, BoxShadow, Color, ComputedStyle, CornerShape,
    Outline,
    CornerShapes, Corners, Dimension, Display, Edges, FlexDirection, FlexWrap, FontStyle,
    JustifyContent, Overflow, Position, StepPosition, StyleContext, StyleResolver, StyleRuleIndex,
    TextAlign, TextDirection, TextOverflow, Transform2D, TransitionEasing, TransitionProperties,
    TransitionStyle, VisualFilter, WhiteSpace,
};
pub use tree::EventHandlerCall;
pub use tree::{
    InputPreedit, KeyedTemplateItem, KeyedTemplateList, NodeId, NodeState, OpenBinding,
    PaintOrderChildren, TemplateBindings, TemplateHandlerBinding, TextSelection, WidgetNode,
    WidgetScrollMetrics, WidgetTreeValidationError, WindowSurfaceState, children_in_paint_order,
    for_each_node_at_paths_mut, paint_order_child_indices, validate_widget_tree,
};

/// Abstraction over the source of variable values for template evaluation.
///
/// Implemented by the scripting layer to provide script-side state
/// without `mesh-core-elements` depending on `mesh-core-scripting`.
/// Items of a windowed `{#for}` list a host has asked to build.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VirtualRange {
    /// First built item.
    pub first: usize,
    /// One past the last built item, overscan included. [`Self::FULL`] builds
    /// every item.
    pub last: usize,
    /// Main-axis size of one item in px, gap excluded; zero when not yet
    /// measured.
    pub extent: f32,
    /// Main-axis px of the leading and trailing spacers when the items are not
    /// all the same size, measured where known and estimated elsewhere. `None`
    /// derives both from `extent`.
    pub spacers: Option<(f32, f32)>,
}

impl VirtualRange {
    /// Marks a list that must be built in full.
    pub const FULL: Self = Self {
        first: 0,
        last: usize::MAX,
        extent: 0.0,
        spacers: None,
    };

    pub fn is_full(self) -> bool {
        self.first == 0 && self.last == usize::MAX
    }
}

pub trait VariableStore {
    fn get(&self, name: &str) -> Option<serde_json::Value>;
    fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
        let _ = name;
        None
    }
    fn keys(&self) -> Vec<String>;
    /// Look up a translation key. Returns `None` if no locale engine is available.
    fn translate(&self, key: &str) -> Option<String> {
        let _ = key;
        None
    }
    fn template_locals(&self) -> serde_json::Map<String, serde_json::Value> {
        serde_json::Map::new()
    }
    /// Stable identity inherited from a keyed `{#for}` iteration, if any.
    fn loop_identity(&self) -> Option<&str> {
        None
    }
    /// Retained collection identity for a fixed-shape keyed native loop.
    fn keyed_template_item(&self) -> Option<KeyedTemplateItem> {
        None
    }
    /// The range a host has chosen for the windowed `{#for}` list with this
    /// identity. `None` builds the initial window.
    fn virtual_window(&self, list_id: &str) -> Option<VirtualRange> {
        let _ = list_id;
        None
    }
    /// Largest surface extent in px the host is painting, so a list's initial
    /// window can cover the viewport before any row has been measured.
    fn virtual_viewport_hint(&self) -> Option<f32> {
        None
    }
    /// Whether a host drives windowed lists in this store. Large unannotated
    /// `{#for}` lists are only windowed automatically when it does.
    fn supports_virtual_windows(&self) -> bool {
        false
    }
    /// Whether expressions also read a captured loop item.
    fn in_loop(&self) -> bool {
        false
    }
    fn record_template_service_reads(&self, reads: &[(String, String)]) {
        let _ = reads;
    }
}
