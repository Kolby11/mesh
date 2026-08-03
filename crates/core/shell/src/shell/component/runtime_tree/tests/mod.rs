mod annotate;
mod benchmarks;
mod fingerprint;
mod node_id;
mod service_deps;
mod tree;

use super::*;
use std::time::Instant;

fn annotate_with_empty_context(node: &mut WidgetNode) {
    let input_values = HashMap::new();
    let mut slider_values = HashMap::new();
    let mut slider_script_values = HashMap::new();
    let checked_values = HashMap::new();
    let mut scroll_offsets = HashMap::new();
    let mut context = RuntimeAnnotationContext::new(
        None,
        None,
        &[],
        None,
        None,
        &input_values,
        &mut slider_values,
        &mut slider_script_values,
        &checked_values,
        &mut scroll_offsets,
    );
    annotate_runtime_tree(node, "root".to_string(), &mut context);
}

fn benchmark_plain_tree(width: usize, depth: usize) -> WidgetNode {
    let mut node = WidgetNode::new(if depth % 2 == 0 { "box" } else { "row" });
    if depth > 0 {
        node.children = (0..width)
            .map(|_| benchmark_plain_tree(width, depth - 1))
            .collect();
    }
    node
}

fn first_deep_leaf_mut(mut node: &mut WidgetNode) -> &mut WidgetNode {
    while !node.children.is_empty() {
        node = &mut node.children[0];
    }
    node
}

fn update_via_snapshot_map(
    retained: &mut RetainedWidgetTree,
    root: &WidgetNode,
    next_nodes: &mut HashMap<NodeId, RetainedNodeSnapshot>,
) -> RetainedTreeDirtySummary {
    next_nodes.clear();
    collect_retained_snapshots(root, next_nodes);

    let mut dirty = RetainedTreeDirtySummary::default();
    let mut next_dirty = std::mem::take(&mut retained.next_dirty_scratch);
    next_dirty.clear();
    let mut next_dirty_node_ids = std::mem::take(&mut retained.next_dirty_node_ids_scratch);
    next_dirty_node_ids.clear();

    retained.node_keys.retain(|id, key| {
        if next_nodes.contains_key(id) {
            return true;
        }
        retained.nodes.remove(*key);
        dirty.removed += 1;
        false
    });

    for (node_id, next) in next_nodes.drain() {
        match retained.node_keys.get(&node_id).copied() {
            Some(previous_key) => match retained.nodes.get(previous_key) {
                Some(previous) => {
                    let (flags, node_state_bits) = previous.diff_flags(&next);
                    if !flags.is_empty() {
                        dirty.add_flags(flags);
                        dirty.changed_state_bits |= node_state_bits;
                        next_dirty.insert(previous_key, flags);
                        next_dirty_node_ids.insert(node_id);
                        *retained.nodes.get_mut(previous_key).unwrap() = next;
                    }
                }
                None => {
                    let key = retained.nodes.insert(next);
                    retained.node_keys.insert(node_id, key);
                    next_dirty.insert(key, RetainedNodeDirtyFlags::INSERTED);
                    dirty.inserted += 1;
                }
            },
            None => {
                let key = retained.nodes.insert(next);
                retained.node_keys.insert(node_id, key);
                next_dirty.insert(key, RetainedNodeDirtyFlags::INSERTED);
                dirty.inserted += 1;
            }
        }
    }

    if dirty.any() {
        retained.generation = retained.generation.saturating_add(1);
    }
    let previous_dirty = std::mem::replace(&mut retained.dirty, next_dirty);
    retained.next_dirty_scratch = previous_dirty;
    let previous_dirty_node_ids =
        std::mem::replace(&mut retained.dirty_node_ids, next_dirty_node_ids);
    retained.next_dirty_node_ids_scratch = previous_dirty_node_ids;
    retained.last_dirty = dirty;
    dirty
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
    style.min_width = Dimension::Px(24.0);
    style.max_width = Dimension::Px(1200.0);
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
