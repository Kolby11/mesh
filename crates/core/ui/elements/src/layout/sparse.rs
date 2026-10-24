//! Layout write-back for stable-tree patches. After a full pass,
//! [`seed_projection`] records each node's Taffy rect and fit extents;
//! [`write_sparse_layout`] then rewrites only the dirty branches and the
//! subtrees whose projected rect or visibility changed, and returns the nodes
//! whose final geometry differs.

use super::*;

pub(super) struct LayoutProjection {
    raw: LayoutRect,
    /// Unrounded Taffy-tree origin; descendants round their sizes against it.
    cumulative: (f32, f32),
    final_layout: LayoutRect,
    extents: (f32, f32),
    pub(super) hidden: bool,
    pub(super) hidden_ancestor: bool,
}

pub(super) fn seed_projection(root: &WidgetNode, state: &mut PerSurfaceLayoutState) {
    fn seed(
        node: &WidgetNode,
        state: &mut PerSurfaceLayoutState,
        parent: Option<(&WidgetNode, bool)>,
        parent_cumulative: (f32, f32),
        changed: &mut HashSet<NodeId>,
        complete: &mut bool,
    ) -> (f32, f32) {
        crate::frame_work::visit(crate::frame_work::Pass::LayoutExtents);
        match state.projection.get(&node.id) {
            Some(previous) if previous.final_layout != node.layout => {
                changed.insert(node.id);
            }
            None => *complete = false,
            _ => {}
        }
        let hidden_ancestor = parent.is_some_and(|(_, hidden)| hidden);
        let hidden = hidden_ancestor
            || node.computed_style.display == Display::None
            || node.is_closed_container();
        let (x, y, padding) = parent.map_or((0.0, 0.0, None), |(parent, _)| {
            if node.computed_style.position == Position::Fixed {
                (0.0, 0.0, None)
            } else {
                (
                    parent.layout.x,
                    parent.layout.y,
                    Some(parent.computed_style.padding),
                )
            }
        });
        let (raw, cumulative) = if hidden {
            (LayoutRect::default(), parent_cumulative)
        } else {
            projected_taffy_rect(
                node,
                &state.tree,
                &state.node_map,
                padding,
                x,
                y,
                parent_cumulative,
                state.last_available.0,
                state.last_available.1,
            )
        };
        let mut extents = (
            node.layout.x + node.layout.width,
            node.layout.y + node.layout.height,
        );
        for child in &node.children {
            let child_extents = seed(
                child,
                state,
                Some((node, hidden)),
                cumulative,
                changed,
                complete,
            );
            if !child.is_promoted_popover() && child.computed_style.position != Position::Fixed {
                extents.0 = extents.0.max(child_extents.0);
                extents.1 = extents.1.max(child_extents.1);
            }
        }
        let extents = super::lowering::clip_layout_extents(node, extents);
        state.projection.insert(
            node.id,
            LayoutProjection {
                raw,
                cumulative,
                final_layout: node.layout,
                extents,
                hidden,
                hidden_ancestor,
            },
        );
        extents
    }
    let previous_len = state.projection.len();
    let mut changed = HashSet::new();
    let mut complete = true;
    seed(root, state, None, (0.0, 0.0), &mut changed, &mut complete);
    if !complete || previous_len != state.node_map.len() {
        state
            .projection
            .retain(|id, _| state.node_map.contains_key(id));
    }
    state.changed_nodes = (complete && previous_len == state.node_map.len()).then_some(changed);
}

pub(super) fn write_sparse_layout(
    root: &mut WidgetNode,
    state: &mut PerSurfaceLayoutState,
    dirty: &[WidgetNode],
    width: f32,
    height: f32,
) -> HashSet<NodeId> {
    let mut branches = HashSet::new();
    for node in dirty {
        let mut current = state.node_map.get(&node.id).copied();
        while let Some(id) = current {
            if !branches.insert(id) {
                break;
            }
            current = state.tree.parent(id);
        }
    }
    let mut changed = HashSet::new();
    write(
        root,
        state,
        &branches,
        None,
        0.0,
        0.0,
        (0.0, 0.0),
        false,
        width,
        height,
        &mut changed,
    );
    changed
}

#[allow(clippy::too_many_arguments)]
fn write(
    node: &mut WidgetNode,
    state: &mut PerSurfaceLayoutState,
    branches: &HashSet<TaffyNodeId>,
    parent_padding: Option<Edges>,
    parent_x: f32,
    parent_y: f32,
    parent_cumulative: (f32, f32),
    hidden_ancestor: bool,
    width: f32,
    height: f32,
    changed: &mut HashSet<NodeId>,
) -> (f32, f32) {
    crate::frame_work::visit(crate::frame_work::Pass::LayoutWrite);
    let hidden = hidden_ancestor
        || node.computed_style.display == Display::None
        || node.is_closed_container();
    let (raw, cumulative) = if hidden {
        (LayoutRect::default(), parent_cumulative)
    } else {
        projected_taffy_rect(
            node,
            &state.tree,
            &state.node_map,
            parent_padding,
            parent_x,
            parent_y,
            parent_cumulative,
            width,
            height,
        )
    };
    let dirty_branch = state
        .node_map
        .get(&node.id)
        .is_some_and(|id| branches.contains(id));
    if !dirty_branch
        && let Some(old) = state.projection.get(&node.id)
        && old.raw == raw
        && old.cumulative == cumulative
        && old.hidden == hidden
        && old.hidden_ancestor == hidden_ancestor
    {
        return old.extents;
    }
    let previous = node.layout;
    node.layout = raw;
    let mut extents = (raw.x + raw.width, raw.y + raw.height);
    for child in &mut node.children {
        let fixed = child.computed_style.position == Position::Fixed;
        let (x, y, padding) = if fixed {
            (0.0, 0.0, None)
        } else {
            (raw.x, raw.y, Some(node.computed_style.padding))
        };
        let child_extents = write(
            child, state, branches, padding, x, y, cumulative, hidden, width, height, changed,
        );
        if !child.is_promoted_popover() && !fixed {
            extents.0 = extents.0.max(child_extents.0);
            extents.1 = extents.1.max(child_extents.1);
        }
    }
    crate::frame_work::visit(crate::frame_work::Pass::LayoutFit);
    crate::frame_work::visit(crate::frame_work::Pass::LayoutExtents);
    if !hidden {
        if matches!(node.computed_style.width, Dimension::Fit) {
            node.layout.width = (extents.0 - raw.x).max(0.0);
        }
        if matches!(node.computed_style.height, Dimension::Fit) {
            node.layout.height = (extents.1 - raw.y).max(0.0);
        }
    }
    let extents = super::lowering::clip_layout_extents(node, extents);
    if node.layout != previous {
        changed.insert(node.id);
    }
    state.projection.insert(
        node.id,
        LayoutProjection {
            raw,
            cumulative,
            final_layout: node.layout,
            extents,
            hidden,
            hidden_ancestor,
        },
    );
    extents
}
