/// UI runtime for MESH.
///
/// This crate owns the widget tree, layout computation, style resolution,
/// event dispatch, and accessibility tree. It represents what should be
/// on screen but does not paint pixels.
///
/// **Separation boundary**: this crate does NOT depend on `mesh-service`,
/// `mesh-wayland`, `mesh-renderer`, or `mesh-scripting`. Frontends connect
/// to backends only through bindings injected by the scripting layer.

pub mod tree;
pub mod style;
pub mod layout;
pub mod events;
pub mod accessibility;

pub use tree::{WidgetNode, NodeId};
pub use style::{ComputedStyle, Color, Dimension, Edges, Corners, StyleResolver};
pub use layout::{LayoutRect, LayoutEngine};
pub use events::{UiEvent, RawInputEvent, Modifiers, EventDispatcher};
pub use accessibility::{AccessibilityInfo, AccessibilityState, AccessibilityTree, AccessibilityTreeNode};

/// Abstraction over the source of variable values for template evaluation.
///
/// Implemented by the scripting layer to provide script-side state
/// without `mesh-ui` depending on `mesh-scripting`.
pub trait VariableStore {
    fn get(&self, name: &str) -> Option<serde_json::Value>;
    fn keys(&self) -> Vec<String>;
    /// Look up a translation key. Returns `None` if no locale engine is available.
    fn translate(&self, key: &str) -> Option<String> {
        let _ = key;
        None
    }
}
