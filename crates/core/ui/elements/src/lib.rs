pub mod accessibility;
pub mod element;
pub mod events;
pub mod layout;
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
    AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityTree,
    AccessibilityTreeNode,
};
pub use element::{
    BASE_ELEMENT_FIELDS, ELEMENT_TYPE_DEFS, ElementFieldDef, ElementFieldType, ElementKind,
    ElementRect, ElementSnapshot, ElementStateSnapshot, ElementTypeDef, element_snapshot,
    element_snapshot_json, element_type_for_tag,
};
pub use events::{EventDispatcher, InputState, Modifiers, RawInputEvent, UiEvent};
pub use layout::{IntrinsicLayoutCache, LayoutEngine, LayoutRect, TextMeasurer};
pub use style::{
    AlignContent, AlignItems, AlignSelf, Color, ComputedStyle, Corners, Dimension, Display, Edges,
    FlexDirection, FlexWrap, FontStyle, JustifyContent, Overflow, Position, StyleContext,
    StyleResolver, TextAlign, TextDirection, TextOverflow, Transform2D, TransitionEasing,
    TransitionProperties, TransitionStyle,
};
pub use tree::{ElementState, NodeId, WidgetNode};

/// Abstraction over the source of variable values for template evaluation.
///
/// Implemented by the scripting layer to provide script-side state
/// without `mesh-core-elements` depending on `mesh-core-scripting`.
pub trait VariableStore {
    fn get(&self, name: &str) -> Option<serde_json::Value>;
    fn keys(&self) -> Vec<String>;
    /// Look up a translation key. Returns `None` if no locale engine is available.
    fn translate(&self, key: &str) -> Option<String> {
        let _ = key;
        None
    }
}
