use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use bitflags::bitflags;
use mesh_core_elements::style::{Color, ComputedStyle, Corners, Dimension, Edges, Transform2D};
use mesh_core_elements::{ElementState, LayoutRect, NodeId, WidgetNode, element_snapshot_json};
use mesh_core_interaction::{ScrollOffsetState, node_is_source, source_element_tag};
use slotmap::{SecondaryMap, SlotMap, new_key_type};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RetainedTreeDirtySummary {
    pub(super) inserted: usize,
    pub(super) removed: usize,
    pub(super) layout: usize,
    pub(super) style: usize,
    pub(super) attributes: usize,
    pub(super) children: usize,
    pub(super) state: usize,
    /// Bitmask of state bits that flipped this frame (old_state ^ new_state),
    /// OR'd across all nodes that had STATE dirty. Zero if no state changed.
    /// Bits correspond to STATE_HOVERED, STATE_FOCUSED, STATE_ACTIVE, etc.
    pub(super) changed_state_bits: u32,
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
    // Scratch map reused each frame to avoid per-frame allocation.
    next_nodes_scratch: HashMap<NodeId, RetainedNodeSnapshot>,
}

impl RetainedWidgetTree {
    pub(super) fn update(&mut self, root: &WidgetNode) -> RetainedTreeDirtySummary {
        // Take the scratch map out so we can freely mutate other fields while holding it.
        let mut next_nodes = std::mem::take(&mut self.next_nodes_scratch);
        next_nodes.clear();
        collect_retained_snapshots(root, &mut next_nodes);

        let mut dirty = RetainedTreeDirtySummary::default();
        let mut next_dirty = SecondaryMap::new();

        for (&node_id, next) in &next_nodes {
            match self.node_keys.get(&node_id).copied() {
                Some(previous) => {
                    if let Some(previous_snapshot) = self.nodes.get(previous) {
                        let (flags, node_state_bits) = previous_snapshot.diff_flags(next);
                        if flags.is_empty() {
                            continue;
                        }
                        dirty.add_flags(flags);
                        dirty.changed_state_bits |= node_state_bits;
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

        // Remove nodes no longer in the tree — retain avoids the intermediate Vec.
        {
            let RetainedWidgetTree {
                ref mut nodes,
                ref mut node_keys,
                ..
            } = *self;
            node_keys.retain(|id, key| {
                if next_nodes.contains_key(id) {
                    return true;
                }
                nodes.remove(*key);
                dirty.removed += 1;
                false
            });
        }

        if dirty.any() {
            self.generation = self.generation.saturating_add(1);
        }
        self.dirty = next_dirty;
        self.last_dirty = dirty;

        // Return the scratch map, preserving its backing allocation for the next frame.
        self.next_nodes_scratch = next_nodes;
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

    pub(super) fn narrow_script_diff(&self, root: &WidgetNode) -> Option<(HashSet<NodeId>, usize)> {
        let mut fresh_snapshots = HashMap::new();
        collect_retained_snapshots(root, &mut fresh_snapshots);
        let total = fresh_snapshots.len();

        let mut affected = HashSet::new();
        for (&node_id, fresh) in &fresh_snapshots {
            let prev_key = match self.node_keys.get(&node_id).copied() {
                Some(key) => key,
                None => return None, // INSERTED
            };
            let prev = match self.nodes.get(prev_key) {
                Some(snap) => snap,
                None => return None, // INSERTED
            };
            let (flags, _) = prev.diff_flags(fresh);
            if flags.is_empty() {
                continue;
            }
            if flags.contains(RetainedNodeDirtyFlags::CHILDREN) {
                return None; // structural change
            }
            affected.insert(node_id);
        }
        Some((affected, total))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RetainedNodeSnapshot {
    layout: LayoutFingerprint,
    style_hash: u64,
    attributes_hash: u64,
    child_ids: Vec<NodeId>,
    state: ElementState,
}

type LayoutFingerprint = (u32, u32, u32, u32);

impl RetainedNodeSnapshot {
    fn diff_flags(&self, next: &Self) -> (RetainedNodeDirtyFlags, u32) {
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
        let changed_state_bits = if self.state != next.state {
            flags |= RetainedNodeDirtyFlags::STATE;
            state_bitmask(self.state) ^ state_bitmask(next.state)
        } else {
            0
        };
        (flags, changed_state_bits)
    }
}

struct RuntimeTreeHasher(u64);

impl Default for RuntimeTreeHasher {
    fn default() -> Self {
        Self(FNV_OFFSET)
    }
}

impl Hasher for RuntimeTreeHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }
}

/// Deterministic runtime node id derived from the stable runtime key assigned
/// during annotation. This keeps node ids stable across full rebuilds when the
/// logical path is unchanged, which is the minimum identity contract needed for
/// a retained tree/render-object cache.
pub(super) fn stable_runtime_node_id(key: &str) -> NodeId {
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
        state: node.state,
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
    let mut hasher = RuntimeTreeHasher::default();
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
    style.transition.hash(&mut hasher);
    style.animation.hash(&mut hasher);
    style.overflow_x.hash(&mut hasher);
    style.overflow_y.hash(&mut hasher);
    style.font_family.hash(&mut hasher);
    style.font_size.to_bits().hash(&mut hasher);
    style.font_weight.hash(&mut hasher);
    hash_color(style.color, &mut hasher);
    style.text_align.hash(&mut hasher);
    style.line_height.to_bits().hash(&mut hasher);
    style.font_style.hash(&mut hasher);
    style.letter_spacing.to_bits().hash(&mut hasher);
    style.text_overflow.hash(&mut hasher);
    style.text_direction.hash(&mut hasher);
    style.display.hash(&mut hasher);
    style.direction.hash(&mut hasher);
    style.justify_content.hash(&mut hasher);
    style.align_items.hash(&mut hasher);
    style.align_content.hash(&mut hasher);
    style.gap.to_bits().hash(&mut hasher);
    style.flex_grow.to_bits().hash(&mut hasher);
    style.flex_shrink.to_bits().hash(&mut hasher);
    hash_dimension(style.flex_basis, &mut hasher);
    style.flex_wrap.hash(&mut hasher);
    style.align_self.hash(&mut hasher);
    style.position.hash(&mut hasher);
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
    let mut hasher = RuntimeTreeHasher::default();
    node.tag.hash(&mut hasher);
    for (key, value) in &node.attributes {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    for (event, handler) in &node.event_handlers {
        event.hash(&mut hasher);
        handler.hash(&mut hasher);
    }
    hasher.finish()
}

/// Converts ElementState to a u32 bitmask using stable bit positions.
/// Bit positions mirror the style resolver's STATE_HOVERED, STATE_FOCUSED, etc. constants
/// and are kept self-contained here to avoid a cross-crate dependency on private constants.
fn state_bitmask(state: ElementState) -> u32 {
    let mut mask = 0u32;
    if state.hovered {
        mask |= 1 << 0;
    }
    if state.focused {
        mask |= 1 << 1;
    }
    if state.active {
        mask |= 1 << 2;
    }
    if state.disabled {
        mask |= 1 << 3;
    }
    if state.read_only {
        mask |= 1 << 4;
    }
    if state.required {
        mask |= 1 << 5;
    }
    if state.selected {
        mask |= 1 << 6;
    }
    if state.checked {
        mask |= 1 << 7;
    }
    if state.expanded {
        mask |= 1 << 8;
    }
    if state.pressed {
        mask |= 1 << 9;
    }
    if state.invalid {
        mask |= 1 << 10;
    }
    if state.value {
        mask |= 1 << 11;
    }
    if state.focus_visible {
        mask |= 1 << 12;
    }
    mask
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
    active_slider_key: &Option<String>,
    input_values: &HashMap<String, String>,
    slider_values: &mut HashMap<String, f32>,
    slider_script_values: &mut HashMap<String, f32>,
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
        ..ElementState::default()
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

    let source_tag = source_element_tag(node).to_string();
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
            let script_value = node
                .attributes
                .get("value")
                .and_then(|value: &String| value.parse::<f32>().ok());
            let preserved_value = slider_values.get(&key).copied();
            let value = if active_slider_key.as_deref() == Some(key_str) {
                preserved_value.or(script_value).unwrap_or(0.0)
            } else if let Some(script_value) = script_value {
                match (preserved_value, slider_script_values.get(&key).copied()) {
                    (Some(preserved), Some(previous_script))
                        if float_eq(script_value, previous_script) =>
                    {
                        preserved
                    }
                    (Some(preserved), None) => preserved,
                    (Some(_), Some(_)) => {
                        slider_values.remove(&key);
                        slider_script_values.remove(&key);
                        script_value
                    }
                    (None, _) => script_value,
                }
            } else {
                preserved_value.unwrap_or(0.0)
            };
            {
                use std::fmt::Write as _;
                let entry = node
                    .attributes
                    .entry("value".into())
                    .or_insert_with(String::new);
                entry.clear();
                let _ = write!(entry, "{:.2}", value);
            }
        }
        "switch" | "checkbox" => {
            node.attributes.insert(
                "checked".into(),
                if checked { "true" } else { "false" }.into(),
            );
        }
        _ => {}
    }

    if node_is_source(node, &["switch", "checkbox", "radio", "option"]) {
        node.attributes.insert(
            "checked".into(),
            if checked { "true" } else { "false" }.into(),
        );
        if matches!(source_tag.as_str(), "radio" | "option") {
            node.attributes.insert(
                "selected".into(),
                if checked { "true" } else { "false" }.into(),
            );
        }
        node.state.checked = checked;
        node.state.selected = checked;
        node.accessibility.state.checked = Some(checked);
        node.accessibility.state.selected = checked;
    }

    if node_is_source(node, &["select", "radio-group"])
        && let Some(value) = input_values
            .get(&key)
            .cloned()
            .or_else(|| node.attributes.get("value").cloned())
    {
        node.attributes.insert("value".into(), value.clone());
        node.state.value = true;
        node.accessibility.state.value = Some(value);
    }

    let offset = scroll_offsets.get(&key).copied().unwrap_or_default();
    {
        use std::fmt::Write as _;
        let ex = node
            .attributes
            .entry("_mesh_scroll_x".into())
            .or_insert_with(String::new);
        ex.clear();
        let _ = write!(ex, "{:.2}", offset.x);
        let ey = node
            .attributes
            .entry("_mesh_scroll_y".into())
            .or_insert_with(String::new);
        ey.clear();
        let _ = write!(ey, "{:.2}", offset.y);
    }

    for (index, child) in node.children.iter_mut().enumerate() {
        annotate_runtime_tree(
            child,
            format!("{key}/{index}"),
            focused_key,
            focus_visible_key,
            hovered_path,
            active_key,
            active_slider_key,
            input_values,
            slider_values,
            slider_script_values,
            checked_values,
            scroll_offsets,
        );
    }
}

fn float_eq(left: f32, right: f32) -> bool {
    (left - right).abs() <= f32::EPSILON
}

fn truthy_attribute(value: &str) -> bool {
    matches!(value, "" | "true" | "1" | "disabled" | "checked")
}

/// Bidirectional index from widget nodes to the service fields they read.
///
/// Built after each full `build_tree()` pass (not on targeted interaction restyle).
/// Answers both directions in O(1).
#[derive(Debug, Default)]
pub(super) struct NodeServiceFieldDependencies {
    /// node_id → set of (service, field) pairs that node reads
    forward: HashMap<NodeId, HashSet<(String, String)>>,
    /// (service, field) → set of node_ids that read it
    reverse: HashMap<(String, String), HashSet<NodeId>>,
}

impl NodeServiceFieldDependencies {
    /// Build the bidirectional index from a fully-annotated WidgetNode tree.
    /// Must be called after `annotate_runtime_tree()` so `node.id` values are stable.
    pub(super) fn build(root: &WidgetNode) -> Self {
        let mut deps = Self::default();
        collect_node_service_deps(root, &mut deps);
        deps
    }

    /// Returns node IDs that read `(service, field)`. Empty set if none.
    pub(super) fn nodes_reading_field(&self, service: &str, field: &str) -> &HashSet<NodeId> {
        static EMPTY: std::sync::OnceLock<HashSet<NodeId>> = std::sync::OnceLock::new();
        let key = (service.to_string(), field.to_string());
        self.reverse
            .get(&key)
            .unwrap_or_else(|| EMPTY.get_or_init(HashSet::new))
    }

    /// Returns `(service, field)` pairs that `node_id` reads. `None` if not tracked.
    pub(super) fn fields_read_by_node(
        &self,
        node_id: NodeId,
    ) -> Option<&HashSet<(String, String)>> {
        self.forward.get(&node_id)
    }
}

fn collect_node_service_deps(node: &WidgetNode, deps: &mut NodeServiceFieldDependencies) {
    if !node.service_field_reads.is_empty() {
        let entry = deps.forward.entry(node.id).or_default();
        for pair in &node.service_field_reads {
            entry.insert(pair.clone());
            deps.reverse
                .entry(pair.clone())
                .or_default()
                .insert(node.id);
        }
    }
    for child in &node.children {
        collect_node_service_deps(child, deps);
    }
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
            &None,
            &HashMap::new(),
            &mut HashMap::new(),
            &mut HashMap::new(),
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
            &None,
            &HashMap::new(),
            &mut HashMap::new(),
            &mut HashMap::new(),
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
            &None,
            &HashMap::new(),
            &mut HashMap::new(),
            &mut HashMap::new(),
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

    #[test]
    fn node_service_field_deps_forward_lookup() {
        let mut node = WidgetNode::new("text");
        node.service_field_reads
            .push(("audio".to_string(), "percent".to_string()));
        let id = node.id;
        let mut root = WidgetNode::new("column");
        root.children.push(node);

        let deps = NodeServiceFieldDependencies::build(&root);
        let fields = deps
            .fields_read_by_node(id)
            .expect("node should be tracked");
        assert!(fields.contains(&("audio".to_string(), "percent".to_string())));
    }

    #[test]
    fn node_service_field_deps_reverse_lookup() {
        let mut node = WidgetNode::new("text");
        node.service_field_reads
            .push(("audio".to_string(), "percent".to_string()));
        let id = node.id;
        let mut root = WidgetNode::new("column");
        root.children.push(node);

        let deps = NodeServiceFieldDependencies::build(&root);
        let nodes = deps.nodes_reading_field("audio", "percent");
        assert!(nodes.contains(&id));
    }

    #[test]
    fn node_service_field_deps_empty_node_not_in_forward() {
        let root = WidgetNode::new("column");
        let id = root.id;
        let deps = NodeServiceFieldDependencies::build(&root);
        assert!(deps.fields_read_by_node(id).is_none());
    }

    #[test]
    fn node_service_field_deps_unknown_field_empty() {
        let root = WidgetNode::new("column");
        let deps = NodeServiceFieldDependencies::build(&root);
        let result = deps.nodes_reading_field("bogus", "x");
        assert!(result.is_empty());
    }
}
