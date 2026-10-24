use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mesh_core_elements::style::Position;
use mesh_core_elements::{
    AffineClipStack, AffineTransform, NodeId, WidgetNode, child_transform, frame_work,
    node_transform, root_transform,
};

use super::blur::command_has_effect_overflow;
use super::build::{
    command_bounds, node_clip_for, should_preclip_child_subtree_with_transform, visual_clip_for,
};
use super::paint_node::{
    build_paint_node_with_previous_transform_and_clips, intersect_display_clip,
};
use super::sequence::PaintSequence;
use super::subtree::{LocalReuseMetrics, RetainedPaintSubtree, RetainedSubtreeSpan};
use super::types::{
    DamageRect, DisplayListClip, DisplayPaintCommand, DisplayPaintCommandKind, DisplayPaintNode,
    RetainedDisplayList,
};

#[derive(Debug)]
struct CommandLocation {
    start: usize,
    local_commands: usize,
}

#[derive(Debug, Default)]
pub(super) struct SparsePaintIndex {
    starts: HashMap<NodeId, CommandLocation>,
    pub(super) has_scroll_viewports: bool,
}

impl SparsePaintIndex {
    pub(super) fn rebuild(&mut self, commands: &PaintSequence<DisplayPaintCommand>) {
        self.starts.clear();
        self.has_scroll_viewports = false;
        for (index, command) in commands.iter().enumerate() {
            let location = self
                .starts
                .entry(command.node.id)
                .or_insert(CommandLocation {
                    start: index,
                    local_commands: 0,
                });
            location.local_commands += 1;
            self.has_scroll_viewports |= command.kind == DisplayPaintCommandKind::PushScrollContent;
        }
    }
}

pub(super) struct SparsePaintPatch {
    pub(super) subtrees: HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    pub(super) metrics: LocalReuseMetrics,
}

/// The coordinate space and clip a node passes to its children, as
/// `build_paint_subtree_with_transform` derives them.
struct PathFrame<'t> {
    node: &'t WidgetNode,
    child_space: AffineTransform,
    child_clip: DisplayListClip,
}

impl<'t> PathFrame<'t> {
    fn new(node: &'t WidgetNode, paint: &DisplayPaintNode, clip: DisplayListClip) -> Self {
        let scroll = node.resolved_scroll_metrics();
        let child_space = child_transform(paint.transform, node, scroll.x, scroll.y);
        let child_clip = if node.computed_style.overflow_x.clips_contents()
            || node.computed_style.overflow_y.clips_contents()
        {
            intersect_display_clip(clip, node_clip_for(paint))
        } else {
            clip
        };
        Self {
            node,
            child_space,
            child_clip,
        }
    }
}

/// A replaced command whose visual bounds changed, with every subtree that
/// contains it, outermost first.
struct MovedCommand {
    id: NodeId,
    bounds: DamageRect,
    ancestors: Vec<NodeId>,
}

impl RetainedDisplayList {
    /// Replace the commands of nodes that each own one ordinary command,
    /// without visiting their siblings. Text, semantic and local geometry
    /// changes preserve command topology. Moved bounds also update ancestor
    /// coverage and spans exactly, which requires single-command ancestors.
    pub(super) fn patch_stable_commands(
        &self,
        root: &WidgetNode,
        dirty: &HashSet<NodeId>,
        surface_clip: DisplayListClip,
        offset: (f32, f32),
    ) -> Option<SparsePaintPatch> {
        let mut targets = dirty
            .iter()
            .map(|id| self.batch_index.node_path(*id).map(|path| (path, *id)))
            .collect::<Option<Vec<_>>>()?;
        targets.sort_unstable_by(|left, right| left.0.cmp(right.0));
        let retained_paint = |id: NodeId| {
            self.subtrees
                .get(&id)?
                .commands
                .first()
                .filter(|command| command.node.id == id)
                .map(|command| &*command.node)
        };
        let mut updated: HashMap<NodeId, RetainedPaintSubtree> = HashMap::new();
        let mut moved = Vec::new();
        // What each path node passes its children, for re-unioning coverage.
        let mut child_spaces: HashMap<NodeId, (AffineTransform, DisplayListClip)> = HashMap::new();
        let mut stack: Vec<PathFrame<'_>> = Vec::new();
        let mut previous: &[usize] = &[];
        for (path, id) in targets {
            let shared = previous
                .iter()
                .zip(path)
                .take_while(|(left, right)| left == right)
                .count();
            if stack.is_empty() {
                frame_work::visit(frame_work::Pass::DisplayAncestors);
                stack.push(PathFrame::new(root, retained_paint(root.id)?, surface_clip));
            }
            stack.truncate(shared + 1);
            for &index in &path[shared..] {
                frame_work::visit(frame_work::Pass::DisplayAncestors);
                let parent = stack.last()?;
                let node = parent.node.children.get(index)?;
                let clip = if node.computed_style.position == Position::Fixed {
                    surface_clip
                } else {
                    parent.child_clip
                };
                stack.push(PathFrame::new(node, retained_paint(node.id)?, clip));
            }
            previous = path;
            let live = stack.last()?.node;
            if live.id != id || super::build::node_is_explicitly_hidden(live) {
                return None;
            }
            if self.sparse_paint_index.starts.get(&id)?.local_commands != 1 {
                return None;
            }
            let command = self.subtrees.get(&id)?.commands.first()?;
            if command.kind != DisplayPaintCommandKind::Node || command.node.id != id {
                return None;
            }
            if live.computed_style.position == Position::Fixed {
                return None;
            }
            let (parent_space, clip) = match stack.len().checked_sub(2) {
                Some(parent) => (stack[parent].child_space, stack[parent].child_clip),
                None => (root_transform(offset.0, offset.1), surface_clip),
            };
            frame_work::visit(frame_work::Pass::DisplayBuild);
            let mut node = build_paint_node_with_previous_transform_and_clips(
                live,
                node_transform(parent_space, live),
                Some(&command.node),
                &AffineClipStack::default(),
            );
            node.ancestor_clips = Arc::clone(&command.node.ancestor_clips);
            node.in_scroll_content = command.node.in_scroll_content;
            let node_clip = intersect_display_clip(clip, visual_clip_for(&node));
            if node_clip.width <= 0 || node_clip.height <= 0 {
                return None;
            }
            let box_changed = node.transform != command.node.transform
                || node.layout != command.node.layout
                || node.local_layout != command.node.local_layout;
            // Children keep their retained coordinate space and clips only
            // while this node's box does not define them.
            if box_changed
                && !live.children.is_empty()
                && (live.computed_style.overflow_x.clips_contents()
                    || live.computed_style.overflow_y.clips_contents()
                    || !super::batch_index::size_independent_transform(
                        live.computed_style.transform,
                    ))
            {
                return None;
            }
            let replacement = DisplayPaintCommand {
                node: Arc::new(node),
                clip: node_clip,
                kind: command.kind,
            };
            if command_has_effect_overflow(&replacement) != command_has_effect_overflow(command) {
                return None;
            }
            let bounds = command_bounds(&replacement);
            let position = self.sparse_paint_index.starts.get(&id)?.start;
            let ancestors: Vec<NodeId> = stack.iter().map(|frame| frame.node.id).collect();
            for &ancestor in &ancestors {
                let previous = self.subtrees.get(&ancestor)?;
                let relative =
                    position.checked_sub(self.sparse_paint_index.starts.get(&ancestor)?.start)?;
                let subtree = updated.entry(ancestor).or_insert_with(|| {
                    let mut next = (**previous).clone();
                    next.generation = next.generation.saturating_add(1);
                    next
                });
                let old = subtree.commands.get(relative)?;
                if old.node.id != id || old.kind != replacement.kind {
                    return None;
                }
                subtree.commands =
                    Arc::new(subtree.commands.replaced(relative, replacement.clone())?);
            }
            if bounds != command_bounds(command) {
                moved.push(MovedCommand {
                    id,
                    bounds,
                    ancestors,
                });
            }
            let frame = stack.last_mut()?;
            *frame = PathFrame::new(live, &replacement.node, clip);
            for frame in &stack {
                child_spaces.insert(frame.node.id, (frame.child_space, frame.child_clip));
            }
        }
        if !moved.is_empty() {
            self.update_moved_coverage(root, &moved, &child_spaces, surface_clip, &mut updated)?;
        }
        let metrics = LocalReuseMetrics {
            rebuilt_commands: dirty.len() as u64,
            rebuilt_segments: updated.len() as u64,
            reused_segments: self.subtrees.len().saturating_sub(updated.len()) as u64,
        };
        Some(SparsePaintPatch {
            subtrees: updated
                .into_iter()
                .map(|(id, subtree)| (id, Arc::new(subtree)))
                .collect(),
            metrics,
        })
    }

    /// Recompute bounds and span coverage innermost first. A child whose new
    /// bounds still reach every edge of its parent's child union that its old
    /// bounds reached updates that union directly; otherwise the parent
    /// re-unions its children.
    fn update_moved_coverage(
        &self,
        root: &WidgetNode,
        moved: &[MovedCommand],
        child_spaces: &HashMap<NodeId, (AffineTransform, DisplayListClip)>,
        surface_clip: DisplayListClip,
        updated: &mut HashMap<NodeId, RetainedPaintSubtree>,
    ) -> Option<()> {
        let mut depth_of: HashMap<NodeId, (usize, Option<NodeId>)> = HashMap::new();
        let mut local: HashMap<NodeId, DamageRect> = HashMap::new();
        for command in moved {
            local.insert(command.id, command.bounds);
            for (depth, &id) in command.ancestors.iter().enumerate() {
                let parent = depth.checked_sub(1).map(|parent| command.ancestors[parent]);
                depth_of.insert(id, (depth, parent));
            }
        }
        for &id in depth_of.keys() {
            let index = self.sparse_paint_index.starts.get(&id)?;
            if index.local_commands != 1 || self.subtrees.get(&id)?.scroll_viewport.is_some() {
                return None;
            }
        }
        let mut order: Vec<_> = depth_of
            .iter()
            .map(|(id, (depth, _))| (*depth, *id))
            .collect();
        order.sort_unstable_by(|left, right| right.cmp(left));
        // Per parent: the (old, new) bounds of children that changed.
        let mut changed_children: HashMap<NodeId, Vec<(DamageRect, DamageRect)>> = HashMap::new();
        for (_, id) in order {
            let subtree = updated.get_mut(&id)?;
            let span = subtree.command_span?;
            let local_bounds = local.get(&id).copied().unwrap_or(span.local_bounds);
            let mut child_bounds = span.child_bounds;
            let mut rescan = false;
            for (old, new) in changed_children.remove(&id).unwrap_or_default() {
                if !shrinks_union(old, new, child_bounds) {
                    child_bounds = child_bounds.union(new);
                } else {
                    rescan = true;
                }
            }
            if rescan {
                let node = self.live_node(root, id)?;
                let &(space, clip) = child_spaces.get(&id)?;
                child_bounds = DamageRect::default();
                for child in &node.children {
                    frame_work::visit(frame_work::Pass::DisplayAncestors);
                    let child_span = match updated.get(&child.id) {
                        Some(subtree) => subtree.command_span,
                        None => match self.subtrees.get(&child.id) {
                            Some(subtree) => subtree.command_span,
                            // A pre-clipped child was never built.
                            None => {
                                let (space, clip) =
                                    if child.computed_style.position == Position::Fixed {
                                        (root_transform(0.0, 0.0), surface_clip)
                                    } else {
                                        (space, clip)
                                    };
                                if !should_preclip_child_subtree_with_transform(child, space, clip)
                                {
                                    return None;
                                }
                                None
                            }
                        },
                    };
                    if let Some(child_span) = child_span {
                        child_bounds = child_bounds.union(child_span.bounds);
                    }
                }
            }
            let bounds = local_bounds.union(child_bounds);
            let subtree = updated.get_mut(&id)?;
            subtree.command_span = Some(RetainedSubtreeSpan {
                bounds,
                local_bounds,
                child_bounds,
                ..span
            });
            let start = self.sparse_paint_index.starts.get(&id)?.start;
            let mut spans = subtree.spans.to_vec();
            for command in moved {
                if !command.ancestors.contains(&id) {
                    continue;
                }
                let relative = self.sparse_paint_index.starts.get(&command.id)?.start - start;
                let index = spans.partition_point(|span| span.start < relative);
                let span = spans.get_mut(index)?;
                if span.owner != command.id || span.start != relative {
                    return None;
                }
                span.bounds = command.bounds;
            }
            subtree.spans = spans.into();
            if let Some(parent) = depth_of[&id].1 {
                changed_children
                    .entry(parent)
                    .or_default()
                    .push((span.bounds, bounds));
            }
        }
        Some(())
    }

    fn live_node<'t>(&self, root: &'t WidgetNode, id: NodeId) -> Option<&'t WidgetNode> {
        let mut node = root;
        for &index in self.batch_index.node_path(id)? {
            node = node.children.get(index)?;
        }
        (node.id == id).then_some(node)
    }
}

fn is_empty(rect: DamageRect) -> bool {
    rect.width == 0 || rect.height == 0
}

/// Whether replacing `old` by `new` can shrink `union`: some edge of the
/// union is reached by `old` but not by `new`.
fn shrinks_union(old: DamageRect, new: DamageRect, union: DamageRect) -> bool {
    if is_empty(old) {
        return false;
    }
    if is_empty(new) || is_empty(union) {
        return true;
    }
    let right = |rect: DamageRect| rect.x + rect.width;
    let bottom = |rect: DamageRect| rect.y + rect.height;
    (old.x <= union.x && new.x > union.x)
        || (old.y <= union.y && new.y > union.y)
        || (right(old) >= right(union) && right(new) < right(union))
        || (bottom(old) >= bottom(union) && bottom(new) < bottom(union))
}
