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
    pub attributes: HashMap<String, String>,
    /// Cached split tokens for the `class` attribute.
    cached_class_source: Option<String>,
    cached_classes: Vec<String>,
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
            cached_class_source: None,
            cached_classes: Vec::new(),
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

    /// Refresh the cached whitespace-split class tokens after mutating
    /// `attributes["class"]`.
    pub fn refresh_class_cache(&mut self) {
        let Some(class_source) = self.attributes.get("class") else {
            self.cached_class_source = None;
            self.cached_classes.clear();
            return;
        };

        if self.cached_class_source.as_deref() == Some(class_source.as_str()) {
            return;
        }

        self.cached_classes.clear();
        self.cached_classes
            .extend(class_source.split_whitespace().map(str::to_owned));
        self.cached_class_source = Some(class_source.clone());
    }

    /// Borrow cached class tokens when they match the current `class`
    /// attribute. Returns `None` if direct attribute mutation left the cache
    /// stale; callers can split the raw attribute as a correctness fallback.
    pub fn cached_classes(&self) -> Option<&[String]> {
        let class_source = self.attributes.get("class")?;
        if self.cached_class_source.as_deref() == Some(class_source.as_str()) {
            Some(&self.cached_classes)
        } else {
            None
        }
    }
}
