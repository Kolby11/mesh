use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mesh_core_elements::style::Position;
use mesh_core_elements::{
    AffineClipStack, AffineTransform, NodeId, WidgetNode, child_transform, frame_work,
    node_transform, root_transform,
};

use super::blur::command_has_effect_overflow;
use super::build::{command_bounds, node_clip_for, visual_clip_for};
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
    positions: Vec<usize>,
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
                    positions: Vec::new(),
                });
            location.positions.push(index);
            self.has_scroll_viewports |= command.kind == DisplayPaintCommandKind::PushScrollContent;
        }
    }
}

pub(super) struct SparsePaintPatch {
    pub(super) subtrees: HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    pub(super) metrics: LocalReuseMetrics,
    pub(super) damage: Vec<DamageRect>,
}

/// The coordinate space and clip a node passes to its children, as
/// `build_paint_subtree_with_transform` derives them.
struct PathFrame<'t> {
    node: &'t WidgetNode,
    child_space: AffineTransform,
    child_clip: DisplayListClip,
    clip: DisplayListClip,
}

impl<'t> PathFrame<'t> {
    fn new(
        node: &'t WidgetNode,
        paint: &DisplayPaintNode,
        clip: DisplayListClip,
        retained: bool,
    ) -> Self {
        let scroll = node.resolved_scroll_metrics();
        let child_space = child_transform(
            paint.transform,
            node,
            if retained { 0.0 } else { scroll.x },
            if retained { 0.0 } else { scroll.y },
        );
        let child_clip = if retained {
            super::scroll::CONTENT_CLIP
        } else if node.computed_style.overflow_x.clips_contents()
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
            clip,
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
    /// Replace topology-stable node and scrollbar commands through retained
    /// paths. Ancestor layer coverage and scroll coordinate spaces are repaired
    /// without rebuilding clean paint payloads or command topology.
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
        let mut incoming_clips: HashMap<NodeId, DisplayListClip> = HashMap::new();
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
                stack.push(PathFrame::new(
                    root,
                    retained_paint(root.id)?,
                    surface_clip,
                    self.subtrees.get(&root.id)?.scroll_viewport.is_some(),
                ));
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
                stack.push(PathFrame::new(
                    node,
                    retained_paint(node.id)?,
                    clip,
                    self.subtrees.get(&node.id)?.scroll_viewport.is_some(),
                ));
            }
            previous = path;
            let live = stack.last()?.node;
            if live.id != id || super::build::node_is_explicitly_hidden(live) {
                return None;
            }
            let location = self.sparse_paint_index.starts.get(&id)?;
            let subtree = self.subtrees.get(&id)?;
            let command = location.positions.iter().find_map(|position| {
                self.paint_commands
                    .get(*position)
                    .filter(|command| command.kind == DisplayPaintCommandKind::Node)
            })?;
            if command.node.id != id {
                return None;
            }
            if location.positions.iter().any(|position| {
                !matches!(
                    self.paint_commands[*position].kind,
                    DisplayPaintCommandKind::Node
                        | DisplayPaintCommandKind::Scrollbars
                        | DisplayPaintCommandKind::PushScrollContent
                        | DisplayPaintCommandKind::PopScrollContent
                        | DisplayPaintCommandKind::PushCompositingLayer
                        | DisplayPaintCommandKind::PopCompositingLayer
                        | DisplayPaintCommandKind::PushFilterLayer
                        | DisplayPaintCommandKind::PopFilterLayer
                )
            }) {
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
            if super::build::display_node_may_show_scrollbars(&node)
                != location.positions.iter().any(|position| {
                    self.paint_commands[*position].kind == DisplayPaintCommandKind::Scrollbars
                })
                || (subtree.scroll_viewport.is_none()
                    && (node.scrollbars.scroll_x != command.node.scrollbars.scroll_x
                        || node.scrollbars.scroll_y != command.node.scrollbars.scroll_y))
                || (subtree.scroll_viewport.is_some()
                    && node.scrollbars.max_x <= f32::EPSILON
                    && node.scrollbars.max_y <= f32::EPSILON)
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
            let ancestors: Vec<NodeId> = stack.iter().map(|frame| frame.node.id).collect();
            for &position in &location.positions {
                let original = self.paint_commands.get(position)?;
                let mut local = replacement.clone();
                local.kind = original.kind;
                if matches!(
                    local.kind,
                    DisplayPaintCommandKind::PushCompositingLayer
                        | DisplayPaintCommandKind::PushFilterLayer
                ) {
                    local.clip = original.clip;
                }
                if matches!(
                    local.kind,
                    DisplayPaintCommandKind::PushScrollContent
                        | DisplayPaintCommandKind::PopScrollContent
                ) {
                    let mut scope_node = (*local.node).clone();
                    scope_node.ancestor_clips = Arc::clone(&original.node.ancestor_clips);
                    local.node = Arc::new(scope_node);
                    local.clip = original.clip;
                }
                for &ancestor in &ancestors {
                    let previous = self.subtrees.get(&ancestor)?;
                    let relative = position
                        .checked_sub(self.sparse_paint_index.starts.get(&ancestor)?.start)?;
                    let subtree = updated.entry(ancestor).or_insert_with(|| {
                        let mut next = (**previous).clone();
                        next.generation = next.generation.saturating_add(1);
                        next
                    });
                    let old = subtree.commands.get(relative)?;
                    if old.node.id != id || old.kind != local.kind {
                        return None;
                    }
                    subtree.commands =
                        Arc::new(subtree.commands.replaced(relative, local.clone())?);
                }
            }
            if subtree.scroll_viewport.is_some() {
                updated.get_mut(&id)?.scroll_offset = (
                    replacement.node.scrollbars.scroll_x,
                    replacement.node.scrollbars.scroll_y,
                );
            }
            if bounds != command_bounds(command) {
                moved.push(MovedCommand {
                    id,
                    bounds,
                    ancestors,
                });
            }
            let frame = stack.last_mut()?;
            *frame = PathFrame::new(
                live,
                &replacement.node,
                clip,
                subtree.scroll_viewport.is_some(),
            );
            for frame in &stack {
                incoming_clips.insert(frame.node.id, frame.clip);
            }
        }
        if !moved.is_empty() {
            self.update_moved_coverage(&moved, &incoming_clips, &mut updated)?;
        }
        let metrics = LocalReuseMetrics {
            rebuilt_commands: dirty
                .iter()
                .map(|id| self.sparse_paint_index.starts[id].positions.len() as u64)
                .sum(),
            rebuilt_segments: updated.len() as u64,
            reused_segments: self.subtrees.len().saturating_sub(updated.len()) as u64,
        };
        let damage = updated
            .iter()
            .filter_map(|(id, next)| {
                let previous = self.subtrees.get(id)?;
                if let Some(viewport) = next.scroll_viewport {
                    return Some([viewport, viewport]);
                }
                if !self
                    .sparse_paint_index
                    .starts
                    .get(id)?
                    .positions
                    .iter()
                    .any(|position| {
                        matches!(
                            self.paint_commands[*position].kind,
                            DisplayPaintCommandKind::PushFilterLayer
                                | DisplayPaintCommandKind::PushCompositingLayer
                        )
                    })
                {
                    return None;
                }
                let old = previous.command_span?;
                let next = next.command_span?;
                (old.bounds != next.bounds).then_some([old.bounds, next.bounds])
            })
            .flatten()
            .collect();
        Some(SparsePaintPatch {
            damage,
            subtrees: updated
                .into_iter()
                .map(|(id, subtree)| (id, Arc::new(subtree)))
                .collect(),
            metrics,
        })
    }

    /// Repair child unions and scope coverage innermost first, sharing every
    /// unaffected bounds-index branch with the previous retained snapshot.
    fn update_moved_coverage(
        &self,
        moved: &[MovedCommand],
        incoming_clips: &HashMap<NodeId, DisplayListClip>,
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
        let mut order: Vec<_> = depth_of
            .iter()
            .map(|(id, (depth, _))| (*depth, *id))
            .collect();
        order.sort_unstable_by(|left, right| right.cmp(left));
        // Per parent: the IDs and new bounds of children that changed.
        let mut changed_children: HashMap<NodeId, Vec<(NodeId, DamageRect)>> = HashMap::new();
        for (_, id) in order {
            let subtree = updated.get_mut(&id)?;
            let span = subtree.command_span?;
            let local_bounds = local.get(&id).copied().unwrap_or(span.local_bounds);
            for (child, bounds) in changed_children.remove(&id).unwrap_or_default() {
                subtree.child_coverage.set(child, bounds)?;
            }
            let mut child_bounds = subtree.child_coverage.bounds();
            let start = self.sparse_paint_index.starts.get(&id)?.start;
            let previous = self.subtrees.get(&id)?;
            let mut bounds = local_bounds.union(child_bounds);
            let mut local_bounds = local_bounds;
            // Layer clips depend on descendant coverage, but their command
            // positions and clean paint payloads remain valid.
            let mut local_commands = Vec::new();
            for &position in &self.sparse_paint_index.starts.get(&id)?.positions {
                let index = position.checked_sub(start)?;
                local_commands.push((index, updated.get(&id)?.commands.get(index)?.clone()));
            }
            let incoming = *incoming_clips.get(&id)?;
            let filter = local_commands
                .iter()
                .any(|(_, c)| c.kind == DisplayPaintCommandKind::PushFilterLayer);
            let compositing = local_commands
                .iter()
                .any(|(_, c)| c.kind == DisplayPaintCommandKind::PushCompositingLayer);
            if filter || compositing {
                let mut builder = super::subtree::PaintSubtreeBuilder::default();
                for (_, command) in &local_commands {
                    let mut command = command.clone();
                    command.clip = intersect_display_clip(incoming, visual_clip_for(&command.node));
                    builder.push_command(command);
                }
                builder.child_bounds = child_bounds;
                builder.bounds = builder.local_bounds.union(child_bounds);
                if filter {
                    let radius = local_commands.first()?.1.node.style.filter.blur_radius;
                    builder.grow_filter_layer_bounds(incoming, radius * 3.0);
                }
                if compositing {
                    builder.grow_compositing_layer_bounds(incoming);
                }
                bounds = builder.bounds;
                local_bounds = builder.local_bounds;
                for ((index, old), replacement) in local_commands.iter().zip(builder.commands) {
                    if old.clip != replacement.clip {
                        for (&ancestor, (_, _)) in &depth_of {
                            let ancestor_start =
                                self.sparse_paint_index.starts.get(&ancestor)?.start;
                            if start < ancestor_start {
                                continue;
                            }
                            let relative = start + index - ancestor_start;
                            let next = updated.get_mut(&ancestor)?;
                            if next
                                .commands
                                .get(relative)
                                .is_some_and(|c| c.node.id == id && c.kind == replacement.kind)
                            {
                                next.commands = Arc::new(
                                    next.commands.replaced(relative, replacement.clone())?,
                                );
                            }
                        }
                    }
                }
            }
            if let Some(viewport) = previous.scroll_viewport {
                let own = local_commands
                    .iter()
                    .find(|(_, c)| c.kind == DisplayPaintCommandKind::Node)?;
                bounds = command_bounds(&own.1).union(viewport);
                child_bounds = viewport;
            }
            let subtree = updated.get_mut(&id)?;
            subtree.command_span = Some(RetainedSubtreeSpan {
                bounds,
                local_bounds,
                child_bounds,
                ..span
            });
            if let Some(parent) = depth_of[&id].1.filter(|_| span.bounds != bounds) {
                changed_children
                    .entry(parent)
                    .or_default()
                    .push((id, bounds));
            }
        }
        let mut changes = Vec::new();
        for (&id, next) in updated.iter() {
            let previous = self.subtrees.get(&id)?;
            let old = previous.command_span?;
            let next = next.command_span?;
            let scope =
                previous.spans.len() == 1 && previous.spans[0].end == previous.commands.len();
            let (old, new) = if scope {
                (old.bounds, next.bounds)
            } else {
                (old.local_bounds, next.local_bounds)
            };
            if old != new {
                changes.push((id, new));
            }
        }
        for (&id, subtree) in updated.iter_mut() {
            let start = self.sparse_paint_index.starts.get(&id)?.start;
            let mut replacements = Vec::new();
            if subtree.command_span?.includes_scrollbars {
                for (index, span) in subtree.spans.iter().enumerate() {
                    if let Some((_, bounds)) =
                        changes.iter().find(|(owner, _)| *owner == span.owner)
                    {
                        replacements.push((index, *bounds));
                    }
                }
            } else {
                // Without trailing scrollbar spans, paint spans are ordered by
                // command offset; covered descendants have no separate span.
                for &(owner, bounds) in &changes {
                    let Some(relative) = self
                        .sparse_paint_index
                        .starts
                        .get(&owner)?
                        .start
                        .checked_sub(start)
                    else {
                        continue;
                    };
                    let index = subtree.spans.partition_point(|span| span.start < relative);
                    if subtree
                        .spans
                        .get(index)
                        .is_some_and(|span| span.owner == owner && span.start == relative)
                    {
                        replacements.push((index, bounds));
                    }
                }
            }
            if !replacements.is_empty() {
                let mut spans = subtree.spans.to_vec();
                for (index, bounds) in replacements {
                    spans[index].bounds = bounds;
                }
                subtree.spans = spans.into();
            }
        }
        Some(())
    }

    pub(super) fn scroll_metrics_are_local(
        &self,
        root: &WidgetNode,
        dirty: &HashSet<NodeId>,
    ) -> bool {
        dirty.iter().all(|id| {
            let Some(node) = self.live_node(root, *id) else {
                return false;
            };
            let Some(previous) = self.subtrees.get(id) else {
                return false;
            };
            let Some(paint) = previous.commands.first().map(|c| &c.node) else {
                return false;
            };
            let scroll = node.resolved_scroll_metrics();
            let old = &paint.scrollbars;
            if scroll.x == old.scroll_x
                && scroll.y == old.scroll_y
                && scroll.max_x == old.max_x
                && scroll.max_y == old.max_y
                && scroll.content_width == old.content_width
                && scroll.content_height == old.content_height
            {
                return true;
            }
            previous.scroll_viewport.is_some()
                && (scroll.max_x > f32::EPSILON || scroll.max_y > f32::EPSILON)
        })
    }

    fn live_node<'t>(&self, root: &'t WidgetNode, id: NodeId) -> Option<&'t WidgetNode> {
        let mut node = root;
        for &index in self.batch_index.node_path(id)? {
            node = node.children.get(index)?;
        }
        (node.id == id).then_some(node)
    }
}
