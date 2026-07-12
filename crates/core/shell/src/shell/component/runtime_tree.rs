use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use bitflags::bitflags;
use mesh_core_elements::style::{Color, ComputedStyle, Corners, Dimension, Edges, Transform2D};
use mesh_core_elements::{ElementState, NodeId, WidgetNode, element_snapshot_json};
use mesh_core_interaction::{ScrollOffsetState, node_is_source, source_element_tag};
use slotmap::{SecondaryMap, SlotMap, new_key_type};
use smallvec::SmallVec;

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
    // Dirty slots are transient but interaction frames repopulate them often.
    // Swap the previous map into scratch so its slot allocation is retained.
    next_dirty_scratch: SecondaryMap<RetainedNodeKey, RetainedNodeDirtyFlags>,
}

impl RetainedWidgetTree {
    pub(super) fn update(&mut self, root: &WidgetNode) -> RetainedTreeDirtySummary {
        let _span = tracing::debug_span!("retained_tree_update").entered();
        // Take the scratch map out so we can freely mutate other fields while holding it.
        let mut next_nodes = std::mem::take(&mut self.next_nodes_scratch);
        next_nodes.clear();
        collect_retained_snapshots(root, &mut next_nodes);

        let mut dirty = RetainedTreeDirtySummary::default();
        let mut next_dirty = std::mem::take(&mut self.next_dirty_scratch);
        next_dirty.clear();

        // Remove stale nodes before draining the scratch map. This lets the
        // update loop move changed snapshots into retained storage instead of
        // cloning them while still reusing the scratch map allocation.
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

        for (node_id, next) in next_nodes.drain() {
            match self.node_keys.get(&node_id).copied() {
                Some(previous) => {
                    if let Some(previous_snapshot) = self.nodes.get(previous) {
                        let (flags, node_state_bits) = previous_snapshot.diff_flags(&next);
                        if flags.is_empty() {
                            continue;
                        }
                        dirty.add_flags(flags);
                        dirty.changed_state_bits |= node_state_bits;
                        next_dirty.insert(previous, flags);
                        if let Some(slot) = self.nodes.get_mut(previous) {
                            *slot = next;
                        }
                    } else {
                        let key = self.nodes.insert(next);
                        self.node_keys.insert(node_id, key);
                        next_dirty.insert(key, RetainedNodeDirtyFlags::INSERTED);
                        dirty.inserted += 1;
                    }
                }
                None => {
                    let key = self.nodes.insert(next);
                    self.node_keys.insert(node_id, key);
                    next_dirty.insert(key, RetainedNodeDirtyFlags::INSERTED);
                    dirty.inserted += 1;
                }
            }
        }

        if dirty.any() {
            self.generation = self.generation.saturating_add(1);
        }
        let previous_dirty = std::mem::replace(&mut self.dirty, next_dirty);
        self.next_dirty_scratch = previous_dirty;
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
            let ancestor_only_flags =
                RetainedNodeDirtyFlags::LAYOUT | RetainedNodeDirtyFlags::ATTRIBUTES;
            if !fresh.child_ids.is_empty() && flags.difference(ancestor_only_flags).is_empty() {
                continue;
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
    child_ids: SmallVec<[NodeId; 8]>,
    state: ElementState,
}

type LayoutFingerprint = (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32);

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

    fn write_u8(&mut self, value: u8) {
        self.write_mix(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.write_mix(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.write_mix(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.write_mix(value);
    }

    fn write_u128(&mut self, value: u128) {
        self.write_mix(value as u64);
        self.write_mix((value >> 64) as u64);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_mix(value as u64);
    }

    fn write_i8(&mut self, value: i8) {
        self.write_mix(value as u8 as u64);
    }

    fn write_i16(&mut self, value: i16) {
        self.write_mix(value as u16 as u64);
    }

    fn write_i32(&mut self, value: i32) {
        self.write_mix(value as u32 as u64);
    }

    fn write_i64(&mut self, value: i64) {
        self.write_mix(value as u64);
    }

    fn write_i128(&mut self, value: i128) {
        self.write_u128(value as u128);
    }

    fn write_isize(&mut self, value: isize) {
        self.write_mix(value as usize as u64);
    }
}

impl RuntimeTreeHasher {
    #[inline]
    fn write_mix(&mut self, value: u64) {
        self.0 ^= value;
        self.0 = self.0.wrapping_mul(FNV_PRIME);
        self.0 ^= self.0 >> 32;
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

#[inline]
fn child_runtime_node_id(parent_id: NodeId, child_index: usize) -> NodeId {
    let mut hash = parent_id ^ 0x9e37_79b9_7f4a_7c15;
    hash ^= (child_index as u64).wrapping_add(1);
    hash = hash.wrapping_mul(FNV_PRIME);
    hash ^= hash >> 32;
    if hash == 0 { 1 } else { hash }
}

fn collect_retained_snapshots(
    node: &WidgetNode,
    snapshots: &mut HashMap<NodeId, RetainedNodeSnapshot>,
) {
    #[cfg(not(debug_assertions))]
    snapshots.insert(node.id, retained_snapshot(node));

    #[cfg(debug_assertions)]
    let previous = snapshots.insert(node.id, retained_snapshot(node));

    #[cfg(debug_assertions)]
    {
        assert!(
            previous.is_none(),
            "runtime NodeId collision while collecting retained snapshots: id={} key={:?}",
            node.id,
            node.attributes.get("_mesh_key")
        );
    }
    for child in &node.children {
        collect_retained_snapshots(child, snapshots);
    }
}

fn retained_snapshot(node: &WidgetNode) -> RetainedNodeSnapshot {
    RetainedNodeSnapshot {
        layout: layout_fingerprint(node),
        style_hash: style_fingerprint(&node.computed_style),
        attributes_hash: attributes_fingerprint(node),
        child_ids: node.children.iter().map(|child| child.id).collect(),
        state: node.state,
    }
}

fn layout_fingerprint(node: &WidgetNode) -> LayoutFingerprint {
    let layout = node.layout;
    let scroll = node.resolved_scroll_metrics();
    (
        layout.x.to_bits(),
        layout.y.to_bits(),
        layout.width.to_bits(),
        layout.height.to_bits(),
        scroll.x.to_bits(),
        scroll.y.to_bits(),
        scroll.max_x.to_bits(),
        scroll.max_y.to_bits(),
        scroll.content_width.to_bits(),
        scroll.content_height.to_bits(),
    )
}

fn style_fingerprint(style: &ComputedStyle) -> u64 {
    let mut hasher = RuntimeTreeHasher::default();
    hash_style_fields(style, &mut hasher);
    hasher.finish()
}

fn hash_style_fields(style: &ComputedStyle, hasher: &mut impl Hasher) {
    hash_dimension(style.width, hasher);
    hash_dimension(style.height, hasher);
    hash_option_f32(style.min_width, hasher);
    hash_option_f32(style.max_width, hasher);
    hash_option_f32(style.min_height, hasher);
    hash_option_f32(style.max_height, hasher);
    hash_edges(style.padding, hasher);
    hash_edges(style.margin, hasher);
    hash_edges(style.border_width, hasher);
    hash_color(style.background_color, hasher);
    hash_color(style.border_color, hasher);
    hash_corners(style.border_radius, hasher);
    style.opacity.to_bits().hash(hasher);
    hash_transform(style.transform, hasher);
    style.transitions.hash(hasher);
    style.animations.hash(hasher);
    style.overflow_x.hash(hasher);
    style.overflow_y.hash(hasher);
    style.font_family.hash(hasher);
    style.font_size.to_bits().hash(hasher);
    style.font_weight.hash(hasher);
    hash_color(style.color, hasher);
    style.text_align.hash(hasher);
    style.line_height.to_bits().hash(hasher);
    style.font_style.hash(hasher);
    style.letter_spacing.to_bits().hash(hasher);
    style.text_overflow.hash(hasher);
    style.text_direction.hash(hasher);
    style.display.hash(hasher);
    style.direction.hash(hasher);
    style.justify_content.hash(hasher);
    style.align_items.hash(hasher);
    style.align_content.hash(hasher);
    style.gap.to_bits().hash(hasher);
    style.flex_grow.to_bits().hash(hasher);
    style.flex_shrink.to_bits().hash(hasher);
    hash_dimension(style.flex_basis, hasher);
    style.flex_wrap.hash(hasher);
    style.align_self.hash(hasher);
    style.position.hash(hasher);
    style.mix_blend_mode.hash(hasher);
    style.z_index.hash(hasher);
    hash_option_f32(style.inset_top, hasher);
    hash_option_f32(style.inset_right, hasher);
    hash_option_f32(style.inset_bottom, hasher);
    hash_option_f32(style.inset_left, hasher);
    hash_option_f32(style.icon_fill, hasher);
    hash_option_f32(style.icon_weight, hasher);
    hash_option_f32(style.icon_grade, hasher);
    hash_option_f32(style.icon_optical_size, hasher);
}

fn attributes_fingerprint(node: &WidgetNode) -> u64 {
    let mut hasher = RuntimeTreeHasher::default();
    node.tag.hash(&mut hasher);
    for (key, value) in &node.attributes {
        // Runtime identity is already represented by `node.id`. Rehashing the
        // full slash-joined path here makes deep trees pay for the same
        // identity twice on every retained snapshot.
        if matches!(key.as_str(), "_mesh_key" | "_mesh_focused") {
            continue;
        }
        if key == "content" && !node.children.is_empty() {
            continue;
        }
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    for (event, handler) in &node.event_handlers {
        event.hash(&mut hasher);
        handler.hash(&mut hasher);
    }
    for (event, call) in &node.event_handler_calls {
        event.hash(&mut hasher);
        call.handler.hash(&mut hasher);
        for arg in &call.args {
            hash_json_value(arg, &mut hasher);
        }
    }
    hasher.finish()
}

fn hash_json_value(value: &serde_json::Value, hasher: &mut impl Hasher) {
    match value {
        serde_json::Value::Null => 0u8.hash(hasher),
        serde_json::Value::Bool(value) => {
            1u8.hash(hasher);
            value.hash(hasher);
        }
        serde_json::Value::Number(value) => {
            2u8.hash(hasher);
            if let Some(value) = value.as_i64() {
                0u8.hash(hasher);
                value.hash(hasher);
            } else if let Some(value) = value.as_u64() {
                1u8.hash(hasher);
                value.hash(hasher);
            } else if let Some(value) = value.as_f64() {
                2u8.hash(hasher);
                value.to_bits().hash(hasher);
            } else {
                3u8.hash(hasher);
                value.to_string().hash(hasher);
            }
        }
        serde_json::Value::String(value) => {
            3u8.hash(hasher);
            value.hash(hasher);
        }
        serde_json::Value::Array(values) => {
            4u8.hash(hasher);
            values.len().hash(hasher);
            for value in values {
                hash_json_value(value, hasher);
            }
        }
        serde_json::Value::Object(values) => {
            5u8.hash(hasher);
            values.len().hash(hasher);
            for (key, value) in values {
                key.hash(hasher);
                hash_json_value(value, hasher);
            }
        }
    }
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
    collect_elements: bool,
    collect_refs: bool,
    elements: &mut serde_json::Map<String, serde_json::Value>,
    refs: &mut serde_json::Map<String, serde_json::Value>,
    ref_keys: &mut HashMap<String, String>,
) {
    let node_key = node.attributes.get("_mesh_key");
    let publishes_element = collect_elements && node_key.is_some();
    let publishes_ref = collect_refs
        && (node.attributes.contains_key("id")
            || node.attributes.contains_key("ref")
            || node.attributes.contains_key("_mesh_bind_this"));

    let metrics = (publishes_element || publishes_ref)
        .then(|| element_snapshot_json(node, offset_x, offset_y));

    if collect_elements && let (Some(key), Some(metrics)) = (node_key, metrics.as_ref()) {
        elements.insert(key.clone(), metrics.clone());
    }
    // Map each `refs.<name>` to the node's runtime key so imperative element
    // actions (focus/blur/…) can resolve a name back to the live widget node.
    if collect_refs && let Some(metrics) = metrics.as_ref() {
        if let Some(id) = node.attributes.get("id") {
            refs.insert(id.clone(), metrics.clone());
            if let Some(key) = node_key {
                ref_keys.insert(id.clone(), key.clone());
            }
        }
        if let Some(reference) = node.attributes.get("ref") {
            refs.insert(reference.clone(), metrics.clone());
            if let Some(key) = node_key {
                ref_keys.insert(reference.clone(), key.clone());
            }
        }
        if let Some(binding) = node.attributes.get("_mesh_bind_this") {
            refs.insert(binding.clone(), metrics.clone());
            if let Some(key) = node_key {
                ref_keys.insert(binding.clone(), key.clone());
            }
        }
    }

    let scroll = node.resolved_scroll_metrics();
    let scroll_x = scroll.x;
    let scroll_y = scroll.y;
    let child_offset_x = offset_x - scroll_x;
    let child_offset_y = offset_y - scroll_y;
    for child in &node.children {
        collect_element_metrics(
            child,
            child_offset_x,
            child_offset_y,
            collect_elements,
            collect_refs,
            elements,
            refs,
            ref_keys,
        );
    }
}

pub(super) struct RuntimeAnnotationContext<'a> {
    focused_key: &'a Option<String>,
    focus_visible_key: &'a Option<String>,
    hovered_path: &'a [String],
    active_key: &'a Option<String>,
    active_slider_key: &'a Option<String>,
    input_values: &'a HashMap<String, String>,
    slider_values: &'a mut HashMap<String, f32>,
    slider_script_values: &'a mut HashMap<String, f32>,
    checked_values: &'a HashMap<String, bool>,
    scroll_offsets: &'a HashMap<String, ScrollOffsetState>,
}

impl<'a> RuntimeAnnotationContext<'a> {
    pub(super) fn new(
        focused_key: &'a Option<String>,
        focus_visible_key: &'a Option<String>,
        hovered_path: &'a [String],
        active_key: &'a Option<String>,
        active_slider_key: &'a Option<String>,
        input_values: &'a HashMap<String, String>,
        slider_values: &'a mut HashMap<String, f32>,
        slider_script_values: &'a mut HashMap<String, f32>,
        checked_values: &'a HashMap<String, bool>,
        scroll_offsets: &'a HashMap<String, ScrollOffsetState>,
    ) -> Self {
        Self {
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
        }
    }
}

pub(super) fn annotate_runtime_tree(
    node: &mut WidgetNode,
    key: String,
    context: &mut RuntimeAnnotationContext<'_>,
) {
    let node_id = stable_runtime_node_id(&key);
    let mut key = key;
    annotate_runtime_tree_inner(node, &mut key, node_id, context);
}

fn annotate_runtime_tree_inner(
    node: &mut WidgetNode,
    key: &mut String,
    node_id: NodeId,
    context: &mut RuntimeAnnotationContext<'_>,
) {
    node.id = node_id;
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
    let checked = context
        .checked_values
        .get(key_str)
        .copied()
        .or_else(|| {
            node.attributes
                .get("checked")
                .map(|value| matches!(value.as_str(), "true" | "1" | "checked"))
        })
        .unwrap_or(false);

    node.state = ElementState {
        focused: context.focused_key.as_deref() == Some(key_str),
        focus_visible: context.focus_visible_key.as_deref() == Some(key_str)
            || (context.focus_visible_key.is_none()
                && context.focused_key.as_deref() == Some(key_str)
                && node.tag == "input"),
        hovered: context
            .hovered_path
            .iter()
            .any(|hovered_key| hovered_key == key_str),
        active: context.active_key.as_deref() == Some(key_str),
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
            let value = context
                .input_values
                .get(key_str)
                .cloned()
                .or_else(|| node.attributes.get("value").cloned())
                .unwrap_or_default();
            node.attributes.insert("value".into(), value);
        }
        "slider" => {
            annotate_slider_node(node, key_str, key_str, context);
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
        && let Some(value) = context
            .input_values
            .get(key_str)
            .cloned()
            .or_else(|| node.attributes.get("value").cloned())
    {
        node.attributes.insert("value".into(), value.clone());
        node.state.value = true;
        node.accessibility.state.value = Some(value);
    }

    let offset = context
        .scroll_offsets
        .get(key_str)
        .copied()
        .unwrap_or_default();
    let scroll = node.scroll_metrics.get_or_insert_default();
    scroll.x = offset.x;
    scroll.y = offset.y;

    for (index, child) in node.children.iter_mut().enumerate() {
        let previous_len = key.len();
        {
            use std::fmt::Write as _;
            let _ = write!(key, "/{index}");
        }
        annotate_runtime_tree_inner(child, key, child_runtime_node_id(node_id, index), context);
        key.truncate(previous_len);
    }
}

fn annotate_slider_node(
    node: &mut WidgetNode,
    key: &str,
    key_str: &str,
    context: &mut RuntimeAnnotationContext<'_>,
) {
    let script_value = node
        .attributes
        .get("value")
        .and_then(|value: &String| value.parse::<f32>().ok());
    let value = resolved_slider_value(key, key_str, script_value, context);
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

fn resolved_slider_value(
    key: &str,
    key_str: &str,
    script_value: Option<f32>,
    context: &mut RuntimeAnnotationContext<'_>,
) -> f32 {
    let preserved_value = context.slider_values.get(key).copied();
    if context.active_slider_key.as_deref() == Some(key_str) {
        return preserved_value.or(script_value).unwrap_or(0.0);
    }

    if let Some(script_value) = script_value {
        match (
            preserved_value,
            context.slider_script_values.get(key).copied(),
        ) {
            (Some(preserved), Some(previous_script)) if float_eq(script_value, previous_script) => {
                preserved
            }
            (Some(preserved), None) => preserved,
            (Some(_), Some(_)) => {
                context.slider_values.remove(key);
                context.slider_script_values.remove(key);
                script_value
            }
            (None, _) => script_value,
        }
    } else {
        preserved_value.unwrap_or(0.0)
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
    use std::time::Instant;

    fn annotate_with_empty_context(node: &mut WidgetNode) {
        let input_values = HashMap::new();
        let mut slider_values = HashMap::new();
        let mut slider_script_values = HashMap::new();
        let checked_values = HashMap::new();
        let scroll_offsets = HashMap::new();
        let mut context = RuntimeAnnotationContext::new(
            &None,
            &None,
            &[],
            &None,
            &None,
            &input_values,
            &mut slider_values,
            &mut slider_script_values,
            &checked_values,
            &scroll_offsets,
        );
        annotate_runtime_tree(node, "root".to_string(), &mut context);
    }

    #[derive(Default)]
    struct ByteOnlyRuntimeTreeHasher(u64);

    impl Hasher for ByteOnlyRuntimeTreeHasher {
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

    fn benchmark_style() -> ComputedStyle {
        let mut style = ComputedStyle::default();
        style.width = Dimension::Px(960.0);
        style.height = Dimension::Percent(100.0);
        style.min_width = Some(24.0);
        style.max_width = Some(1200.0);
        style.padding = Edges {
            top: 4.0,
            right: 8.0,
            bottom: 4.0,
            left: 8.0,
        };
        style.margin = Edges {
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
            left: 4.0,
        };
        style.border_width = Edges {
            top: 1.0,
            right: 1.0,
            bottom: 1.0,
            left: 1.0,
        };
        style.background_color = Color::BLACK;
        style.border_color = Color::WHITE;
        style.border_radius = Corners {
            top_left: 6.0,
            top_right: 7.0,
            bottom_right: 8.0,
            bottom_left: 9.0,
        };
        style.opacity = 0.87;
        style.font_size = 13.0;
        style.line_height = 18.0;
        style.letter_spacing = 0.3;
        style.gap = 6.0;
        style.flex_grow = 1.0;
        style.flex_shrink = 0.0;
        style.flex_basis = Dimension::Content;
        style.inset_top = Some(2.0);
        style.inset_right = Some(3.0);
        style.inset_bottom = Some(4.0);
        style.inset_left = Some(5.0);
        style.icon_fill = Some(1.0);
        style.icon_weight = Some(400.0);
        style.icon_grade = Some(0.0);
        style.icon_optical_size = Some(20.0);
        style
    }

    // cargo test -p mesh-core-shell --release -- runtime_tree_primitive_hashing_beats_byte_fallback --ignored --nocapture
    #[test]
    #[ignore = "release-only retained-tree fingerprint microbenchmark"]
    fn runtime_tree_primitive_hashing_beats_byte_fallback() {
        let style = benchmark_style();
        let iterations = 500_000;

        let old_started = Instant::now();
        let mut old_accumulator = 0u64;
        for _ in 0..iterations {
            let mut hasher = ByteOnlyRuntimeTreeHasher(FNV_OFFSET);
            hash_style_fields(std::hint::black_box(&style), &mut hasher);
            old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_accumulator = 0u64;
        for _ in 0..iterations {
            new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(
                style_fingerprint(std::hint::black_box(&style)),
            ));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "runtime tree style fingerprint byte fallback: {old_time:?}; primitive-aware: {new_time:?}; ratio: {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_ne!(old_accumulator, 0);
        assert_ne!(new_accumulator, 0);
        assert!(new_time * 5 < old_time * 4);
    }

    #[test]
    fn stable_runtime_node_id_is_deterministic_and_non_zero() {
        let first = stable_runtime_node_id("root/0/2");
        let second = stable_runtime_node_id("root/0/2");

        assert_ne!(first, 0);
        assert_eq!(first, second);
        assert_ne!(first, stable_runtime_node_id("root/0/3"));
    }

    #[test]
    fn chained_runtime_node_ids_are_deterministic_and_distinguish_siblings() {
        let parent = stable_runtime_node_id("root/0");
        assert_eq!(
            child_runtime_node_id(parent, 2),
            child_runtime_node_id(parent, 2)
        );
        assert_ne!(
            child_runtime_node_id(parent, 2),
            child_runtime_node_id(parent, 3)
        );
        assert_ne!(child_runtime_node_id(parent, 2), 0);
    }

    // cargo test -p mesh-core-shell --release -- chained_runtime_ids_beat_rehashing_deep_paths --ignored --nocapture
    #[test]
    #[ignore = "release-only runtime node id microbenchmark"]
    fn chained_runtime_ids_beat_rehashing_deep_paths() {
        let paths = (0..10)
            .scan("root".to_string(), |path, index| {
                path.push('/');
                path.push_str(&index.to_string());
                Some(path.clone())
            })
            .collect::<Vec<_>>();
        let iterations = 500_000;

        let old_started = Instant::now();
        let mut old_accumulator = 0u64;
        for _ in 0..iterations {
            for path in &paths {
                old_accumulator ^= stable_runtime_node_id(std::hint::black_box(path));
            }
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_accumulator = 0u64;
        for _ in 0..iterations {
            let mut parent = stable_runtime_node_id("root");
            for index in 0..paths.len() {
                parent = child_runtime_node_id(parent, index);
                new_accumulator ^= std::hint::black_box(parent);
            }
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "runtime node ids: full-path hash {old_time:?}; parent-chain {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }

    #[test]
    fn attribute_fingerprint_uses_node_id_instead_of_runtime_key_string() {
        let mut node = WidgetNode::new("box");
        node.id = stable_runtime_node_id("root/0");
        node.attributes.insert("_mesh_key".into(), "root/0".into());
        node.attributes.insert("class".into(), "card".into());
        let original = attributes_fingerprint(&node);

        node.attributes
            .insert("_mesh_key".into(), "different/debug/path".into());
        assert_eq!(attributes_fingerprint(&node), original);

        node.attributes.insert("class".into(), "card active".into());
        assert_ne!(attributes_fingerprint(&node), original);
    }

    #[test]
    fn attribute_fingerprint_ignores_redundant_focused_annotation() {
        let mut node = WidgetNode::new("input");
        node.attributes.insert("_mesh_key".into(), "root/0".into());
        node.attributes.insert("value".into(), "hello".into());
        let original = attributes_fingerprint(&node);

        node.attributes
            .insert("_mesh_focused".into(), "true".into());
        assert_eq!(attributes_fingerprint(&node), original);

        node.attributes.insert("value".into(), "world".into());
        assert_ne!(attributes_fingerprint(&node), original);
    }

    #[test]
    fn attribute_fingerprint_tracks_typed_handler_arg_changes() {
        let mut node = WidgetNode::new("button");
        node.event_handler_calls.insert(
            "click".into(),
            mesh_core_elements::EventHandlerCall {
                handler: "select".into(),
                args: vec![serde_json::json!({
                    "id": "alpha",
                    "meta": { "index": 1, "enabled": true },
                    "tags": ["a", "b"]
                })],
            },
        );
        let original = attributes_fingerprint(&node);

        node.event_handler_calls
            .get_mut("click")
            .expect("call")
            .args[0]["meta"]["index"] = serde_json::json!(2);

        assert_ne!(attributes_fingerprint(&node), original);
    }

    // cargo test -p mesh-core-shell --release -- typed_json_arg_hashing_beats_to_string_fingerprint --ignored --nocapture
    #[test]
    #[ignore = "release-only JSON handler arg fingerprint microbenchmark"]
    fn typed_json_arg_hashing_beats_to_string_fingerprint() {
        fn old_hash_json_value(value: &serde_json::Value, hasher: &mut impl Hasher) {
            value.to_string().hash(hasher);
        }

        let arg = serde_json::json!({
            "id": "alpha",
            "meta": {
                "index": 42,
                "enabled": true,
                "ratio": 0.875,
                "label": "A moderately long label used by a pre-bound handler"
            },
            "tags": ["audio", "primary", "interactive", "toolbar"],
            "bounds": { "x": 10, "y": 20, "width": 140, "height": 32 }
        });
        let iterations = 500_000;

        let old_started = Instant::now();
        let mut old_accumulator = 0u64;
        for _ in 0..iterations {
            let mut hasher = RuntimeTreeHasher::default();
            old_hash_json_value(std::hint::black_box(&arg), &mut hasher);
            old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_accumulator = 0u64;
        for _ in 0..iterations {
            let mut hasher = RuntimeTreeHasher::default();
            hash_json_value(std::hint::black_box(&arg), &mut hasher);
            new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "typed JSON arg fingerprint: to_string {old_time:?}; direct hash {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_ne!(old_accumulator, 0);
        assert_ne!(new_accumulator, 0);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- focused_annotation_skip_beats_redundant_attribute_hash --ignored --nocapture
    #[test]
    #[ignore = "release-only focused annotation fingerprint microbenchmark"]
    fn focused_annotation_skip_beats_redundant_attribute_hash() {
        fn old_attributes_fingerprint(node: &WidgetNode) -> u64 {
            let mut hasher = RuntimeTreeHasher::default();
            node.tag.hash(&mut hasher);
            for (key, value) in &node.attributes {
                if key == "_mesh_key" {
                    continue;
                }
                key.hash(&mut hasher);
                value.hash(&mut hasher);
            }
            hasher.finish()
        }

        let mut node = WidgetNode::new("input");
        node.attributes.insert("_mesh_key".into(), "root/0".into());
        node.attributes
            .insert("_mesh_focused".into(), "true".into());
        node.attributes
            .insert("value".into(), "active field".into());
        node.attributes.insert("placeholder".into(), "Name".into());
        let iterations = 2_000_000;

        let old_started = Instant::now();
        let mut old_accumulator = 0u64;
        for _ in 0..iterations {
            old_accumulator = old_accumulator
                .wrapping_add(old_attributes_fingerprint(std::hint::black_box(&node)));
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_accumulator = 0u64;
        for _ in 0..iterations {
            new_accumulator =
                new_accumulator.wrapping_add(attributes_fingerprint(std::hint::black_box(&node)));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "focused annotation fingerprint: redundant attribute hash {old_time:?}; skipped {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_ne!(old_accumulator, 0);
        assert_ne!(new_accumulator, 0);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- attribute_fingerprint_skips_redundant_runtime_key_hash --ignored --nocapture
    #[test]
    #[ignore = "release-only attribute fingerprint microbenchmark"]
    fn attribute_fingerprint_skips_redundant_runtime_key_hash() {
        fn old_attributes_fingerprint(node: &WidgetNode) -> u64 {
            let mut hasher = RuntimeTreeHasher::default();
            node.tag.hash(&mut hasher);
            for (key, value) in &node.attributes {
                key.hash(&mut hasher);
                value.hash(&mut hasher);
            }
            hasher.finish()
        }

        let mut node = WidgetNode::new("box");
        node.id = stable_runtime_node_id("root/0/1/2/3/4/5/6/7/8/9");
        node.attributes
            .insert("_mesh_key".into(), "root/0/1/2/3/4/5/6/7/8/9".into());
        node.attributes.insert("class".into(), "card active".into());
        node.attributes.insert("role".into(), "button".into());
        let iterations = 2_000_000;

        let old_started = Instant::now();
        let mut old_accumulator = 0u64;
        for _ in 0..iterations {
            old_accumulator = old_accumulator
                .wrapping_add(old_attributes_fingerprint(std::hint::black_box(&node)));
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_accumulator = 0u64;
        for _ in 0..iterations {
            new_accumulator =
                new_accumulator.wrapping_add(attributes_fingerprint(std::hint::black_box(&node)));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "attribute fingerprint: runtime-key hash {old_time:?}; node-id identity {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }

    #[test]
    fn retained_snapshot_keeps_common_child_lists_inline() {
        let mut node = WidgetNode::new("row");
        node.children = (0..8)
            .map(|index| {
                let mut child = WidgetNode::new("box");
                child.id = index + 1;
                child
            })
            .collect();

        let snapshot = retained_snapshot(&node);
        assert_eq!(snapshot.child_ids.len(), 8);
        assert!(!snapshot.child_ids.spilled());

        node.children.push(WidgetNode::new("box"));
        assert!(retained_snapshot(&node).child_ids.spilled());
    }

    // cargo test -p mesh-core-shell --release -- inline_child_ids_beat_fresh_vec_allocations --ignored --nocapture
    #[test]
    #[ignore = "release-only retained child-id allocation microbenchmark"]
    fn inline_child_ids_beat_fresh_vec_allocations() {
        let child_ids = [11_u64, 12, 13, 14];
        let iterations = 2_000_000;

        let old_started = Instant::now();
        let mut old_total = 0u64;
        for _ in 0..iterations {
            let ids = child_ids.iter().copied().collect::<Vec<NodeId>>();
            old_total = old_total.wrapping_add(std::hint::black_box(ids)[0]);
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0u64;
        for _ in 0..iterations {
            let ids = child_ids.iter().copied().collect::<SmallVec<[NodeId; 8]>>();
            new_total = new_total.wrapping_add(std::hint::black_box(ids)[0]);
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "retained child ids: Vec {old_time:?}; inline SmallVec {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- reused_dirty_secondary_map_beats_fresh_allocations --ignored --nocapture
    #[test]
    #[ignore = "release-only retained dirty-map allocation microbenchmark"]
    fn reused_dirty_secondary_map_beats_fresh_allocations() {
        let mut nodes: SlotMap<RetainedNodeKey, RetainedNodeSnapshot> = SlotMap::with_key();
        let keys = (0..128)
            .map(|_| {
                nodes.insert(RetainedNodeSnapshot {
                    layout: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
                    style_hash: 0,
                    attributes_hash: 0,
                    child_ids: SmallVec::new(),
                    state: ElementState::default(),
                })
            })
            .collect::<Vec<_>>();
        let iterations = 20_000;

        let old_started = Instant::now();
        let mut old_count = 0;
        for _ in 0..iterations {
            let mut dirty = SecondaryMap::new();
            for key in &keys {
                dirty.insert(*key, RetainedNodeDirtyFlags::STATE);
            }
            old_count += std::hint::black_box(dirty.len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_count = 0;
        let mut dirty = SecondaryMap::new();
        for _ in 0..iterations {
            dirty.clear();
            for key in &keys {
                dirty.insert(*key, RetainedNodeDirtyFlags::STATE);
            }
            new_count += std::hint::black_box(dirty.len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "retained dirty map: fresh {old_time:?}; reused {new_time:?}; ratio {:.1}x; counts={old_count}/{new_count}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_count, new_count);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- drained_retained_snapshot_map_beats_clone_transfer --ignored --nocapture
    #[test]
    #[ignore = "release-only retained snapshot update microbenchmark"]
    fn drained_retained_snapshot_map_beats_clone_transfer() {
        let snapshots = (0..256_u64)
            .map(|index| {
                let mut child_ids = SmallVec::<[NodeId; 8]>::new();
                child_ids.extend((0..6).map(|child| index * 16 + child));
                (
                    index,
                    RetainedNodeSnapshot {
                        layout: (index as u32, 1, 2, 3, 0, 0, 0, 0, 0, 0),
                        style_hash: index.wrapping_mul(31),
                        attributes_hash: index.wrapping_mul(131),
                        child_ids,
                        state: ElementState {
                            hovered: index % 2 == 0,
                            focused: index % 3 == 0,
                            ..ElementState::default()
                        },
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let iterations = 20_000;

        let clone_started = Instant::now();
        let mut clone_total = 0usize;
        for _ in 0..iterations {
            let source = snapshots.clone();
            let mut slots = HashMap::with_capacity(source.len());
            for (&id, snapshot) in &source {
                slots.insert(id, snapshot.clone());
            }
            clone_total += std::hint::black_box(slots.len());
        }
        let clone_time = clone_started.elapsed();

        let move_started = Instant::now();
        let mut move_total = 0usize;
        for _ in 0..iterations {
            let mut source = snapshots.clone();
            let mut slots = HashMap::with_capacity(source.len());
            for (id, snapshot) in source.drain() {
                slots.insert(id, snapshot);
            }
            move_total += std::hint::black_box(slots.len());
        }
        let move_time = move_started.elapsed();

        eprintln!(
            "retained snapshot map transfer: clone {clone_time:?}; drain-move {move_time:?}; ratio {:.1}x; counts={clone_total}/{move_total}",
            clone_time.as_secs_f64() / move_time.as_secs_f64()
        );
        assert_eq!(clone_total, move_total);
        assert!(move_time < clone_time);
    }

    #[test]
    fn annotate_runtime_tree_assigns_stable_ids_from_runtime_keys() {
        let mut first = WidgetNode::new("row");
        first.children.push(WidgetNode::new("button"));
        let mut second = WidgetNode::new("row");
        second.children.push(WidgetNode::new("button"));

        annotate_with_empty_context(&mut first);
        annotate_with_empty_context(&mut second);

        assert_eq!(first.id, second.id);
        assert_eq!(first.children[0].id, second.children[0].id);
        assert_ne!(first.id, first.children[0].id);
        assert_eq!(
            first.attributes.get("_mesh_key").map(String::as_str),
            Some("root")
        );
        assert_eq!(
            first.children[0]
                .attributes
                .get("_mesh_key")
                .map(String::as_str),
            Some("root/0")
        );
    }

    // cargo test -p mesh-core-shell --release -- mutable_runtime_key_paths_beat_format_per_child --ignored --nocapture
    #[test]
    #[ignore = "release-only runtime key path construction microbenchmark"]
    fn mutable_runtime_key_paths_beat_format_per_child() {
        fn old_sum_paths(key: String, width: usize, depth: usize) -> usize {
            let mut total = key.len();
            if depth > 0 {
                for index in 0..width {
                    total += old_sum_paths(format!("{key}/{index}"), width, depth - 1);
                }
            }
            total
        }

        fn new_sum_paths(key: &mut String, width: usize, depth: usize) -> usize {
            let mut total = key.len();
            if depth > 0 {
                for index in 0..width {
                    let previous_len = key.len();
                    {
                        use std::fmt::Write as _;
                        let _ = write!(key, "/{index}");
                    }
                    total += new_sum_paths(key, width, depth - 1);
                    key.truncate(previous_len);
                }
            }
            total
        }

        let iterations = 20_000;
        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total += old_sum_paths("root".to_string(), 4, 5);
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let mut key = "root".to_string();
            new_total += new_sum_paths(&mut key, 4, 5);
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "runtime key paths: format-per-child {old_time:?}; mutable path {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    #[test]
    fn retained_widget_tree_reports_dirty_categories_by_stable_id() {
        let mut tree = WidgetNode::new("row");
        tree.children.push(WidgetNode::new("button"));
        annotate_with_empty_context(&mut tree);

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
    #[should_panic(expected = "runtime NodeId collision")]
    fn retained_snapshot_collection_panics_on_duplicate_node_ids() {
        let mut root = WidgetNode::new("row");
        root.id = 42;
        root.attributes
            .insert("_mesh_key".to_string(), "root".to_string());
        let mut child = WidgetNode::new("button");
        child.id = 42;
        child
            .attributes
            .insert("_mesh_key".to_string(), "root/0".to_string());
        root.children.push(child);

        let mut snapshots = HashMap::new();
        collect_retained_snapshots(&root, &mut snapshots);
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
