//! Retained batch-material metadata for the ordered primitive stream.
//!
//! Batch metrics describe maximal runs of adjacent primitives that share a
//! material, which is a property of the *whole* ordered stream: a run can span
//! any number of nodes. Recomputing them the obvious way therefore costs a full
//! tree walk plus one entry per primitive slot, on every frame — including
//! frames where a handful of nodes changed colour.
//!
//! This index keeps that stream in a segment tree over `(node, slot)` leaves in
//! paint order. [`Summary::join`] merges two adjacent ranges in constant time by
//! carrying each range's leading and trailing run, so patching one leaf costs
//! `O(log n)` and the root summary is the same answer
//! [`compute_batch_metrics`](super::signature::compute_batch_metrics) computes
//! by scanning.
//!
//! The index is only valid while node identity, order, transforms and geometry
//! are unchanged — exactly the conditions
//! [`can_patch_sparse_entries`](super::RetainedDisplayList::can_patch_sparse_entries)
//! already requires for sparse entry patching. Anything broader rebuilds it.

use std::collections::{HashMap, HashSet};

use mesh_core_elements::{
    AffineTransform, NodeId, WidgetNode, child_transform, node_transform, root_transform,
};

use super::{build::*, types::*};

/// Batch statistics for one contiguous range of the primitive stream, plus the
/// leading and trailing runs needed to merge it with its neighbours.
///
/// `prefix`/`suffix` are `(batch signature, run length)` for the run at each
/// end, or `None` when that end is a barrier. A range of `len == 0` is the join
/// identity: it stands for slots a node does not emit.
#[derive(Clone, Copy, Debug, Default)]
struct Summary {
    len: usize,
    prefix: Option<(u64, u64)>,
    suffix: Option<(u64, u64)>,
    batches: u64,
    primitives: u64,
    barriers: DisplayBatchBarrierCounts,
    barrier_count: u64,
}

impl Summary {
    fn leaf(material: Option<DisplayBatchMaterial>) -> Self {
        let Some(material) = material else {
            return Self::default();
        };
        let mut result = Self {
            len: 1,
            ..Self::default()
        };
        if let Some(reason) = material.barrier {
            reason.record(&mut result.barriers);
            result.barrier_count = 1;
        } else {
            // A lone primitive is not yet a batch; `compute_batch_metrics`
            // counts a run only once it reaches two.
            result.prefix = Some((material.batch_signature, 1));
            result.suffix = result.prefix;
        }
        result
    }

    fn join(left: Self, right: Self) -> Self {
        if left.len == 0 {
            return right;
        }
        if right.len == 0 {
            return left;
        }
        let mut joined = Self {
            len: left.len + right.len,
            prefix: left.prefix,
            suffix: right.suffix,
            batches: left.batches + right.batches,
            primitives: left.primitives + right.primitives,
            barrier_count: left.barrier_count + right.barrier_count,
            barriers: DisplayBatchBarrierCounts {
                text: left.barriers.text + right.barriers.text,
                icon: left.barriers.icon + right.barriers.icon,
                opacity: left.barriers.opacity + right.barriers.opacity,
                clip: left.barriers.clip + right.barriers.clip,
                translucency: left.barriers.translucency + right.barriers.translucency,
                material_change: left.barriers.material_change + right.barriers.material_change,
            },
        };
        // Only the seam between the two ranges is unaccounted for. A barrier on
        // either side of it resets the run, exactly as the scanning form does.
        let (Some((left_signature, left_run)), Some((right_signature, right_run))) =
            (left.suffix, right.prefix)
        else {
            return joined;
        };
        if left_signature != right_signature {
            joined.barrier_count += 1;
            joined.barriers.material_change += 1;
            return joined;
        }
        // The two runs are one run. Withdraw whichever of them was already
        // counted as a batch on its own side, then count the merged run once.
        joined.batches -= u64::from(left_run > 1) + u64::from(right_run > 1);
        joined.primitives -=
            if left_run > 1 { left_run } else { 0 } + if right_run > 1 { right_run } else { 0 };
        joined.batches += 1;
        joined.primitives += left_run + right_run;
        let merged = left_run + right_run;
        if left.prefix.is_some_and(|(_, run)| run == left.len as u64) {
            joined.prefix = Some((left_signature, merged));
        }
        if right.suffix.is_some_and(|(_, run)| run == right.len as u64) {
            joined.suffix = Some((right_signature, merged));
        }
        joined
    }
}

/// Where one node's primitive slots sit in the ordered stream, and what it
/// takes to re-collect that node's entries without walking the tree.
#[derive(Debug)]
struct NodeLocation {
    /// Child indices from the root, for direct descent to the live node.
    path: Box<[usize]>,
    /// The node's parent transform at the last rebuild. Valid only while the
    /// dirty summary reports no transform or geometry change.
    parent_transform: AffineTransform,
    /// Index of this node's first slot leaf.
    start: usize,
    /// Whether the node and all its ancestors were visible at the last rebuild.
    visible: bool,
}

/// Paint order within a node, matching `for_each_primitive_slot`. `Text` and
/// `Icon` are mutually exclusive, and `Generic` is emitted only when no other
/// slot is, so absent slots simply leave identity leaves behind.
const SLOTS: [DisplayPrimitiveSlot; 5] = [
    DisplayPrimitiveSlot::Background,
    DisplayPrimitiveSlot::Border,
    DisplayPrimitiveSlot::Text,
    DisplayPrimitiveSlot::Icon,
    DisplayPrimitiveSlot::Generic,
];

#[derive(Debug, Default)]
pub(super) struct BatchIndex {
    nodes: HashMap<NodeId, NodeLocation>,
    /// Implicit segment tree; `summaries[1]` is the whole stream and the leaves
    /// start at `base`.
    summaries: Vec<Summary>,
    base: usize,
    root_id: Option<NodeId>,
}

impl BatchIndex {
    /// Whether this index still describes `root` and can be patched rather than
    /// rebuilt. The caller is responsible for the dirty-summary conditions.
    pub(super) fn matches_root(&self, root: &WidgetNode) -> bool {
        self.root_id == Some(root.id) && !self.summaries.is_empty()
    }

    /// Record paint-order positions for every node under `root` and seed the
    /// segment tree from a *complete* entry map.
    pub(super) fn rebuild(
        &mut self,
        root: &WidgetNode,
        offset_x: f32,
        offset_y: f32,
        entries: &HashMap<DisplayListKey, DisplayListEntry>,
    ) {
        self.nodes.clear();
        let mut path = Vec::new();
        self.record(root, root_transform(offset_x, offset_y), &mut path, true);
        self.root_id = Some(root.id);
        self.base = (self.nodes.len() * SLOTS.len()).max(1).next_power_of_two();
        self.summaries.clear();
        self.summaries.resize(self.base * 2, Summary::default());
        for (node_id, location) in &self.nodes {
            for (offset, slot) in SLOTS.iter().enumerate() {
                self.summaries[self.base + location.start + offset] =
                    Summary::leaf(material(entries, *node_id, *slot));
            }
        }
        for index in (1..self.base).rev() {
            self.summaries[index] =
                Summary::join(self.summaries[index * 2], self.summaries[index * 2 + 1]);
        }
    }

    fn record(
        &mut self,
        node: &WidgetNode,
        parent_transform: AffineTransform,
        path: &mut Vec<usize>,
        visible: bool,
    ) {
        let visible = visible && !node_is_explicitly_hidden(node);
        let start = self.nodes.len() * SLOTS.len();
        self.nodes.insert(
            node.id,
            NodeLocation {
                path: path.as_slice().into(),
                parent_transform,
                start,
                visible,
            },
        );
        let scroll = node.resolved_scroll_metrics();
        let child_transform = child_transform(
            node_transform(parent_transform, node),
            node,
            scroll.x,
            scroll.y,
        );
        for (index, child) in node.children.iter().enumerate() {
            path.push(index);
            self.record(child, child_transform, path, visible);
            path.pop();
        }
    }

    /// Re-collect display entries for `dirty` alone, descending to each node by
    /// its recorded path instead of walking the tree. Returns `false` when any
    /// node has moved, disappeared, or flipped visibility, in which case the
    /// caller must fall back to a full collection.
    #[cfg(test)]
    pub(super) fn collect_dirty_entries(
        &self,
        root: &WidgetNode,
        dirty: &HashSet<NodeId>,
        next: &mut HashMap<DisplayListKey, DisplayListEntry>,
    ) -> bool {
        self.collect_dirty_entries_with_fingerprints(root, dirty, next, None)
    }

    pub(super) fn collect_dirty_entries_with_fingerprints(
        &self,
        root: &WidgetNode,
        dirty: &HashSet<NodeId>,
        next: &mut HashMap<DisplayListKey, DisplayListEntry>,
        fingerprints: Option<&super::RetainedFingerprintLookup<'_>>,
    ) -> bool {
        for node_id in dirty {
            let Some(location) = self.nodes.get(node_id) else {
                return false;
            };
            let mut node = root;
            for index in location.path.iter() {
                let Some(child) = node.children.get(*index) else {
                    return false;
                };
                node = child;
            }
            if node.id != *node_id {
                return false;
            }
            // An ancestor's `hidden` is folded into `visible`, so a node that
            // is itself visible under a hidden ancestor also takes the
            // fallback rather than emitting entries nothing paints.
            if location.visible == node_is_explicitly_hidden(node) {
                return false;
            }
            if location.visible {
                collect_node_entries(
                    node,
                    node_transform(location.parent_transform, node),
                    None,
                    true,
                    next,
                    fingerprints,
                );
            }
        }
        true
    }

    /// Re-summarize the slots of every node in `dirty` from the reconciled
    /// entry map, then repair the path to the root. Returns `false` when a
    /// node is unknown to the index.
    pub(super) fn patch(
        &mut self,
        dirty: &HashSet<NodeId>,
        entries: &HashMap<DisplayListKey, DisplayListEntry>,
    ) -> bool {
        for node_id in dirty {
            let Some(start) = self.nodes.get(node_id).map(|location| location.start) else {
                return false;
            };
            for (offset, slot) in SLOTS.iter().enumerate() {
                let mut index = self.base + start + offset;
                self.summaries[index] = Summary::leaf(material(entries, *node_id, *slot));
                index /= 2;
                while index > 0 {
                    self.summaries[index] =
                        Summary::join(self.summaries[index * 2], self.summaries[index * 2 + 1]);
                    index /= 2;
                }
            }
        }
        true
    }

    pub(super) fn metrics(&self) -> DisplayListMetrics {
        let summary = self.summaries.get(1).copied().unwrap_or_default();
        DisplayListMetrics {
            batch_count: summary.batches,
            batched_primitives: summary.primitives,
            barrier_count: summary.barrier_count,
            barriers: summary.barriers,
            ..Default::default()
        }
    }
}

fn material(
    entries: &HashMap<DisplayListKey, DisplayListEntry>,
    node_id: NodeId,
    slot: DisplayPrimitiveSlot,
) -> Option<DisplayBatchMaterial> {
    entries
        .get(&DisplayListKey { node_id, slot })
        .map(|entry| DisplayBatchMaterial {
            batch_signature: entry.batch_signature,
            barrier: entry.barrier,
        })
}
