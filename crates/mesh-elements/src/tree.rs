/// Widget tree — the live, evaluated UI structure.
use crate::accessibility::AccessibilityInfo;
use crate::layout::LayoutRect;
use crate::style::ComputedStyle;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Live interaction state for a single node.
///
/// Updated by `InputState::process` as pointer and keyboard events arrive.
/// Read by `selector_matches` to evaluate pseudo-class selectors like `:hover`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementState {
    pub hovered: bool,
    pub active: bool,
    pub focused: bool,
    pub disabled: bool,
    pub checked: bool,
}

/// Unique identifier for a node in the widget tree.
pub type NodeId = u64;

static NEXT_NODE_ID: AtomicU64 = AtomicU64::new(1);

/// Generate a unique node ID.
pub fn next_node_id() -> NodeId {
    NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed)
}

/// A single node in the widget tree.
///
/// Produced by evaluating a template against script state. Each node has
/// computed styles, layout, accessibility info, and optional event handlers.
#[derive(Debug, Clone)]
pub struct WidgetNode {
    pub id: NodeId,
    /// Tag name: `row`, `column`, `text`, `button`, `image`, `icon`, etc.
    pub tag: String,
    /// Resolved attributes (after binding evaluation).
    pub attributes: HashMap<String, String>,
    /// Fully resolved style (theme tokens → concrete values).
    pub computed_style: ComputedStyle,
    /// Layout rectangle computed by the layout engine.
    pub layout: LayoutRect,
    /// Child nodes.
    pub children: Vec<WidgetNode>,
    /// Accessibility metadata.
    pub accessibility: AccessibilityInfo,
    /// Event handler mappings: event name → script handler name.
    pub event_handlers: HashMap<String, String>,
    /// Live interaction state (hover, focus, active, etc.).
    pub state: ElementState,
}

impl WidgetNode {
    /// Create a new node with defaults.
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            id: next_node_id(),
            tag: tag.into(),
            attributes: HashMap::new(),
            computed_style: ComputedStyle::default(),
            layout: LayoutRect::default(),
            children: Vec::new(),
            accessibility: AccessibilityInfo::default(),
            event_handlers: HashMap::new(),
            state: ElementState::default(),
        }
    }

    /// Recursively find a node by ID.
    pub fn find(&self, id: NodeId) -> Option<&WidgetNode> {
        if self.id == id {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find(id) {
                return Some(found);
            }
        }
        None
    }

    /// Recursively find a node by ID, returning a mutable reference.
    pub fn find_mut(&mut self, id: NodeId) -> Option<&mut WidgetNode> {
        if self.id == id {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_mut(id) {
                return Some(found);
            }
        }
        None
    }

    /// Count total nodes in this subtree.
    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.node_count()).sum::<usize>()
    }
}
