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
    find_focusable_at_with_offset(node, x, y, 0.0, 0.0)
}

pub fn collect_focus_traversal(node: &WidgetNode) -> Vec<String> {
    let mut targets = Vec::new();
    collect_focus_traversal_with_offset(node, 0.0, 0.0, None, &mut targets);

    targets.sort_by(|left, right| compare_focus_targets(left, right));
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

fn find_focusable_at_with_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<String> {
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let inside_self = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside_self && node_clips_children(node) {
        return None;
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);

    for child in node.children.iter().rev() {
        if let Some(found) =
            find_focusable_at_with_offset(child, x, y, child_offset_x, child_offset_y)
        {
            return Some(found);
        }
    }

    if inside_self && node_is_pointer_focusable(node) {
        return node.attributes.get("_mesh_key").cloned();
    }

    None
}

fn collect_focus_traversal_with_offset(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    clip: Option<ContentBounds>,
    targets: &mut Vec<FocusTraversalTarget>,
) {
    if node_is_hidden(node) {
        return;
    }

    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let rect = node_rect_with_offset(node, offset_x, offset_y);
    let visible_rect = match clip {
        Some(clip_bounds) => intersect_bounds(rect, clip_bounds),
        None => Some(rect),
    };
    if clip.is_some() && visible_rect.is_none() {
        return;
    }

    if node_is_tabbable(node)
        && let Some(key) = node.attributes.get("_mesh_key")
    {
        let (left, top, _right, bottom) = visible_rect.unwrap_or(rect);
        targets.push(FocusTraversalTarget {
            key: key.clone(),
            tabindex: parse_tabindex(node),
            left,
            top,
            bottom,
            discovery_index: targets.len(),
        });
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    let child_clip = if node_clips_children(node) {
        match clip {
            Some(clip_bounds) => intersect_bounds(rect, clip_bounds),
            None => Some(rect),
        }
    } else {
        clip
    };

    for child in &node.children {
        collect_focus_traversal_with_offset(
            child,
            child_offset_x,
            child_offset_y,
            child_clip,
            targets,
        );
    }
}

fn compare_focus_targets(
    left: &FocusTraversalTarget,
    right: &FocusTraversalTarget,
) -> std::cmp::Ordering {
    match (left.tabindex.unwrap_or(0), right.tabindex.unwrap_or(0)) {
        (l, r) if l > 0 && r > 0 => l.cmp(&r).then_with(|| compare_focus_geometry(left, right)),
        (l, _) if l > 0 => std::cmp::Ordering::Less,
        (_, r) if r > 0 => std::cmp::Ordering::Greater,
        _ => compare_focus_geometry(left, right),
    }
}

fn compare_focus_geometry(
    left: &FocusTraversalTarget,
    right: &FocusTraversalTarget,
) -> std::cmp::Ordering {
    if nodes_share_row(left, right) {
        compare_f32(left.left, right.left)
            .then_with(|| compare_f32(left.top, right.top))
            .then_with(|| left.discovery_index.cmp(&right.discovery_index))
    } else {
        compare_f32(left.top, right.top)
            .then_with(|| compare_f32(left.left, right.left))
            .then_with(|| left.discovery_index.cmp(&right.discovery_index))
    }
}

fn nodes_share_row(left: &FocusTraversalTarget, right: &FocusTraversalTarget) -> bool {
    left.top < right.bottom && right.top < left.bottom
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

fn node_is_pointer_focusable(node: &WidgetNode) -> bool {
    !node_is_hidden(node)
        && !node_is_disabled(node)
        && (node_is_native_focusable(node) || parse_tabindex(node).is_some())
}

fn node_is_tabbable(node: &WidgetNode) -> bool {
    if node_is_hidden(node) || node_is_disabled(node) {
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
            ],
        )
}
