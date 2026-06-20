/// Widget tree — the live, evaluated UI structure.
use crate::accessibility::AccessibilityInfo;
use crate::layout::LayoutRect;
use crate::style::ComputedStyle;
use std::collections::BTreeMap;
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
    pub focus_visible: bool,
    pub disabled: bool,
    pub read_only: bool,
    pub required: bool,
    pub selected: bool,
    pub checked: bool,
    pub expanded: bool,
    pub pressed: bool,
    pub invalid: bool,
    pub value: bool,
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
    pub attributes: BTreeMap<String, String>,
    /// Fully resolved style (theme tokens → concrete values).
    pub computed_style: ComputedStyle,
    /// Layout rectangle computed by the layout engine.
    pub layout: LayoutRect,
    /// Child nodes.
    pub children: Vec<WidgetNode>,
    /// Accessibility metadata.
    pub accessibility: AccessibilityInfo,
    /// Event handler mappings: event name → script handler name.
    pub event_handlers: BTreeMap<String, String>,
    /// Live interaction state (hover, focus, active, etc.).
    pub state: ElementState,
    /// Service field reads captured during template evaluation.
    /// Each entry is a (service_name, field_name) pair read by this node's expressions.
    pub service_field_reads: Vec<(String, String)>,
    /// Cached split `class` tokens derived from the raw `class` attribute.
    cached_class_attr: Option<String>,
    cached_classes: Vec<String>,
}

impl WidgetNode {
    /// Create a new node with defaults.
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            id: next_node_id(),
            tag: tag.into(),
            attributes: BTreeMap::new(),
            computed_style: ComputedStyle::default(),
            layout: LayoutRect::default(),
            children: Vec::new(),
            accessibility: AccessibilityInfo::default(),
            event_handlers: BTreeMap::new(),
            state: ElementState::default(),
            service_field_reads: Vec::new(),
            cached_class_attr: None,
            cached_classes: Vec::new(),
        }
    }

    pub fn refresh_class_tokens_cache(&mut self) {
        let class_attr = self.attributes.get("class").map(String::as_str);
        if self.cached_class_attr.as_deref() != class_attr {
            self.cached_classes = class_attr
                .into_iter()
                .flat_map(str::split_whitespace)
                .filter(|class| !class.is_empty())
                .map(str::to_owned)
                .collect();
            self.cached_class_attr = class_attr.map(str::to_owned);
        }
    }

    pub fn class_tokens(&self) -> &[String] {
        &self.cached_classes
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_widget_node_has_empty_service_field_reads() {
        assert!(WidgetNode::new("text").service_field_reads.is_empty());
    }

    #[test]
    fn class_tokens_refresh_when_class_attribute_changes() {
        let mut node = WidgetNode::new("text");
        node.refresh_class_tokens_cache();
        assert!(node.class_tokens().is_empty());

        node.attributes.insert("class".into(), "primary compact".into());
        node.refresh_class_tokens_cache();
        assert_eq!(node.class_tokens(), ["primary", "compact"]);

        node.attributes.insert("class".into(), "compact active".into());
        node.refresh_class_tokens_cache();
        assert_eq!(node.class_tokens(), ["compact", "active"]);

        node.attributes.remove("class");
        node.refresh_class_tokens_cache();
        assert!(node.class_tokens().is_empty());
    }
}
