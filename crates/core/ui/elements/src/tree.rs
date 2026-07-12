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

/// Pre-bound event-handler call generated from markup like
/// `onclick={handler(arg)}`.
///
/// Kept out of `event_handlers` string values so compiled trees do not encode
/// handler-call arguments as JSON strings that must be reparsed at dispatch.
#[derive(Debug, Clone, PartialEq)]
pub struct EventHandlerCall {
    pub handler: String,
    pub args: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WidgetScrollMetrics {
    pub x: f32,
    pub y: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub content_width: f32,
    pub content_height: f32,
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
    /// Event handler mappings with pre-bound arguments.
    pub event_handler_calls: BTreeMap<String, EventHandlerCall>,
    /// Live interaction state (hover, focus, active, etc.).
    pub state: ElementState,
    /// Typed runtime scroll state, kept out of the string attribute map.
    pub scroll_metrics: Option<WidgetScrollMetrics>,
    /// Stable runtime identity for this node, kept out of the string attribute map.
    mesh_key: Option<String>,
    /// Source module identity used for module-scoped theme defaults.
    module_id: Option<String>,
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
            event_handler_calls: BTreeMap::new(),
            state: ElementState::default(),
            scroll_metrics: None,
            mesh_key: None,
            module_id: None,
            service_field_reads: Vec::new(),
            cached_class_attr: None,
            cached_classes: Vec::new(),
        }
    }

    pub fn set_mesh_key(&mut self, key: impl Into<String>) {
        self.mesh_key = Some(key.into());
    }

    pub fn clear_mesh_key(&mut self) {
        self.mesh_key = None;
    }

    pub fn mesh_key(&self) -> Option<&str> {
        self.mesh_key
            .as_deref()
            .or_else(|| self.attributes.get("_mesh_key").map(String::as_str))
    }

    pub fn has_mesh_key(&self) -> bool {
        self.mesh_key().is_some()
    }

    pub fn set_module_id(&mut self, module_id: impl Into<String>) {
        self.module_id = Some(module_id.into());
    }

    pub fn clear_module_id(&mut self) {
        self.module_id = None;
    }

    pub fn module_id(&self) -> Option<&str> {
        self.module_id
            .as_deref()
            .or_else(|| self.attributes.get("_mesh_module_id").map(String::as_str))
    }

    pub fn resolved_scroll_metrics(&self) -> WidgetScrollMetrics {
        if let Some(scroll_metrics) = self.scroll_metrics {
            return scroll_metrics;
        }
        let value = |key: &str| {
            self.attributes
                .get(key)
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or(0.0)
        };
        WidgetScrollMetrics {
            x: value("_mesh_scroll_x"),
            y: value("_mesh_scroll_y"),
            max_x: value("_mesh_scroll_max_x"),
            max_y: value("_mesh_scroll_max_y"),
            content_width: value("_mesh_content_width"),
            content_height: value("_mesh_content_height"),
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

        node.attributes
            .insert("class".into(), "primary compact".into());
        node.refresh_class_tokens_cache();
        assert_eq!(node.class_tokens(), ["primary", "compact"]);

        node.attributes
            .insert("class".into(), "compact active".into());
        node.refresh_class_tokens_cache();
        assert_eq!(node.class_tokens(), ["compact", "active"]);

        node.attributes.remove("class");
        node.refresh_class_tokens_cache();
        assert!(node.class_tokens().is_empty());
    }

    #[test]
    fn mesh_key_uses_typed_field_before_legacy_attribute() {
        let mut node = WidgetNode::new("button");
        assert_eq!(node.mesh_key(), None);

        node.attributes
            .insert("_mesh_key".into(), "legacy/path".into());
        assert_eq!(node.mesh_key(), Some("legacy/path"));

        node.set_mesh_key("typed/path");
        assert_eq!(node.mesh_key(), Some("typed/path"));

        node.clear_mesh_key();
        assert_eq!(node.mesh_key(), Some("legacy/path"));
    }

    #[test]
    fn module_id_uses_typed_field_before_legacy_attribute() {
        let mut node = WidgetNode::new("button");
        assert_eq!(node.module_id(), None);

        node.attributes
            .insert("_mesh_module_id".into(), "@legacy/module".into());
        assert_eq!(node.module_id(), Some("@legacy/module"));

        node.set_module_id("@typed/module");
        assert_eq!(node.module_id(), Some("@typed/module"));

        node.clear_module_id();
        assert_eq!(node.module_id(), Some("@legacy/module"));
    }

    // cargo test -p mesh-core-elements --release -- typed_mesh_key_assignment_beats_attribute_map_insert --ignored --nocapture
    #[test]
    #[ignore = "release-only mesh key assignment microbenchmark"]
    fn typed_mesh_key_assignment_beats_attribute_map_insert() {
        let iterations = 500_000usize;

        let attribute_started = std::time::Instant::now();
        let mut attribute_total = 0usize;
        for index in 0..iterations {
            let mut node = WidgetNode::new("row");
            let key = format!("root/{index}");
            node.attributes.insert("_mesh_key".into(), key);
            attribute_total =
                attribute_total.wrapping_add(std::hint::black_box(node.mesh_key().unwrap().len()));
        }
        let attribute_time = attribute_started.elapsed();

        let typed_started = std::time::Instant::now();
        let mut typed_total = 0usize;
        for index in 0..iterations {
            let mut node = WidgetNode::new("row");
            node.set_mesh_key(format!("root/{index}"));
            typed_total =
                typed_total.wrapping_add(std::hint::black_box(node.mesh_key().unwrap().len()));
        }
        let typed_time = typed_started.elapsed();

        eprintln!(
            "mesh key assignment: attribute map {attribute_time:?}; typed field {typed_time:?}; ratio {:.1}x; totals={attribute_total}/{typed_total}",
            attribute_time.as_secs_f64() / typed_time.as_secs_f64()
        );
        assert_eq!(attribute_total, typed_total);
        assert!(typed_time < attribute_time);
    }

    // cargo test -p mesh-core-elements --release -- typed_module_id_assignment_beats_attribute_map_insert --ignored --nocapture
    #[test]
    #[ignore = "release-only module id assignment microbenchmark"]
    fn typed_module_id_assignment_beats_attribute_map_insert() {
        let iterations = 500_000usize;
        let module_id = "@mesh/navigation-bar";
        let mut template = WidgetNode::new("row");
        for index in 0..8 {
            template
                .attributes
                .insert(format!("attr{index}"), format!("value{index}"));
        }

        let attribute_started = std::time::Instant::now();
        let mut attribute_total = 0usize;
        for _ in 0..iterations {
            let mut node = template.clone();
            node.attributes
                .insert("_mesh_module_id".into(), module_id.to_string());
            attribute_total =
                attribute_total.wrapping_add(std::hint::black_box(node.module_id().unwrap().len()));
        }
        let attribute_time = attribute_started.elapsed();

        let typed_started = std::time::Instant::now();
        let mut typed_total = 0usize;
        for _ in 0..iterations {
            let mut node = template.clone();
            node.set_module_id(module_id);
            typed_total =
                typed_total.wrapping_add(std::hint::black_box(node.module_id().unwrap().len()));
        }
        let typed_time = typed_started.elapsed();

        eprintln!(
            "module id assignment: attribute map {attribute_time:?}; typed field {typed_time:?}; ratio {:.1}x; totals={attribute_total}/{typed_total}",
            attribute_time.as_secs_f64() / typed_time.as_secs_f64()
        );
        assert_eq!(attribute_total, typed_total);
        assert!(typed_time < attribute_time);
    }
}
