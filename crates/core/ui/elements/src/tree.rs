/// Widget tree — the live, evaluated UI structure.
use crate::accessibility::AccessibilityInfo;
use crate::attributes::AttributeMap;
use crate::composition::HandlerTarget;
use crate::layout::LayoutRect;
use crate::style::ComputedStyle;
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
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
    /// Ambient state of the surface the node lives on, not of the node itself:
    /// the compositor's `xdg_toplevel` states, projected onto every node in the
    /// tree. This CSS engine has no descendant combinators, so a window state
    /// carried only on the root would be unreachable from the elements that
    /// need to restyle for it — a sidebar cannot say
    /// `.window:fullscreen .sidebar`. Carrying it on every node lets any
    /// element write `.sidebar:fullscreen` directly.
    ///
    /// Always false on layer surfaces and popups, which have no such states.
    pub window: WindowSurfaceState,
}

/// The compositor's view of the containing toplevel, projected onto every node
/// of that surface's tree as CSS state. See [`ElementState::window`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowSurfaceState {
    /// `:windowed` — the surface is realized as an `xdg_toplevel` rather than
    /// shell chrome. Unlike the flags below this is MESH's own decision, not the
    /// compositor's, and it is the one a component restyles against to draw its
    /// own window chrome (a "dock back" control instead of a "pop out" one).
    /// The four compositor states are only ever true when this is.
    pub windowed: bool,
    /// `:fullscreen` — the window covers a whole output.
    pub fullscreen: bool,
    /// `:maximized` — the window fills its work area.
    pub maximized: bool,
    /// `:activated` — the compositor considers the window focused.
    pub activated: bool,
    /// `:tiled` — some edge abuts a neighbour or screen edge.
    pub tiled: bool,
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
    pub handler: HandlerTarget,
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

/// Copy-on-write child topology for a [`WidgetNode`].
///
/// Component memo entries and live trees share this allocation. A mutable tree
/// walk copies only the immediate child-node overlays before descending; each
/// child's authored payload and descendants remain shared until that level is
/// actually mutated.
#[derive(Clone, Default)]
pub struct SharedWidgetChildren(Arc<Vec<WidgetNode>>);

impl SharedWidgetChildren {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(Arc::new(Vec::with_capacity(capacity)))
    }

    #[doc(hidden)]
    pub fn shares_allocation_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for SharedWidgetChildren {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Deref for SharedWidgetChildren {
    type Target = Vec<WidgetNode>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SharedWidgetChildren {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}

impl From<Vec<WidgetNode>> for SharedWidgetChildren {
    fn from(children: Vec<WidgetNode>) -> Self {
        Self(Arc::new(children))
    }
}

impl FromIterator<WidgetNode> for SharedWidgetChildren {
    fn from_iter<T: IntoIterator<Item = WidgetNode>>(iter: T) -> Self {
        Vec::from_iter(iter).into()
    }
}

impl Extend<WidgetNode> for SharedWidgetChildren {
    fn extend<T: IntoIterator<Item = WidgetNode>>(&mut self, iter: T) {
        self.deref_mut().extend(iter);
    }
}

impl<'a> IntoIterator for &'a SharedWidgetChildren {
    type Item = &'a WidgetNode;
    type IntoIter = std::slice::Iter<'a, WidgetNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut SharedWidgetChildren {
    type Item = &'a mut WidgetNode;
    type IntoIter = std::slice::IterMut<'a, WidgetNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl IntoIterator for SharedWidgetChildren {
    type Item = WidgetNode;
    type IntoIter = std::vec::IntoIter<WidgetNode>;

    fn into_iter(self) -> Self::IntoIter {
        match Arc::try_unwrap(self.0) {
            Ok(children) => children.into_iter(),
            Err(children) => children.as_ref().clone().into_iter(),
        }
    }
}

/// Immutable template/build payload shared by memo entries and live nodes.
///
/// Public fields preserve the existing `WidgetNode` field API through its
/// `Deref` implementation. Mutation is copy-on-write and is normally limited
/// to the handful of nodes whose runtime attributes actually change.
#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct WidgetNodeAuthored {
    pub tag: String,
    pub attributes: AttributeMap,
    pub event_handlers: BTreeMap<String, HandlerTarget>,
    pub event_handler_calls: BTreeMap<String, EventHandlerCall>,
    module_id: Option<Arc<str>>,
    pub service_field_reads: Vec<(String, String)>,
    /// Shell-owned composition flags that must survive memoized subtree reuse.
    composition: WidgetCompositionMetadata,
}

/// Typed metadata added while component trees are composed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WidgetCompositionMetadata {
    pub promoted_popover: bool,
}

/// A single node in the widget tree.
///
/// Produced by evaluating a template against script state. Each node has
/// computed styles, layout, accessibility info, and optional event handlers.
#[derive(Debug, Clone)]
pub struct WidgetNode {
    pub id: NodeId,
    /// Fully resolved style (theme tokens → concrete values).
    pub computed_style: ComputedStyle,
    /// Layout rectangle computed by the layout engine.
    pub layout: LayoutRect,
    /// Child nodes, shared copy-on-write across component memo hits.
    pub children: SharedWidgetChildren,
    /// Accessibility metadata.
    pub accessibility: AccessibilityInfo,
    /// Live interaction state (hover, focus, active, etc.).
    pub state: ElementState,
    /// Typed runtime scroll state, kept out of the string attribute map.
    pub scroll_metrics: Option<WidgetScrollMetrics>,
    /// Stable runtime identity for this node, kept out of the string attribute map.
    mesh_key: Option<String>,
    /// Cached split `class` tokens derived from the raw `class` attribute.
    cached_class_attr: Option<String>,
    cached_classes: Vec<String>,
    authored: Arc<WidgetNodeAuthored>,
}

impl Deref for WidgetNode {
    type Target = WidgetNodeAuthored;

    fn deref(&self) -> &Self::Target {
        &self.authored
    }
}

impl DerefMut for WidgetNode {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.authored)
    }
}

impl WidgetNode {
    /// Create a new node with defaults.
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            id: next_node_id(),
            computed_style: ComputedStyle::default(),
            layout: LayoutRect::default(),
            children: SharedWidgetChildren::new(),
            accessibility: AccessibilityInfo::default(),
            state: ElementState::default(),
            scroll_metrics: None,
            mesh_key: None,
            cached_class_attr: None,
            cached_classes: Vec::new(),
            authored: Arc::new(WidgetNodeAuthored {
                tag: tag.into(),
                attributes: AttributeMap::new(),
                event_handlers: BTreeMap::new(),
                event_handler_calls: BTreeMap::new(),
                module_id: None,
                service_field_reads: Vec::new(),
                composition: WidgetCompositionMetadata::default(),
            }),
        }
    }

    #[doc(hidden)]
    pub fn shares_authored_payload_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.authored, &other.authored)
    }

    /// Whether any node in this subtree shares authored template payload with
    /// `other`. This is useful at composition boundaries where runtime
    /// finalization may legitimately copy the root overlay while leaving clean
    /// descendants shared with a memo entry.
    #[doc(hidden)]
    pub fn contains_shared_authored_payload_with(&self, other: &Self) -> bool {
        self.shares_authored_payload_with(other)
            || self
                .children
                .iter()
                .any(|child| child.contains_shared_authored_payload_with(other))
    }

    #[doc(hidden)]
    pub fn authored_payload(&self) -> &WidgetNodeAuthored {
        &self.authored
    }

    pub fn mark_promoted_popover(&mut self) {
        self.composition.promoted_popover = true;
    }

    pub fn is_promoted_popover(&self) -> bool {
        self.composition.promoted_popover
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

    pub fn set_module_id(&mut self, module_id: impl Into<Arc<str>>) {
        self.module_id = Some(module_id.into());
    }

    /// The shared module identity, for handing the same allocation to sibling
    /// and child nodes built from the same module.
    pub fn shared_module_id(&self) -> Option<&Arc<str>> {
        self.module_id.as_ref()
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
        let class_attr = self.attributes.get("class").cloned();
        if self.cached_class_attr != class_attr {
            self.cached_classes = class_attr
                .as_deref()
                .into_iter()
                .flat_map(str::split_whitespace)
                .filter(|class| !class.is_empty())
                .map(str::to_owned)
                .collect();
            self.cached_class_attr = class_attr;
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

    fn representative_node(index: usize, depth: usize, width: usize) -> WidgetNode {
        let mut node = WidgetNode::new(format!("component-node-{index}"));
        node.attributes.insert(
            "content".into(),
            format!("node {index}: {}", "memoized component payload ".repeat(8)),
        );
        node.attributes
            .insert("class".into(), "surface-card primary interactive".into());
        node.event_handlers
            .insert("click".into(), format!("instance/{index}:activate").into());
        node.event_handler_calls.insert(
            "change".into(),
            EventHandlerCall {
                handler: format!("instance/{index}:change").into(),
                args: vec![serde_json::json!({ "index": index, "enabled": true })],
            },
        );
        node.service_field_reads
            .push(("audio".into(), format!("field_{index}")));
        if depth > 0 {
            node.children = (0..width)
                .map(|child| representative_node(index * width + child + 1, depth - 1, width))
                .collect();
        }
        node
    }

    fn legacy_deep_clone(node: &WidgetNode) -> WidgetNode {
        WidgetNode {
            id: node.id,
            computed_style: node.computed_style.clone(),
            layout: node.layout,
            children: node.children.iter().map(legacy_deep_clone).collect(),
            accessibility: node.accessibility.clone(),
            state: node.state,
            scroll_metrics: node.scroll_metrics,
            mesh_key: node.mesh_key.clone(),
            cached_class_attr: node.cached_class_attr.clone(),
            cached_classes: node.cached_classes.clone(),
            authored: Arc::new((*node.authored).clone()),
        }
    }

    fn dynamic_heap_bytes(node: &WidgetNode) -> usize {
        node.computed_style.transitions.capacity()
            * std::mem::size_of::<crate::style::TransitionStyle>()
            + node.computed_style.animations.capacity()
                * std::mem::size_of::<crate::style::AnimationStyle>()
            + node
                .accessibility
                .label
                .as_ref()
                .map_or(0, String::capacity)
            + node
                .accessibility
                .description
                .as_ref()
                .map_or(0, String::capacity)
            + node
                .accessibility
                .keyboard_shortcut
                .as_ref()
                .map_or(0, String::capacity)
            + node.mesh_key.as_ref().map_or(0, String::capacity)
            + node.cached_class_attr.as_ref().map_or(0, String::capacity)
            + node.cached_classes.capacity() * std::mem::size_of::<String>()
            + node
                .cached_classes
                .iter()
                .map(String::capacity)
                .sum::<usize>()
    }

    fn authored_heap_bytes(node: &WidgetNode) -> usize {
        node.tag.capacity()
            + node
                .attributes
                .iter()
                .map(|(key, value)| key.len() + value.capacity())
                .sum::<usize>()
            + node.event_handlers.len() * std::mem::size_of::<(String, HandlerTarget)>()
            + node
                .event_handlers
                .iter()
                .map(|(event, handler)| event.capacity() + handler.dynamic_heap_bytes())
                .sum::<usize>()
            + node.event_handler_calls.len() * std::mem::size_of::<(String, EventHandlerCall)>()
            + node
                .event_handler_calls
                .iter()
                .map(|(event, call)| {
                    event.capacity()
                        + call.handler.dynamic_heap_bytes()
                        + call.args.capacity() * std::mem::size_of::<serde_json::Value>()
                })
                .sum::<usize>()
            + node.service_field_reads.capacity() * std::mem::size_of::<(String, String)>()
            + node
                .service_field_reads
                .iter()
                .map(|(service, field)| service.capacity() + field.capacity())
                .sum::<usize>()
    }

    /// Conservative heap payload owned only by this tree clone. Allocator
    /// headers and allocations nested inside JSON values are intentionally
    /// omitted, so the reported COW memory reduction is a lower bound.
    fn exclusive_heap_bytes(node: &WidgetNode) -> usize {
        let mut bytes = dynamic_heap_bytes(node);
        if Arc::strong_count(&node.authored) == 1 {
            bytes += std::mem::size_of::<WidgetNodeAuthored>() + authored_heap_bytes(node);
        }
        if Arc::strong_count(&node.children.0) == 1 {
            bytes += node.children.capacity() * std::mem::size_of::<WidgetNode>();
            bytes += node
                .children
                .iter()
                .map(exclusive_heap_bytes)
                .sum::<usize>();
        }
        bytes
    }

    #[test]
    fn new_widget_node_has_empty_service_field_reads() {
        assert!(WidgetNode::new("text").service_field_reads.is_empty());
    }

    #[test]
    fn widget_node_clone_copies_only_the_mutated_cow_path() {
        let mut root = representative_node(0, 2, 2);
        let mut cloned = root.clone();

        assert!(root.shares_authored_payload_with(&cloned));
        assert!(root.children.shares_allocation_with(&cloned.children));
        assert!(root.children[0].shares_authored_payload_with(&cloned.children[0]));
        assert!(root.children[1].shares_authored_payload_with(&cloned.children[1]));

        cloned.children[0]
            .attributes
            .insert("content".into(), "changed".into());

        assert_ne!(
            root.children[0].attributes.get("content"),
            cloned.children[0].attributes.get("content")
        );
        assert!(!root.children[0].shares_authored_payload_with(&cloned.children[0]));
        assert!(
            root.children[1].shares_authored_payload_with(&cloned.children[1]),
            "an untouched sibling should keep sharing its authored payload"
        );
        assert!(
            root.shares_authored_payload_with(&cloned),
            "mutating a descendant must not copy the root payload"
        );

        root.children[0]
            .attributes
            .insert("content".into(), "original changed separately".into());
        assert_eq!(
            cloned.children[0]
                .attributes
                .get("content")
                .map(String::as_str),
            Some("changed")
        );
    }

    // cargo test --release -p mesh-core-elements --lib cow_widget_tree_clone_beats_legacy_deep_clone -- --ignored --nocapture
    #[test]
    #[ignore = "release-only component memo tree-clone benchmark"]
    fn cow_widget_tree_clone_beats_legacy_deep_clone() {
        const ITERATIONS: usize = 2_000;
        let wide = representative_node(0, 2, 16);
        let deep = representative_node(0, 96, 1);

        fn measure(tree: &WidgetNode) -> (std::time::Duration, std::time::Duration, usize, usize) {
            let legacy_started = std::time::Instant::now();
            for _ in 0..ITERATIONS {
                std::hint::black_box(legacy_deep_clone(std::hint::black_box(tree)));
                std::hint::black_box(legacy_deep_clone(std::hint::black_box(tree)));
            }
            let legacy = legacy_started.elapsed();

            let cow_started = std::time::Instant::now();
            for _ in 0..ITERATIONS {
                std::hint::black_box(std::hint::black_box(tree).clone());
                std::hint::black_box(std::hint::black_box(tree).clone());
            }
            let cow = cow_started.elapsed();

            let legacy_clone = legacy_deep_clone(tree);
            let cow_clone = tree.clone();
            (
                legacy,
                cow,
                exclusive_heap_bytes(&legacy_clone),
                exclusive_heap_bytes(&cow_clone),
            )
        }

        let (wide_legacy, wide_cow, wide_legacy_bytes, wide_cow_bytes) = measure(&wide);
        let (deep_legacy, deep_cow, deep_legacy_bytes, deep_cow_bytes) = measure(&deep);
        let wide_speedup = wide_legacy.as_secs_f64() / wide_cow.as_secs_f64();
        let deep_speedup = deep_legacy.as_secs_f64() / deep_cow.as_secs_f64();

        eprintln!(
            "component memo COW clone: wide_nodes={} legacy={wide_legacy:?} cow={wide_cow:?} speedup={wide_speedup:.2}x retained={wide_legacy_bytes}->{wide_cow_bytes}B; deep_nodes={} legacy={deep_legacy:?} cow={deep_cow:?} speedup={deep_speedup:.2}x retained={deep_legacy_bytes}->{deep_cow_bytes}B",
            wide.node_count(),
            deep.node_count(),
        );
        assert!(
            wide_speedup >= 5.0,
            "wide COW clone should be at least 5x faster, measured {wide_speedup:.2}x"
        );
        assert!(
            deep_speedup >= 5.0,
            "deep COW clone should be at least 5x faster, measured {deep_speedup:.2}x"
        );
        assert!(
            wide_cow_bytes * 10 <= wide_legacy_bytes,
            "wide COW clone should retain at least 10x fewer unique heap bytes"
        );
        assert!(
            deep_cow_bytes * 10 <= deep_legacy_bytes,
            "deep COW clone should retain at least 10x fewer unique heap bytes"
        );
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
                .insert(format!("attr{index}").into(), format!("value{index}"));
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
