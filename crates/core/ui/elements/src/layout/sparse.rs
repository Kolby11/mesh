//! Producer-directed projection of geometry and exact retained child extents.

use super::*;
use crate::bounds_index::ChildBoundsIndex;
use std::collections::BTreeMap;

pub(super) struct LayoutProjection {
    raw: LayoutRect,
    cumulative: (f32, f32),
    final_layout: LayoutRect,
    padding: Edges,
    path: Vec<usize>,
    children: ChildBoundsIndex,
    pub(super) hidden: bool,
    pub(super) hidden_ancestor: bool,
}

#[derive(Default)]
struct Branches {
    children: BTreeMap<usize, Branches>,
}
impl Branches {
    fn insert(&mut self, path: &[usize]) {
        let mut branch = self;
        for &index in path {
            branch = branch.children.entry(index).or_default();
        }
    }
}

pub(super) fn write_full_layout(
    root: &mut WidgetNode,
    state: &mut PerSurfaceLayoutState,
    width: f32,
    height: f32,
) {
    let previous_len = state.projection.len();
    let mut changed = HashSet::new();
    let mut complete = true;
    state.reverse_nodes.clear();
    write(
        root,
        state,
        None,
        None,
        0.0,
        0.0,
        (0.0, 0.0),
        false,
        width,
        height,
        &mut changed,
        &mut complete,
        &mut Vec::new(),
    );
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
) -> Option<HashSet<NodeId>> {
    let mut branches = Branches::default();
    let changed_ids = state
        .tree
        .changes
        .iter()
        .filter(|(_, before, after)| before != after)
        .map(|(id, _, _)| state.reverse_nodes.get(id).copied())
        .collect::<Option<Vec<_>>>();
    let Some(changed_ids) = changed_ids else {
        write_full_layout(root, state, width, height);
        return state.changed_nodes.take();
    };
    // A dense change set gains nothing from building a branch index. The full
    // traversal also rebuilds child unions in linear order instead of point
    // updating most leaves of each retained index.
    if changed_ids.len().saturating_mul(2) > state.node_map.len() {
        write_full_layout(root, state, width, height);
        return state.changed_nodes.take();
    }
    for id in changed_ids
        .into_iter()
        .chain(dirty.iter().map(|node| node.id))
    {
        let Some(old) = state.projection.get(&id) else {
            write_full_layout(root, state, width, height);
            return state.changed_nodes.take();
        };
        branches.insert(&old.path);
    }
    let mut changed = HashSet::new();
    let mut complete = true;
    write(
        root,
        state,
        Some(&branches),
        None,
        0.0,
        0.0,
        (0.0, 0.0),
        false,
        width,
        height,
        &mut changed,
        &mut complete,
        &mut Vec::new(),
    );
    complete.then_some(changed)
}

#[allow(clippy::too_many_arguments)]
fn write(
    node: &mut WidgetNode,
    state: &mut PerSurfaceLayoutState,
    branches: Option<&Branches>,
    parent_padding: Option<Edges>,
    parent_x: f32,
    parent_y: f32,
    parent_cumulative: (f32, f32),
    hidden_ancestor: bool,
    width: f32,
    height: f32,
    changed: &mut HashSet<NodeId>,
    complete: &mut bool,
    path: &mut Vec<usize>,
) -> (f32, f32) {
    crate::frame_work::visit(crate::frame_work::Pass::LayoutWrite);
    let mut old = state.projection.remove(&node.id);
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
    let propagate = old.as_ref().is_none_or(|old| {
        old.raw.x != raw.x
            || old.raw.y != raw.y
            || old.cumulative != cumulative
            || old.hidden != hidden
            || old.padding != node.computed_style.padding
            || old.hidden_ancestor != hidden_ancestor
    });
    let mut recorded_path = old
        .as_mut()
        .map(|old| std::mem::take(&mut old.path))
        .unwrap_or_default();
    recorded_path.clear();
    recorded_path.extend_from_slice(path);
    let previous = old.as_ref().map(|old| old.final_layout);
    if previous.is_none() {
        *complete = false;
    }
    let rebuild_children = branches.is_none()
        || old
            .as_ref()
            .is_none_or(|old| old.children.len() != node.children.len());
    let mut children = if rebuild_children {
        ChildBoundsIndex::new(Vec::new())
    } else {
        old.expect("retained projection").children
    };
    node.layout = raw;
    let child_count = node.children.len();
    let mut update_child = |index: usize, branch: Option<&Branches>| {
        let child = &mut node.children[index];
        let fixed = child.computed_style.position == Position::Fixed;
        let (x, y, padding) = if fixed {
            (0.0, 0.0, None)
        } else {
            (raw.x, raw.y, Some(node.computed_style.padding))
        };
        path.push(index);
        let extents = write(
            child, state, branch, padding, x, y, cumulative, hidden, width, height, changed,
            complete, path,
        );
        path.pop();
        (!child.is_promoted_popover() && !fixed).then_some((0.0, 0.0, extents.0, extents.1))
    };
    if rebuild_children {
        let mut bounds = Vec::with_capacity(child_count);
        for index in 0..child_count {
            bounds.push(update_child(index, None));
        }
        children = ChildBoundsIndex::new(bounds);
    } else if propagate {
        let empty = Branches::default();
        for index in 0..children.len() {
            let branch = branches
                .and_then(|branch| branch.children.get(&index))
                .unwrap_or(&empty);
            children.set(index, update_child(index, Some(branch)));
        }
    } else if let Some(branches) = branches {
        for (&index, branch) in &branches.children {
            children.set(index, update_child(index, Some(branch)));
        }
    }
    let child_extents =
        children
            .bounds()
            .unwrap_or((0.0, 0.0, f32::NEG_INFINITY, f32::NEG_INFINITY));
    let extents = (
        (raw.x + raw.width).max(child_extents.2),
        (raw.y + raw.height).max(child_extents.3),
    );
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
    if previous.is_some_and(|previous| node.layout != previous) {
        changed.insert(node.id);
    }
    if let Some(id) = state.node_map.get(&node.id) {
        state.reverse_nodes.insert(*id, node.id);
    }
    state.projection.insert(
        node.id,
        LayoutProjection {
            raw,
            cumulative,
            final_layout: node.layout,
            hidden,
            hidden_ancestor,
            padding: node.computed_style.padding,
            path: recorded_path,
            children,
        },
    );
    extents
}
