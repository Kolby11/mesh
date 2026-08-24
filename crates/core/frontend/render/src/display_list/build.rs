use std::collections::{HashMap, HashSet};
use std::hash::Hasher;
use std::sync::Arc;

use mesh_core_elements::style::Position;
use mesh_core_elements::{
    AffineClipStack, AffineTransform, InteractionTarget, LayoutRect, NodeId, WidgetNode,
    child_transform, node_eligibility, node_transform, root_transform,
};

use super::paint_node::*;
use super::signature::*;
use super::subtree::*;
use super::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DisplayListEntry {
    pub(super) bounds: DamageRect,
    pub(super) signature: u64,
    pub(super) batch_signature: u64,
    pub(super) barrier: Option<DisplayBatchBarrier>,
}

pub(super) struct DisplaySignatureHasher(u64);

impl Default for DisplaySignatureHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for DisplaySignatureHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
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

impl DisplaySignatureHasher {
    #[inline]
    pub(super) fn write_mix(&mut self, value: u64) {
        self.0 ^= value;
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        self.0 ^= self.0 >> 32;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PruningMetrics {
    pub(super) omitted_subtrees: u64,
    pub(super) omitted_nodes: u64,
    pub(super) omitted_commands: u64,
    pub(super) preclipped_descendants: u64,
}

impl PruningMetrics {
    pub(super) fn record_omitted_subtree(&mut self, counts: PrunedSubtreeCounts, preclipped: bool) {
        if counts.nodes == 0 && counts.commands == 0 {
            return;
        }
        self.omitted_subtrees = self.omitted_subtrees.saturating_add(1);
        self.omitted_nodes = self.omitted_nodes.saturating_add(counts.nodes);
        self.omitted_commands = self.omitted_commands.saturating_add(counts.commands);
        if preclipped {
            self.preclipped_descendants = self.preclipped_descendants.saturating_add(counts.nodes);
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PrunedSubtreeCounts {
    pub(super) nodes: u64,
    pub(super) commands: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FloatBounds {
    pub(super) left: f32,
    pub(super) top: f32,
    pub(super) right: f32,
    pub(super) bottom: f32,
}

impl FloatBounds {
    pub(super) fn intersects_clip(self, clip: DisplayListClip) -> bool {
        let clip_left = clip.x as f32;
        let clip_top = clip.y as f32;
        let clip_right = (clip.x + clip.width) as f32;
        let clip_bottom = (clip.y + clip.height) as f32;
        self.right > clip_left
            && self.bottom > clip_top
            && self.left < clip_right
            && self.top < clip_bottom
    }

    pub(super) fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

pub(super) fn collect_display_entries(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    mut ordered_entries: Option<&mut Vec<(DisplayListKey, DisplayListEntry)>>,
    selected_node_ids: Option<&HashSet<NodeId>>,
    next: &mut HashMap<DisplayListKey, DisplayListEntry>,
) {
    collect_display_entries_with_transform(
        node,
        root_transform(offset_x, offset_y),
        ordered_entries,
        selected_node_ids,
        next,
    );
}

fn collect_display_entries_with_transform(
    node: &WidgetNode,
    parent_transform: AffineTransform,
    mut ordered_entries: Option<&mut Vec<(DisplayListKey, DisplayListEntry)>>,
    selected_node_ids: Option<&HashSet<NodeId>>,
    next: &mut HashMap<DisplayListKey, DisplayListEntry>,
) {
    if node_is_explicitly_hidden(node) {
        return;
    }

    let world_transform = node_transform(parent_transform, node);

    if let Some(bounds) = damage_rect_for_node_with_transform(node, world_transform) {
        let selected = selected_node_ids.is_none_or(|node_ids| node_ids.contains(&node.id));
        for_each_primitive_slot(node, |slot| {
            // Debug batch metrics still need the full ordered stream. Release
            // builds can avoid constructing signatures for unselected nodes.
            if !selected && ordered_entries.is_none() {
                return;
            }
            let key = DisplayListKey {
                node_id: node.id,
                slot,
            };
            let barrier = batch_barrier(node, slot);
            let entry = DisplayListEntry {
                bounds,
                signature: primitive_signature(node, slot),
                batch_signature: barrier.map_or_else(|| batch_signature(node, slot), |_| 0),
                barrier,
            };
            if let Some(entries) = ordered_entries.as_deref_mut() {
                entries.push((key, entry));
            }
            if selected {
                next.insert(key, entry);
            }
        });
    }

    let scroll = node.resolved_scroll_metrics();
    let child_transform = child_transform(world_transform, node, scroll.x, scroll.y);

    for child in &node.children {
        collect_display_entries_with_transform(
            child,
            child_transform,
            ordered_entries.as_deref_mut(),
            selected_node_ids,
            next,
        );
    }
}

#[cfg(test)]
pub(super) fn collect_dirty_ancestor_ids(
    root: &WidgetNode,
    dirty_node_ids: &HashSet<NodeId>,
) -> HashSet<NodeId> {
    let mut ancestors = HashSet::new();
    let mut path = Vec::new();
    collect_dirty_ancestor_ids_into(root, dirty_node_ids, &mut path, &mut ancestors);
    ancestors
}

pub(super) fn collect_dirty_ancestor_ids_into(
    root: &WidgetNode,
    dirty_node_ids: &HashSet<NodeId>,
    path: &mut Vec<NodeId>,
    ancestors: &mut HashSet<NodeId>,
) {
    if dirty_node_ids.is_empty() {
        return;
    }
    collect_dirty_ancestor_ids_inner(root, dirty_node_ids, dirty_node_ids.len(), path, ancestors);
}

pub(super) fn collect_dirty_ancestor_ids_inner(
    node: &WidgetNode,
    dirty_node_ids: &HashSet<NodeId>,
    remaining_dirty: usize,
    path: &mut Vec<NodeId>,
    ancestors: &mut HashSet<NodeId>,
) -> usize {
    if remaining_dirty == 0 {
        return 0;
    }
    let mut remaining_dirty = remaining_dirty;
    let is_dirty = dirty_node_ids.contains(&node.id);
    if is_dirty {
        for ancestor in path.iter().copied() {
            ancestors.insert(ancestor);
        }
        remaining_dirty -= 1;
        if remaining_dirty == 0 {
            return 0;
        }
    }
    path.push(node.id);
    for child in &node.children {
        remaining_dirty = collect_dirty_ancestor_ids_inner(
            child,
            dirty_node_ids,
            remaining_dirty,
            path,
            ancestors,
        );
        if remaining_dirty == 0 {
            break;
        }
    }
    path.pop();
    remaining_dirty
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_paint_subtree(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    clip: DisplayListClip,
    viewport_clip: DisplayListClip,
    force_rebuild: bool,
    allow_clean_descendant_reuse: bool,
    dirty_node_ids: &HashSet<NodeId>,
    dirty_ancestors: &HashSet<NodeId>,
    previous_subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    next_subtrees: &mut HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    metrics: &mut LocalReuseMetrics,
) -> Arc<RetainedPaintSubtree> {
    build_paint_subtree_with_transform(
        node,
        root_transform(offset_x, offset_y),
        &AffineClipStack::default(),
        clip,
        viewport_clip,
        force_rebuild,
        allow_clean_descendant_reuse,
        dirty_node_ids,
        dirty_ancestors,
        previous_subtrees,
        next_subtrees,
        metrics,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_paint_subtree_with_transform(
    node: &WidgetNode,
    parent_transform: AffineTransform,
    ancestor_clips: &AffineClipStack,
    clip: DisplayListClip,
    viewport_clip: DisplayListClip,
    force_rebuild: bool,
    allow_clean_descendant_reuse: bool,
    dirty_node_ids: &HashSet<NodeId>,
    dirty_ancestors: &HashSet<NodeId>,
    previous_subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    next_subtrees: &mut HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    metrics: &mut LocalReuseMetrics,
) -> Arc<RetainedPaintSubtree> {
    let node_is_dirty = dirty_node_ids.contains(&node.id);
    let node_is_ancestor = dirty_ancestors.contains(&node.id);
    if !force_rebuild
        && !node_is_dirty
        && !node_is_ancestor
        && let Some(previous) = previous_subtrees.get(&node.id)
    {
        metrics.reused_segments = metrics.reused_segments.saturating_add(1);
        let reused = Arc::clone(previous);
        next_subtrees.insert(node.id, Arc::clone(&reused));
        return reused;
    }

    metrics.rebuilt_segments = metrics.rebuilt_segments.saturating_add(1);
    let generation = previous_subtrees
        .get(&node.id)
        .map_or(1, |previous| previous.generation.saturating_add(1));

    if node_is_explicitly_hidden(node) {
        let mut subtree = RetainedPaintSubtree::default();
        subtree.generation = generation;
        subtree.pruning.record_omitted_subtree(
            count_pruned_subtree_with_transform(node, parent_transform, true),
            false,
        );
        let subtree = Arc::new(subtree);
        next_subtrees.insert(node.id, Arc::clone(&subtree));
        return subtree;
    }

    let world_transform = node_transform(parent_transform, node);
    let previous_paint_node = previous_subtrees
        .get(&node.id)
        .and_then(|subtree| subtree.commands.first())
        .filter(|command| command.node.id == node.id)
        .map(|command| command.node.as_ref());
    let paint_node = Arc::new(build_paint_node_with_previous_transform_and_clips(
        node,
        world_transform,
        previous_paint_node,
        ancestor_clips,
    ));
    let bounds = node_clip_for(&paint_node);
    let visual_bounds = visual_clip_for(&paint_node);
    let node_clip = intersect_display_clip(clip, visual_bounds);
    if node_clip.width <= 0 || node_clip.height <= 0 {
        let mut subtree = RetainedPaintSubtree::default();
        subtree.generation = generation;
        subtree.pruning.record_omitted_subtree(
            count_pruned_subtree_with_transform(node, parent_transform, false),
            true,
        );
        let subtree = Arc::new(subtree);
        next_subtrees.insert(node.id, Arc::clone(&subtree));
        return subtree;
    }

    let mut subtree = PaintSubtreeBuilder::default();
    // `filter: blur()` blurs the element *and its descendants*, so it lowers
    // into a layer scope around the whole subtree rather than a filtered paint
    // on this node's own shape. The push clip starts at this node's blurred
    // extent and grows to the subtree's once the children are known.
    let filter_layer = paint_node.style.filter.blur_radius > 0.0;
    if filter_layer {
        subtree.push_command(DisplayPaintCommand {
            node: Arc::clone(&paint_node),
            clip: intersect_display_clip(clip, visual_bounds),
            kind: DisplayPaintCommandKind::PushFilterLayer,
        });
        metrics.rebuilt_commands = metrics.rebuilt_commands.saturating_add(1);
    }
    subtree.push_command(DisplayPaintCommand {
        node: Arc::clone(&paint_node),
        clip: node_clip,
        kind: DisplayPaintCommandKind::Node,
    });
    metrics.rebuilt_commands = metrics.rebuilt_commands.saturating_add(1);

    let scroll = node.resolved_scroll_metrics();
    let scroll_x = scroll.x;
    let scroll_y = scroll.y;
    let child_transform = child_transform(world_transform, node, scroll_x, scroll_y);
    let child_clip = if node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents()
    {
        intersect_display_clip(clip, bounds)
    } else {
        clip
    };
    let child_ancestor_clips = if node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents()
    {
        ancestor_clips.push(mesh_core_elements::node_clip(node, world_transform))
    } else {
        ancestor_clips.clone()
    };
    let child_order = compute_child_order(node);
    for_children_in_order(node, child_order.as_deref(), |child| {
        let (child_parent_transform, cc) = if child.computed_style.position == Position::Fixed {
            (root_transform(0.0, 0.0), viewport_clip)
        } else {
            (child_transform, child_clip)
        };
        let child_clips = if child.computed_style.position == Position::Fixed {
            AffineClipStack::default()
        } else {
            child_ancestor_clips.clone()
        };
        append_child_paint_subtree(
            &mut subtree,
            child,
            child_parent_transform,
            &child_clips,
            cc,
            viewport_clip,
            force_rebuild || (node_is_dirty && !allow_clean_descendant_reuse),
            allow_clean_descendant_reuse,
            dirty_node_ids,
            dirty_ancestors,
            previous_subtrees,
            next_subtrees,
            metrics,
        );
    });
    subtree.child_order = child_order;

    if display_node_may_show_scrollbars(&paint_node) {
        subtree.push_command(DisplayPaintCommand {
            node: Arc::clone(&paint_node),
            clip: node_clip,
            kind: DisplayPaintCommandKind::Scrollbars,
        });
        metrics.rebuilt_commands = metrics.rebuilt_commands.saturating_add(1);
    }
    if filter_layer {
        let blur_pad = paint_node.style.filter.blur_radius * 3.0;
        subtree.grow_filter_layer_bounds(clip, blur_pad);
        subtree.push_command(DisplayPaintCommand {
            node: paint_node,
            clip: node_clip,
            kind: DisplayPaintCommandKind::PopFilterLayer,
        });
        metrics.rebuilt_commands = metrics.rebuilt_commands.saturating_add(1);
    }
    let subtree = Arc::new(subtree.into_retained(generation, filter_layer));
    next_subtrees.insert(node.id, Arc::clone(&subtree));
    subtree
}

pub(super) fn display_node_may_show_scrollbars(node: &DisplayPaintNode) -> bool {
    node.style.overflow_y.always_shows_scrollbar()
        || node.style.overflow_x.always_shows_scrollbar()
        || (node.style.overflow_y.shows_scrollbar_when_overflowing()
            && node.scrollbars.max_y > f32::EPSILON)
        || (node.style.overflow_x.shows_scrollbar_when_overflowing()
            && node.scrollbars.max_x > f32::EPSILON)
}

pub(super) fn damage_covers_surface(damage: DamageRect, surface_size: (u32, u32)) -> bool {
    damage.x == 0
        && damage.y == 0
        && damage.width >= surface_size.0
        && damage.height >= surface_size.1
}

pub(super) fn union_damage_rects(damages: &[DamageRect]) -> Option<DamageRect> {
    let first = damages.first().copied()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x.saturating_add(first.width);
    let mut max_y = first.y.saturating_add(first.height);
    for damage in &damages[1..] {
        min_x = min_x.min(damage.x);
        min_y = min_y.min(damage.y);
        max_x = max_x.max(damage.x.saturating_add(damage.width));
        max_y = max_y.max(damage.y.saturating_add(damage.height));
    }
    Some(DamageRect {
        x: min_x,
        y: min_y,
        width: max_x.saturating_sub(min_x),
        height: max_y.saturating_sub(min_y),
    })
}

pub(super) fn push_sparse_damage_rect(
    rects: &mut Vec<DamageRect>,
    rect: DamageRect,
    surface: DamageRect,
) {
    const MAX_RETAINED_DAMAGE_RECTS: usize = 16;

    let Some(mut merged) = clip_rect(rect, surface) else {
        return;
    };
    let mut index = 0;
    while index < rects.len() {
        if rects[index].intersects(merged) {
            merged = rects.swap_remove(index).union(merged);
            index = 0;
        } else {
            index += 1;
        }
    }
    rects.push(merged);
    if rects.len() > MAX_RETAINED_DAMAGE_RECTS {
        let union = union_damage_rects(rects).expect("non-empty retained damage list");
        rects.clear();
        rects.push(union);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_child_paint_subtree(
    subtree: &mut PaintSubtreeBuilder,
    child: &WidgetNode,
    child_parent_transform: AffineTransform,
    child_ancestor_clips: &AffineClipStack,
    child_clip: DisplayListClip,
    viewport_clip: DisplayListClip,
    force_rebuild: bool,
    allow_clean_descendant_reuse: bool,
    dirty_node_ids: &HashSet<NodeId>,
    dirty_ancestors: &HashSet<NodeId>,
    previous_subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    next_subtrees: &mut HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    metrics: &mut LocalReuseMetrics,
) {
    if should_preclip_child_subtree_with_transform(child, child_parent_transform, child_clip) {
        subtree.pruning.record_omitted_subtree(
            count_pruned_subtree_with_transform(child, child_parent_transform, false),
            true,
        );
        return;
    }
    let child_subtree = build_paint_subtree_with_transform(
        child,
        child_parent_transform,
        child_ancestor_clips,
        child_clip,
        viewport_clip,
        force_rebuild,
        allow_clean_descendant_reuse,
        dirty_node_ids,
        dirty_ancestors,
        previous_subtrees,
        next_subtrees,
        metrics,
    );
    subtree.append_child(&child_subtree);
    subtree.append_pruning(&child_subtree);
}

pub(super) fn for_children_in_order(
    node: &WidgetNode,
    child_order: Option<&[usize]>,
    mut visit: impl FnMut(&WidgetNode),
) {
    let Some(child_order) = child_order else {
        for child in &node.children {
            visit(child);
        }
        return;
    };

    for child_index in child_order {
        visit(&node.children[*child_index]);
    }
}

pub(super) fn compute_child_order(node: &WidgetNode) -> Option<Arc<[usize]>> {
    let child_count = node.children.len();
    if child_count <= 1 {
        return None;
    }

    let mut has_inversion = false;
    let mut previous_z_index = node.children[0].computed_style.z_index;
    for child in node.children.iter().skip(1) {
        if previous_z_index > child.computed_style.z_index {
            has_inversion = true;
            break;
        }
        previous_z_index = child.computed_style.z_index;
    }
    if !has_inversion {
        return None;
    }

    let mut child_order: Vec<usize> = (0..child_count).collect();
    child_order.sort_unstable_by_key(|&index| node.children[index].computed_style.z_index);
    Some(child_order.into_boxed_slice().into())
}

pub(super) fn node_is_explicitly_hidden(node: &WidgetNode) -> bool {
    !node_eligibility(node).allows(InteractionTarget::Paint)
}

pub(super) fn should_preclip_child_subtree(
    child: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    clip: DisplayListClip,
) -> bool {
    should_preclip_child_subtree_with_transform(child, root_transform(offset_x, offset_y), clip)
}

fn should_preclip_child_subtree_with_transform(
    child: &WidgetNode,
    parent_transform: AffineTransform,
    clip: DisplayListClip,
) -> bool {
    subtree_bounds_at_with_transform(child, parent_transform)
        .is_some_and(|bounds| !bounds.intersects_clip(clip))
}

pub(super) fn subtree_bounds_at(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> Option<FloatBounds> {
    subtree_bounds_at_with_transform(node, root_transform(offset_x, offset_y))
}

fn subtree_bounds_at_with_transform(
    node: &WidgetNode,
    parent_transform: AffineTransform,
) -> Option<FloatBounds> {
    if node_is_explicitly_hidden(node) {
        return None;
    }

    let world_transform = node_transform(parent_transform, node);
    let mut bounds = node_visual_bounds_at_with_transform(node, world_transform);
    let scroll = node.resolved_scroll_metrics();
    let child_transform = child_transform(world_transform, node, scroll.x, scroll.y);
    for child in &node.children {
        if let Some(child_bounds) = subtree_bounds_at_with_transform(child, child_transform) {
            bounds = Some(match bounds {
                Some(existing) => existing.union(child_bounds),
                None => child_bounds,
            });
        }
    }
    bounds
}

pub(super) fn node_visual_bounds_at(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> Option<FloatBounds> {
    node_visual_bounds_at_with_transform(
        node,
        node_transform(root_transform(offset_x, offset_y), node),
    )
}

fn node_visual_bounds_at_with_transform(
    node: &WidgetNode,
    world_transform: AffineTransform,
) -> Option<FloatBounds> {
    (node.layout.width > 0.0 && node.layout.height > 0.0).then(|| {
        let paint_node = build_paint_node_with_previous_transform(node, world_transform, None);
        let visual = visual_clip_for(&paint_node);
        FloatBounds {
            left: visual.x as f32,
            top: visual.y as f32,
            right: (visual.x + visual.width) as f32,
            bottom: (visual.y + visual.height) as f32,
        }
    })
}

pub(super) fn count_pruned_subtree(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    include_hidden_root: bool,
) -> PrunedSubtreeCounts {
    count_pruned_subtree_with_transform(
        node,
        root_transform(offset_x, offset_y),
        include_hidden_root,
    )
}

fn count_pruned_subtree_with_transform(
    node: &WidgetNode,
    parent_transform: AffineTransform,
    include_hidden_root: bool,
) -> PrunedSubtreeCounts {
    if node_is_explicitly_hidden(node) && !include_hidden_root {
        return PrunedSubtreeCounts::default();
    }

    let mut counts = PrunedSubtreeCounts::default();
    if node.layout.width > 0.0 && node.layout.height > 0.0 {
        counts.nodes = 1;
        counts.commands = 2;
    }
    let scroll = node.resolved_scroll_metrics();
    let world_transform = node_transform(parent_transform, node);
    let child_transform = child_transform(world_transform, node, scroll.x, scroll.y);
    for child in &node.children {
        let child_counts = count_pruned_subtree_with_transform(child, child_transform, false);
        counts.nodes = counts.nodes.saturating_add(child_counts.nodes);
        counts.commands = counts.commands.saturating_add(child_counts.commands);
    }
    counts
}

pub(super) fn surface_clip(surface: DamageRect) -> DisplayListClip {
    DisplayListClip {
        x: surface.x as i32,
        y: surface.y as i32,
        width: surface.width as i32,
        height: surface.height as i32,
    }
}

pub(super) fn node_clip_for(node: &DisplayPaintNode) -> DisplayListClip {
    DisplayListClip {
        x: node.layout.x.round() as i32,
        y: node.layout.y.round() as i32,
        width: node.layout.width.round().max(0.0) as i32,
        height: node.layout.height.round().max(0.0) as i32,
    }
}

pub(super) fn visual_clip_for(node: &DisplayPaintNode) -> DisplayListClip {
    let mut layout = node.transform.transform_rect(node.local_layout);
    let shadow = node.style.box_shadow;
    if !shadow.is_none() && !shadow.inset {
        let pad = shadow.spread_radius + shadow.blur_radius * 3.0;
        let shadow_layout = node.transform.transform_rect(LayoutRect {
            x: shadow.offset_x - pad,
            y: shadow.offset_y - pad,
            width: node.local_layout.width + pad * 2.0,
            height: node.local_layout.height + pad * 2.0,
        });
        layout = union_layout_rect(layout, shadow_layout);
    }
    let filter_pad = node
        .style
        .filter
        .blur_radius
        .max(node.style.backdrop_filter.blur_radius)
        * 3.0;
    if filter_pad > 0.0 {
        layout.x -= filter_pad;
        layout.y -= filter_pad;
        layout.width += filter_pad * 2.0;
        layout.height += filter_pad * 2.0;
    }
    DisplayListClip {
        x: layout.x.floor() as i32,
        y: layout.y.floor() as i32,
        width: ((layout.x + layout.width).ceil() - layout.x.floor()).max(0.0) as i32,
        height: ((layout.y + layout.height).ceil() - layout.y.floor()).max(0.0) as i32,
    }
}

fn union_layout_rect(left: LayoutRect, right: LayoutRect) -> LayoutRect {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let right_edge = (left.x + left.width).max(right.x + right.width);
    let bottom_edge = (left.y + left.height).max(right.y + right.height);
    LayoutRect {
        x,
        y,
        width: (right_edge - x).max(0.0),
        height: (bottom_edge - y).max(0.0),
    }
}

pub(super) fn command_bounds(command: &DisplayPaintCommand) -> DamageRect {
    let bounds = visual_clip_for(&command.node);
    let clip = intersect_display_clip(bounds, command.clip);
    DamageRect {
        x: clip.x.max(0) as u32,
        y: clip.y.max(0) as u32,
        width: clip.width.max(0) as u32,
        height: clip.height.max(0) as u32,
    }
}
