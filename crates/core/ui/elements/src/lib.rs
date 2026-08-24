pub mod accessibility;
pub mod attributes;
pub mod composition;
pub mod element;
pub mod events;
pub mod frame;
pub mod interaction_contract;
pub mod layout;
pub mod lru;
pub mod popover;
pub mod pseudo_state;
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
};
pub use attributes::{AttrKey, AttributeMap};
pub use composition::{ComponentCompositionProps, HandlerTarget};
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
pub use events::{
    EventDispatcher, InputDispatchResult, InputState, Modifiers, RawInputEvent, UiEvent,
};
pub use frame::{
    FrameNode, FramePhase, FramePhaseStamps, FrameSemanticNode, FrameSemanticRelationships,
    FrameSnapshot, FrameSnapshotError, PhaseStamp, SemanticChange, SemanticChangeKind,
    SemanticDiff, SemanticField, StableNodeIdentity,
};
pub use interaction_contract::{
    InteractionTarget, NodeEligibility, child_eligibility, node_eligibility, transformed_layout_at,
    transformed_layout_for, transformed_offset,
};
pub use layout::{
    IntrinsicLayoutCache, LayoutEngine, LayoutRect, PerSurfaceLayoutState, TextMeasureContext,
    TextMeasureRevisions, TextMeasurer,
};
pub use popover::{
    PopoverAnchor, PopoverConstraintAdjustment, PopoverGrab, PopoverGravity, PopoverPlacement,
    PopoverPlacementDiagnostic, PopoverPlacementDiagnosticKind, PopoverPlacementField,
};
pub use pseudo_state::{
    PSEUDO_STATE_TABLE, PseudoState, PseudoStateKind, PseudoStateSpec, authored_element_state,
    pseudo_state_mask, pseudo_state_specs,
};
pub use style::{
    AlignContent, AlignItems, AlignSelf, BlendMode, BoxShadow, Color, ComputedStyle, Corners,
    Dimension, Display, Edges, FlexDirection, FlexWrap, FontStyle, JustifyContent, Overflow,
    Position, StepPosition, StyleContext, StyleResolver, StyleRuleIndex, TextAlign, TextDirection,
    TextOverflow, Transform2D, TransitionEasing, TransitionProperties, TransitionStyle,
    VisualFilter, WhiteSpace,
};
pub use tree::EventHandlerCall;
pub use tree::{ElementState, NodeId, WidgetNode, WidgetScrollMetrics, WindowSurfaceState};

/// Abstraction over the source of variable values for template evaluation.
///
/// Implemented by the scripting layer to provide script-side state
/// without `mesh-core-elements` depending on `mesh-core-scripting`.
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
    fn record_template_service_reads(&self, reads: &[(String, String)]) {
        let _ = reads;
    }
}
