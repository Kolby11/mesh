use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use bitflags::bitflags;
use mesh_core_elements::style::{Color, ComputedStyle, Corners, Dimension, Edges, Transform2D};
use mesh_core_elements::{ElementState, LayoutRect, NodeId, WidgetNode, element_snapshot_json};
use mesh_core_interaction::ScrollOffsetState;
use slotmap::{SecondaryMap, SlotMap, new_key_type};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RetainedTreeDirtySummary {
    pub(super) inserted: usize,
    pub(super) removed: usize,
    pub(super) layout: usize,
    pub(super) style: usize,
    pub(super) attributes: usize,
    pub(super) children: usize,
    pub(super) state: usize,
}

impl RetainedTreeDirtySummary {
    pub(super) fn any(self) -> bool {
        self.inserted > 0
            || self.removed > 0
            || self.layout > 0
            || self.style > 0
            || self.attributes > 0
            || self.children > 0
            || self.state > 0
    }

    fn add_flags(&mut self, flags: RetainedNodeDirtyFlags) {
        if flags.contains(RetainedNodeDirtyFlags::LAYOUT) {
            self.layout += 1;
        }
        if flags.contains(RetainedNodeDirtyFlags::STYLE) {
            self.style += 1;
        }
        if flags.contains(RetainedNodeDirtyFlags::ATTRIBUTES) {
            self.attributes += 1;
        }
        if flags.contains(RetainedNodeDirtyFlags::CHILDREN) {
            self.children += 1;
        }
        if flags.contains(RetainedNodeDirtyFlags::STATE) {
            self.state += 1;
        }
    }

    pub(super) fn to_debug_counts(self) -> mesh_core_debug::RetainedInvalidationCounts {
        mesh_core_debug::RetainedInvalidationCounts {
            inserted: self.inserted as u64,
            removed: self.removed as u64,
            layout: self.layout as u64,
            style: self.style as u64,
            attributes: self.attributes as u64,
            children: self.children as u64,
            state: self.state as u64,
        }
    }
}

new_key_type! {
    pub(super) struct RetainedNodeKey;
}

bitflags! {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub(super) struct RetainedNodeDirtyFlags: u16 {
        const INSERTED = 1 << 0;
        const LAYOUT = 1 << 1;
        const STYLE = 1 << 2;
        const ATTRIBUTES = 1 << 3;
        const CHILDREN = 1 << 4;
        const STATE = 1 << 5;
    }
}

#[derive(Debug, Default)]
pub(super) struct RetainedWidgetTree {
    generation: u64,
    nodes: SlotMap<RetainedNodeKey, RetainedNodeSnapshot>,
    node_keys: HashMap<NodeId, RetainedNodeKey>,
    dirty: SecondaryMap<RetainedNodeKey, RetainedNodeDirtyFlags>,
    last_dirty: RetainedTreeDirtySummary,
}

impl RetainedWidgetTree {
    pub(super) fn update(&mut self, root: &WidgetNode) -> RetainedTreeDirtySummary {
        let mut next_nodes = HashMap::new();
        collect_retained_snapshots(root, &mut next_nodes);

        let mut dirty = RetainedTreeDirtySummary::default();
        let mut next_dirty = SecondaryMap::new();

        for (&node_id, next) in &next_nodes {
            match self.node_keys.get(&node_id).copied() {
                Some(previous) => {
                    if let Some(previous_snapshot) = self.nodes.get(previous) {
                        let flags = previous_snapshot.diff_flags(next);
                        if flags.is_empty() {
                            continue;
                        }
                        dirty.add_flags(flags);
                        next_dirty.insert(previous, flags);
                        if let Some(slot) = self.nodes.get_mut(previous) {
                            *slot = next.clone();
                        }
                    } else {
                        let key = self.nodes.insert(next.clone());
                        self.node_keys.insert(node_id, key);
                        next_dirty.insert(key, RetainedNodeDirtyFlags::INSERTED);
                        dirty.inserted += 1;
                    }
                }
                None => {
                    let key = self.nodes.insert(next.clone());
                    self.node_keys.insert(node_id, key);
                    next_dirty.insert(key, RetainedNodeDirtyFlags::INSERTED);
                    dirty.inserted += 1;
                }
            }
        }

        let removed_ids: Vec<_> = self
            .node_keys
            .keys()
            .copied()
            .filter(|id| !next_nodes.contains_key(id))
            .collect();
        for node_id in removed_ids {
            if let Some(key) = self.node_keys.remove(&node_id) {
                self.nodes.remove(key);
                dirty.removed += 1;
            }
        }

        if dirty.any() {
            self.generation = self.generation.saturating_add(1);
        }
        self.dirty = next_dirty;
        self.last_dirty = dirty;
        dirty
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn last_dirty(&self) -> RetainedTreeDirtySummary {
        self.last_dirty
    }

    #[cfg(test)]
    fn dirty_flags_for(&self, node_id: NodeId) -> RetainedNodeDirtyFlags {
        self.node_keys
            .get(&node_id)
            .and_then(|key| self.dirty.get(*key))
            .copied()
            .unwrap_or_default()
    }

    #[cfg(test)]
    fn retained_key_for_node_id(&self, node_id: NodeId) -> Option<RetainedNodeKey> {
        self.node_keys.get(&node_id).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RetainedNodeSnapshot {
    layout: LayoutFingerprint,
    style_hash: u64,
    attributes_hash: u64,
    child_ids: Vec<NodeId>,
    state_hash: u64,
}

type LayoutFingerprint = (u32, u32, u32, u32);

impl RetainedNodeSnapshot {
    fn diff_flags(&self, next: &Self) -> RetainedNodeDirtyFlags {
        let mut flags = RetainedNodeDirtyFlags::empty();
        if self.layout != next.layout {
            flags |= RetainedNodeDirtyFlags::LAYOUT;
        }
        if self.style_hash != next.style_hash {
            flags |= RetainedNodeDirtyFlags::STYLE;
        }
        if self.attributes_hash != next.attributes_hash {
            flags |= RetainedNodeDirtyFlags::ATTRIBUTES;
        }
        if self.child_ids != next.child_ids {
            flags |= RetainedNodeDirtyFlags::CHILDREN;
        }
        if self.state_hash != next.state_hash {
            flags |= RetainedNodeDirtyFlags::STATE;
        }
        flags
    }
}

/// Deterministic runtime node id derived from the stable runtime key assigned
/// during annotation. This keeps node ids stable across full rebuilds when the
/// logical path is unchanged, which is the minimum identity contract needed for
/// a retained tree/render-object cache.
pub(super) fn stable_runtime_node_id(key: &str) -> NodeId {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    // Keep zero out of the generated id space so it remains available as a
    // sentinel for future retained-tree tables.
    if hash == 0 { 1 } else { hash }
}

fn collect_retained_snapshots(
    node: &WidgetNode,
    snapshots: &mut HashMap<NodeId, RetainedNodeSnapshot>,
) {
    snapshots.insert(node.id, retained_snapshot(node));
    for child in &node.children {
        collect_retained_snapshots(child, snapshots);
    }
}

fn retained_snapshot(node: &WidgetNode) -> RetainedNodeSnapshot {
    RetainedNodeSnapshot {
        layout: layout_fingerprint(node.layout),
        style_hash: style_fingerprint(&node.computed_style),
        attributes_hash: attributes_fingerprint(node),
        child_ids: node.children.iter().map(|child| child.id).collect(),
        state_hash: state_fingerprint(node.state),
    }
}

fn layout_fingerprint(layout: LayoutRect) -> LayoutFingerprint {
    (
        layout.x.to_bits(),
        layout.y.to_bits(),
        layout.width.to_bits(),
        layout.height.to_bits(),
    )
}

fn style_fingerprint(style: &ComputedStyle) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash_dimension(style.width, &mut hasher);
    hash_dimension(style.height, &mut hasher);
    hash_option_f32(style.min_width, &mut hasher);
    hash_option_f32(style.max_width, &mut hasher);
    hash_option_f32(style.min_height, &mut hasher);
    hash_option_f32(style.max_height, &mut hasher);
    hash_edges(style.padding, &mut hasher);
    hash_edges(style.margin, &mut hasher);
    hash_edges(style.border_width, &mut hasher);
    hash_color(style.background_color, &mut hasher);
    hash_color(style.border_color, &mut hasher);
    hash_corners(style.border_radius, &mut hasher);
    style.opacity.to_bits().hash(&mut hasher);
    hash_transform(style.transform, &mut hasher);
    hash_debug(&style.transition, &mut hasher);
    hash_debug(&style.animation, &mut hasher);
    hash_debug(&style.overflow_x, &mut hasher);
    hash_debug(&style.overflow_y, &mut hasher);
    style.font_family.hash(&mut hasher);
    style.font_size.to_bits().hash(&mut hasher);
    style.font_weight.hash(&mut hasher);
    hash_color(style.color, &mut hasher);
    hash_debug(&style.text_align, &mut hasher);
    style.line_height.to_bits().hash(&mut hasher);
    hash_debug(&style.font_style, &mut hasher);
    style.letter_spacing.to_bits().hash(&mut hasher);
    hash_debug(&style.text_overflow, &mut hasher);
    hash_debug(&style.text_direction, &mut hasher);
    hash_debug(&style.display, &mut hasher);
    hash_debug(&style.direction, &mut hasher);
    hash_debug(&style.justify_content, &mut hasher);
    hash_debug(&style.align_items, &mut hasher);
    hash_debug(&style.align_content, &mut hasher);
    style.gap.to_bits().hash(&mut hasher);
    style.flex_grow.to_bits().hash(&mut hasher);
    style.flex_shrink.to_bits().hash(&mut hasher);
    hash_dimension(style.flex_basis, &mut hasher);
    hash_debug(&style.flex_wrap, &mut hasher);
    hash_debug(&style.align_self, &mut hasher);
    hash_debug(&style.position, &mut hasher);
    style.z_index.hash(&mut hasher);
    hash_option_f32(style.inset_top, &mut hasher);
    hash_option_f32(style.inset_right, &mut hasher);
    hash_option_f32(style.inset_bottom, &mut hasher);
    hash_option_f32(style.inset_left, &mut hasher);
    hash_option_f32(style.icon_fill, &mut hasher);
    hash_option_f32(style.icon_weight, &mut hasher);
    hash_option_f32(style.icon_grade, &mut hasher);
    hash_option_f32(style.icon_optical_size, &mut hasher);
    hasher.finish()
}

fn attributes_fingerprint(node: &WidgetNode) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    node.tag.hash(&mut hasher);
    let mut attributes: Vec<_> = node.attributes.iter().collect();
    attributes.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (key, value) in attributes {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    let mut handlers: Vec<_> = node.event_handlers.iter().collect();
    handlers.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (event, handler) in handlers {
        event.hash(&mut hasher);
        handler.hash(&mut hasher);
    }
    hasher.finish()
}

fn state_fingerprint(state: ElementState) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    state.hovered.hash(&mut hasher);
    state.active.hash(&mut hasher);
    state.focused.hash(&mut hasher);
    state.focus_visible.hash(&mut hasher);
    state.disabled.hash(&mut hasher);
    state.checked.hash(&mut hasher);
    hasher.finish()
}

fn hash_dimension(value: Dimension, hasher: &mut impl Hasher) {
    match value {
        Dimension::Auto => 0u8.hash(hasher),
        Dimension::Px(px) => {
            1u8.hash(hasher);
            px.to_bits().hash(hasher);
        }
        Dimension::Percent(percent) => {
            2u8.hash(hasher);
            percent.to_bits().hash(hasher);
        }
        Dimension::Content => 3u8.hash(hasher),
    }
}

fn hash_edges(value: Edges, hasher: &mut impl Hasher) {
    value.top.to_bits().hash(hasher);
    value.right.to_bits().hash(hasher);
    value.bottom.to_bits().hash(hasher);
    value.left.to_bits().hash(hasher);
}

fn hash_corners(value: Corners, hasher: &mut impl Hasher) {
    value.top_left.to_bits().hash(hasher);
    value.top_right.to_bits().hash(hasher);
    value.bottom_right.to_bits().hash(hasher);
    value.bottom_left.to_bits().hash(hasher);
}

fn hash_color(value: Color, hasher: &mut impl Hasher) {
    value.r.hash(hasher);
    value.g.hash(hasher);
    value.b.hash(hasher);
    value.a.hash(hasher);
}

fn hash_transform(value: Transform2D, hasher: &mut impl Hasher) {
    value.translate_x.to_bits().hash(hasher);
    value.translate_y.to_bits().hash(hasher);
    value.scale_x.to_bits().hash(hasher);
    value.scale_y.to_bits().hash(hasher);
    value.rotation.to_bits().hash(hasher);
}

fn hash_option_f32(value: Option<f32>, hasher: &mut impl Hasher) {
    match value {
        Some(value) => {
            true.hash(hasher);
            value.to_bits().hash(hasher);
        }
        None => false.hash(hasher),
    }
}

fn hash_debug(value: &impl std::fmt::Debug, hasher: &mut impl Hasher) {
    format!("{value:?}").hash(hasher);
}

/// Collect every `_mesh_key` present in the fully built and restyled widget tree.
/// Used by `FrontendSurfaceComponent::prune_stale_interaction_targets` to determine
/// which interaction targets are still valid after a restyle.
pub(super) fn collect_all_keys(node: &WidgetNode, keys: &mut HashSet<String>) {
    if let Some(key) = node.attributes.get("_mesh_key") {
        keys.insert(key.clone());
    }
    for child in &node.children {
        collect_all_keys(child, keys);
    }
}

pub(super) fn collect_stateful_keys(node: &WidgetNode, keys: &mut HashSet<String>) {
    if node.state != ElementState::default()
        && let Some(key) = node.attributes.get("_mesh_key")
    {
        keys.insert(key.clone());
    }
    for child in &node.children {
        collect_stateful_keys(child, keys);
    }
}

pub(super) fn input_accepts_char(node: &WidgetNode, ch: char) -> bool {
    if ch.is_control() {
        return false;
    }

    match node.attributes.get("type").map(|value| value.as_str()) {
        Some("number") => ch.is_ascii_digit() || matches!(ch, '.' | '-'),
        _ => true,
    }
}

pub(super) fn collect_element_metrics(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    elements: &mut serde_json::Map<String, serde_json::Value>,
    refs: &mut serde_json::Map<String, serde_json::Value>,
) {
    let metrics = element_snapshot_json(node, offset_x, offset_y);
    let scroll_x = metrics
        .get("scroll_x")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0) as f32;
    let scroll_y = metrics
        .get("scroll_y")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0) as f32;

    if let Some(key) = node.attributes.get("_mesh_key") {
        elements.insert(key.clone(), metrics.clone());
    }
    if let Some(id) = node.attributes.get("id") {
        refs.insert(id.clone(), metrics.clone());
    }
    if let Some(reference) = node.attributes.get("ref") {
        refs.insert(reference.clone(), metrics);
    }

    let child_offset_x = offset_x - scroll_x;
    let child_offset_y = offset_y - scroll_y;
    for child in &node.children {
        collect_element_metrics(child, child_offset_x, child_offset_y, elements, refs);
    }
}

pub(super) fn annotate_runtime_tree(
    node: &mut WidgetNode,
    key: String,
    focused_key: &Option<String>,
    focus_visible_key: &Option<String>,
    hovered_path: &[String],
    active_key: &Option<String>,
    input_values: &HashMap<String, String>,
    slider_values: &HashMap<String, f32>,
    checked_values: &HashMap<String, bool>,
    scroll_offsets: &HashMap<String, ScrollOffsetState>,
) {
    node.id = stable_runtime_node_id(&key);
    node.attributes.insert("_mesh_key".into(), key.clone());

    let key_str = key.as_str();
    let disabled = node
        .attributes
        .get("disabled")
        .is_some_and(|value| truthy_attribute(value))
        || node
            .attributes
            .get("aria-disabled")
            .is_some_and(|value| truthy_attribute(value));
    let checked = checked_values
        .get(&key)
        .copied()
        .or_else(|| {
            node.attributes
                .get("checked")
                .map(|value| matches!(value.as_str(), "true" | "1" | "checked"))
        })
        .unwrap_or(false);

    node.state = ElementState {
        focused: focused_key.as_deref() == Some(key_str),
        focus_visible: focus_visible_key.as_deref() == Some(key_str)
            || (focus_visible_key.is_none()
                && focused_key.as_deref() == Some(key_str)
                && node.tag == "input"),
        hovered: hovered_path
            .iter()
            .any(|hovered_key| hovered_key == key_str),
        active: active_key.as_deref() == Some(key_str),
        disabled,
        checked,
    };
    if node.state.hovered {
        tracing::trace!(
            "[hover] annotate: key={key} tag={} set hovered=true",
            node.tag
        );
    }

    if node.state.focused {
        node.attributes
            .insert("_mesh_focused".into(), "true".into());
    }
    node.accessibility.focused = node.state.focused;

    match node.tag.as_str() {
        "input" => {
            let value = input_values
                .get(&key)
                .cloned()
                .or_else(|| node.attributes.get("value").cloned())
                .unwrap_or_default();
            node.attributes.insert("value".into(), value);
        }
        "slider" => {
            let value = slider_values
                .get(&key)
                .copied()
                .or_else(|| {
                    node.attributes
                        .get("value")
                        .and_then(|value: &String| value.parse::<f32>().ok())
                })
                .unwrap_or(0.0);
            node.attributes.insert("value".into(), value.to_string());
        }
        "switch" | "checkbox" => {
            node.attributes.insert(
                "checked".into(),
                if checked { "true" } else { "false" }.into(),
            );
        }
        _ => {}
    }

    let offset = scroll_offsets.get(&key).copied().unwrap_or_default();
    node.attributes
        .insert("_mesh_scroll_x".into(), format!("{:.2}", offset.x));
    node.attributes
        .insert("_mesh_scroll_y".into(), format!("{:.2}", offset.y));

    for (index, child) in node.children.iter_mut().enumerate() {
        annotate_runtime_tree(
            child,
            format!("{key}/{index}"),
            focused_key,
            focus_visible_key,
            hovered_path,
            active_key,
            input_values,
            slider_values,
            checked_values,
            scroll_offsets,
        );
    }
}

fn truthy_attribute(value: &str) -> bool {
    matches!(value, "" | "true" | "1" | "disabled" | "checked")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_runtime_node_id_is_deterministic_and_non_zero() {
        let first = stable_runtime_node_id("root/0/2");
        let second = stable_runtime_node_id("root/0/2");

        assert_ne!(first, 0);
        assert_eq!(first, second);
        assert_ne!(first, stable_runtime_node_id("root/0/3"));
    }

    #[test]
    fn annotate_runtime_tree_assigns_stable_ids_from_runtime_keys() {
        let mut first = WidgetNode::new("row");
        first.children.push(WidgetNode::new("button"));
        let mut second = WidgetNode::new("row");
        second.children.push(WidgetNode::new("button"));

        annotate_runtime_tree(
            &mut first,
            "root".to_string(),
            &None,
            &None,
            &[],
            &None,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        annotate_runtime_tree(
            &mut second,
            "root".to_string(),
            &None,
            &None,
            &[],
            &None,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(first.id, second.id);
        assert_eq!(first.children[0].id, second.children[0].id);
        assert_ne!(first.id, first.children[0].id);
    }

    #[test]
    fn retained_widget_tree_reports_dirty_categories_by_stable_id() {
        let mut tree = WidgetNode::new("row");
        tree.children.push(WidgetNode::new("button"));
        annotate_runtime_tree(
            &mut tree,
            "root".to_string(),
            &None,
            &None,
            &[],
            &None,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );

        let mut retained = RetainedWidgetTree::default();
        let first = retained.update(&tree);
        assert_eq!(first.inserted, 2);
        assert_eq!(retained.generation(), 1);
        let child_id = tree.children[0].id;
        let child_key = retained
            .retained_key_for_node_id(child_id)
            .expect("child should be stored in retained slotmap");
        assert_eq!(
            retained.dirty_flags_for(child_id),
            RetainedNodeDirtyFlags::INSERTED
        );

        let clean = retained.update(&tree);
        assert!(!clean.any());
        assert_eq!(retained.generation(), 1);
        assert_eq!(retained.retained_key_for_node_id(child_id), Some(child_key));
        assert!(retained.dirty_flags_for(child_id).is_empty());

        tree.children[0].layout.width = 42.0;
        tree.children[0].computed_style.background_color = Color::BLACK;
        tree.children[0]
            .attributes
            .insert("title".into(), "changed".into());
        tree.children[0].state.hovered = true;

        let dirty = retained.update(&tree);
        assert_eq!(dirty.layout, 1);
        assert_eq!(dirty.style, 1);
        assert_eq!(dirty.attributes, 1);
        assert_eq!(dirty.state, 1);
        assert_eq!(dirty.inserted, 0);
        assert_eq!(dirty.removed, 0);
        assert_eq!(retained.last_dirty(), dirty);
        assert_eq!(retained.generation(), 2);
        assert_eq!(retained.retained_key_for_node_id(child_id), Some(child_key));
        assert_eq!(
            retained.dirty_flags_for(child_id),
            RetainedNodeDirtyFlags::LAYOUT
                | RetainedNodeDirtyFlags::STYLE
                | RetainedNodeDirtyFlags::ATTRIBUTES
                | RetainedNodeDirtyFlags::STATE
        );
    }
}
