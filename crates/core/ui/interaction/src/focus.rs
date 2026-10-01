use super::*;

#[derive(Debug, Clone)]
struct FocusTraversalTarget {
    key: String,
    tabindex: Option<i32>,
    left: f32,
    top: f32,
    bottom: f32,
    discovery_index: usize,
}

pub fn find_focusable_at(node: &WidgetNode, x: f32, y: f32) -> Option<String> {
    find_focusable_at_with_transform(
        node,
        x,
        y,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
    )
}

pub fn collect_focus_traversal(node: &WidgetNode) -> Vec<String> {
    let mut targets = Vec::new();
    collect_focus_traversal_with_transform(
        node,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
        NodeEligibility::ROOT,
        &mut targets,
    );

    order_focus_targets(&mut targets);
    targets.into_iter().map(|target| target.key).collect()
}

pub fn next_focus_target(
    node: &WidgetNode,
    current: Option<&str>,
    backward: bool,
) -> Option<String> {
    let traversal = collect_focus_traversal(node);
    if traversal.is_empty() {
        return None;
    }

    let current_index =
        current.and_then(|key| traversal.iter().position(|candidate| candidate == key));
    let next_index = match (current_index, backward) {
        (Some(index), false) => (index + 1) % traversal.len(),
        (Some(index), true) => {
            if index == 0 {
                traversal.len() - 1
            } else {
                index - 1
            }
        }
        (None, false) => 0,
        (None, true) => traversal.len() - 1,
    };

    traversal.get(next_index).cloned()
}

fn find_focusable_at_with_transform(
    node: &WidgetNode,
    x: f32,
    y: f32,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
) -> Option<String> {
    if !node_allows(node, InteractionTarget::Focus) {
        return None;
    }
    let world = node_world_transform(parent_transform, node);
    if !clips.contains(x, y) {
        return None;
    }
    let inside_self = node_contains_with_transform(node, world, x, y);
    if !inside_self && node_clips_children(node) {
        return None;
    }

    let child_transform = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);

    for child in mesh_core_elements::children_in_paint_order(node).rev() {
        if let Some(found) =
            find_focusable_at_with_transform(child, x, y, child_transform, &child_clips)
        {
            return Some(found);
        }
    }

    if inside_self && node_is_pointer_focusable(node) {
        return node.mesh_key().map(str::to_owned);
    }

    None
}

fn collect_focus_traversal_with_transform(
    node: &WidgetNode,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
    parent_policy: NodeEligibility,
    targets: &mut Vec<FocusTraversalTarget>,
) {
    let policy = parent_policy.child(node);
    if !policy.allows(InteractionTarget::Focus) {
        return;
    }

    let world = node_world_transform(parent_transform, node);
    let rect = node_rect_with_transform(node, world);
    if node_has_layout_geometry(node) && world.inverse().is_none() {
        return;
    }
    if node_has_layout_geometry(node) && !clips.intersects_rect(world, node_local_rect(node)) {
        return;
    }
    let visible_rect = if clips.is_empty() {
        Some(rect)
    } else {
        clips
            .bounds()
            .and_then(|clip| intersect_bounds(rect, content_bounds_from_rect(clip)))
    };
    if visible_rect.is_none() {
        return;
    }

    if node_is_tabbable(node)
        && let Some(key) = node.mesh_key()
    {
        let (left, top, _right, bottom) = visible_rect.unwrap_or(rect);
        targets.push(FocusTraversalTarget {
            key: key.to_owned(),
            tabindex: parse_tabindex(node),
            left,
            top,
            bottom,
            discovery_index: targets.len(),
        });
    }

    let child_transform = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);

    for child in &node.children {
        collect_focus_traversal_with_transform(
            child,
            child_transform,
            &child_clips,
            policy,
            targets,
        );
    }
}

fn content_bounds_from_rect(rect: mesh_core_elements::LayoutRect) -> ContentBounds {
    (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height)
}

/// Order targets for Tab traversal: positive `tabindex` first by value, then
/// everything else in reading order. Reading order bands targets into rows by
/// vertical overlap with the row's first target, then orders each row left to
/// right. Banding is a single pass over a total order, so the result does not
/// depend on the sort algorithm the way a pairwise overlap test would.
fn order_focus_targets(targets: &mut [FocusTraversalTarget]) {
    targets.sort_by(|left, right| {
        compare_f32(left.top, right.top)
            .then_with(|| compare_f32(left.left, right.left))
            .then_with(|| left.discovery_index.cmp(&right.discovery_index))
    });
    let mut row_start = 0;
    while row_start < targets.len() {
        let row_bottom = targets[row_start].bottom;
        let mut row_end = row_start + 1;
        while row_end < targets.len() && targets[row_end].top < row_bottom {
            row_end += 1;
        }
        targets[row_start..row_end].sort_by(|left, right| {
            compare_f32(left.left, right.left)
                .then_with(|| compare_f32(left.top, right.top))
                .then_with(|| left.discovery_index.cmp(&right.discovery_index))
        });
        row_start = row_end;
    }
    // Discovery indices are the contiguous push order 0..len.
    let mut reading_rank = vec![0; targets.len()];
    for (rank, target) in targets.iter().enumerate() {
        reading_rank[target.discovery_index] = rank;
    }
    targets.sort_by_key(|target| {
        let tabindex = target.tabindex.unwrap_or(0);
        (
            tabindex <= 0,
            tabindex.max(0),
            reading_rank[target.discovery_index],
        )
    });
}

fn compare_f32(left: f32, right: f32) -> std::cmp::Ordering {
    left.partial_cmp(&right)
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn parse_tabindex(node: &WidgetNode) -> Option<i32> {
    node.attributes
        .get("tabindex")
        .and_then(|value| value.parse::<i32>().ok())
}

pub(crate) fn node_is_pointer_focusable(node: &WidgetNode) -> bool {
    node_allows(node, InteractionTarget::Focus)
        && (node_is_native_focusable(node) || parse_tabindex(node).is_some())
}

fn node_is_tabbable(node: &WidgetNode) -> bool {
    if !node_allows(node, InteractionTarget::Focus) {
        return false;
    }

    match parse_tabindex(node) {
        Some(value) => value >= 0,
        None => node_is_native_focusable(node),
    }
}

fn node_is_native_focusable(node: &WidgetNode) -> bool {
    matches!(node.tag.as_str(), "input" | "button" | "slider")
        || crate::node_is_source(
            node,
            &[
                "select",
                "option",
                "switch",
                "checkbox",
                "radio",
                "segmented-control",
                "menu",
                "menu-item",
                "command-item",
                "preference-row",
                "tab",
                "list-item",
            ],
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(key: &str, left: f32, top: f32, bottom: f32, index: usize) -> FocusTraversalTarget {
        FocusTraversalTarget {
            key: key.into(),
            tabindex: None,
            left,
            top,
            bottom,
            discovery_index: index,
        }
    }

    /// A overlaps B and B overlaps C, but A and C do not overlap and C sits
    /// above A: a pairwise row test orders A<B, B<C and C<A.
    #[test]
    fn mixed_height_controls_get_one_order_whatever_the_input_order() {
        let shapes = [("a", 0.0, 10.0, 20.0), ("b", 5.0, 0.0, 40.0), ("c", 10.0, 0.0, 5.0)];
        let permutations = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
        for permutation in permutations {
            let mut targets = permutation
                .iter()
                .enumerate()
                .map(|(index, &shape)| {
                    let (key, left, top, bottom) = shapes[shape];
                    target(key, left, top, bottom, index)
                })
                .collect::<Vec<_>>();
            order_focus_targets(&mut targets);
            let keys = targets.iter().map(|target| target.key.as_str()).collect::<Vec<_>>();
            assert_eq!(keys, ["a", "b", "c"], "input order {permutation:?}");
        }
    }

    #[test]
    fn positive_tabindex_comes_first_in_tabindex_order() {
        let mut targets = vec![
            target("plain", 0.0, 0.0, 10.0, 0),
            target("second", 50.0, 0.0, 10.0, 1),
            target("first", 90.0, 0.0, 10.0, 2),
        ];
        targets[1].tabindex = Some(2);
        targets[2].tabindex = Some(1);
        order_focus_targets(&mut targets);
        let keys = targets.iter().map(|target| target.key.as_str()).collect::<Vec<_>>();
        assert_eq!(keys, ["first", "second", "plain"]);
    }
}
