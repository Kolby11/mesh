use mesh_core_elements::WidgetNode;
use mesh_core_module::manifest::SurfaceLayoutSection;
use std::collections::HashMap;

// ScrollOffsetState belongs to the frontend component but layout functions need it.
use super::component::ScrollOffsetState;

pub(super) type ContentBounds = (f32, f32, f32, f32);

#[derive(Debug, Clone)]
struct FocusTraversalTarget {
    key: String,
    tabindex: Option<i32>,
    left: f32,
    top: f32,
    bottom: f32,
    discovery_index: usize,
}

pub(super) fn find_node_by_key<'a>(node: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
    if node
        .attributes
        .get("_mesh_key")
        .is_some_and(|value| value == key)
    {
        return Some(node);
    }

    for child in &node.children {
        if let Some(found) = find_node_by_key(child, key) {
            return Some(found);
        }
    }

    None
}

pub(super) fn find_node_bounds_by_key(
    node: &WidgetNode,
    key: &str,
    offset_x: f32,
    offset_y: f32,
) -> Option<ContentBounds> {
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    if node
        .attributes
        .get("_mesh_key")
        .is_some_and(|value| value == key)
    {
        return Some(node_rect_with_offset(node, offset_x, offset_y));
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in &node.children {
        if let Some(bounds) = find_node_bounds_by_key(child, key, child_offset_x, child_offset_y) {
            return Some(bounds);
        }
    }

    None
}

pub(super) fn find_focusable_at(node: &WidgetNode, x: f32, y: f32) -> Option<String> {
    find_focusable_at_with_offset(node, x, y, 0.0, 0.0)
}

pub(super) fn collect_focus_traversal(node: &WidgetNode) -> Vec<String> {
    let mut targets = Vec::new();
    collect_focus_traversal_with_offset(node, 0.0, 0.0, None, &mut targets);

    targets.sort_by(|left, right| compare_focus_targets(left, right));
    targets.into_iter().map(|target| target.key).collect()
}

pub(super) fn next_focus_target(
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

pub(super) fn find_scrollable_at(node: &WidgetNode, x: f32, y: f32) -> Option<String> {
    find_scrollable_at_with_offset(node, x, y, 0.0, 0.0)
}

/// Return the root-to-deepest key path under the cursor, regardless of type.
pub(super) fn find_node_path_at(node: &WidgetNode, x: f32, y: f32) -> Option<Vec<String>> {
    find_node_path_at_offset(node, x, y, 0.0, 0.0)
}

fn find_node_path_at_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<Vec<String>> {
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside && node_clips_children(node) {
        return None;
    }

    let (child_ox, child_oy) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in node.children.iter().rev() {
        if let Some(mut path) = find_node_path_at_offset(child, x, y, child_ox, child_oy) {
            if let Some(key) = node.attributes.get("_mesh_key") {
                path.insert(0, key.clone());
            }
            return Some(path);
        }
    }

    if inside {
        return node
            .attributes
            .get("_mesh_key")
            .map(|key| vec![key.clone()]);
    }

    None
}

/// Extract tooltip text from a node's attributes and accessibility metadata.
pub(super) fn node_tooltip_text(node: &WidgetNode) -> Option<String> {
    node.attributes
        .get("title")
        .cloned()
        .or_else(|| node.attributes.get("aria-label").cloned())
        .or_else(|| node.attributes.get("description").cloned())
        .or_else(|| node.attributes.get("aria-description").cloned())
        .or_else(|| node.accessibility.label.clone())
        .or_else(|| node.accessibility.description.clone())
}

/// Find tooltip text for a specific node key in the tree.
pub(super) fn find_tooltip_text_by_key(node: &WidgetNode, key: &str) -> Option<String> {
    if node.attributes.get("_mesh_key").is_some_and(|k| k == key) {
        return node_tooltip_text(node);
    }
    for child in &node.children {
        if let Some(text) = find_tooltip_text_by_key(child, key) {
            return Some(text);
        }
    }
    None
}

pub(super) fn is_input_key(tree: &WidgetNode, key: &str) -> bool {
    find_node_by_key(tree, key).is_some_and(|node| node.tag == "input")
}

pub(super) fn is_slider_key(tree: &WidgetNode, key: &str) -> bool {
    find_node_by_key(tree, key).is_some_and(|node| node.tag == "slider")
}

pub(super) fn find_click_handler(tree: &WidgetNode, key: &str) -> Option<String> {
    find_event_handler(tree, key, "click")
}

pub(super) fn find_event_handler(tree: &WidgetNode, key: &str, event_name: &str) -> Option<String> {
    find_node_by_key(tree, key)
        .and_then(|node| node.event_handlers.get(event_name))
        .cloned()
}

pub(super) fn namespace_event_handlers(node: &mut WidgetNode, instance_key: &str) {
    for handler in node.event_handlers.values_mut() {
        if !handler.starts_with("__mesh_embed__::") {
            *handler = format!("__mesh_embed__::{instance_key}::{handler}");
        }
    }

    for child in &mut node.children {
        namespace_event_handlers(child, instance_key);
    }
}

pub(super) fn parse_namespaced_handler(handler: &str) -> Option<(&str, &str)> {
    let rest = handler.strip_prefix("__mesh_embed__::")?;
    rest.rsplit_once("::")
}

pub(super) fn measure_content_size(
    tree: &WidgetNode,
    fallback_width: u32,
    fallback_height: u32,
    surface_layout: Option<&SurfaceLayoutSection>,
) -> (u32, u32) {
    let prefers_children = surface_layout
        .and_then(|sl| sl.prefers_content_children_sizing)
        .unwrap_or(false);

    let bounds = if prefers_children {
        content_children_bounds(tree, 0.0, 0.0).or_else(|| content_bounds(tree, 0.0, 0.0))
    } else {
        content_bounds(tree, 0.0, 0.0)
    };

    let width = bounds
        .map(|(_, _, right, _)| right.ceil().max(1.0) as u32)
        .unwrap_or(fallback_width);
    let height = bounds
        .map(|(_, _, _, bottom)| bottom.ceil().max(1.0) as u32)
        .unwrap_or(fallback_height);

    if let Some(sl) = surface_layout {
        let w = match (sl.min_width, sl.max_width) {
            (Some(min), Some(max)) => width.clamp(min, max),
            _ => fallback_width,
        };
        let h = match (sl.min_height, sl.max_height) {
            (Some(min), Some(max)) => height.clamp(min, max),
            _ => fallback_height,
        };
        (w, h)
    } else {
        (fallback_width, fallback_height)
    }
}

pub(super) fn annotate_overflow_tree(
    node: &mut WidgetNode,
    key: &str,
    scroll_offsets: &mut HashMap<String, ScrollOffsetState>,
) -> Option<ContentBounds> {
    let mut children_bounds: Option<ContentBounds> = None;

    for (index, child) in node.children.iter_mut().enumerate() {
        if let Some(child_bounds) =
            annotate_overflow_tree(child, &format!("{key}/{index}"), scroll_offsets)
        {
            children_bounds = Some(union_bounds(children_bounds, child_bounds));
        }
    }

    let content_origin_x = node.layout.x + node.computed_style.padding.left;
    let content_origin_y = node.layout.y + node.computed_style.padding.top;
    let viewport_width = (node.layout.width - node.computed_style.padding.horizontal()).max(0.0);
    let viewport_height = (node.layout.height - node.computed_style.padding.vertical()).max(0.0);

    let content_width = children_bounds
        .map(|(_, _, max_x, _)| (max_x - content_origin_x).max(0.0))
        .unwrap_or(0.0);
    let content_height = children_bounds
        .map(|(_, _, _, max_y)| (max_y - content_origin_y).max(0.0))
        .unwrap_or(0.0);

    let max_x = if node.computed_style.overflow_x.clips_contents() {
        (content_width - viewport_width).max(0.0)
    } else {
        0.0
    };
    let max_y = if node.computed_style.overflow_y.clips_contents() {
        (content_height - viewport_height).max(0.0)
    } else {
        0.0
    };

    let offset = scroll_offsets.entry(key.to_string()).or_default();
    offset.x = offset.x.clamp(0.0, max_x);
    offset.y = offset.y.clamp(0.0, max_y);

    node.attributes
        .insert("_mesh_content_width".into(), format!("{content_width:.2}"));
    node.attributes.insert(
        "_mesh_content_height".into(),
        format!("{content_height:.2}"),
    );
    node.attributes
        .insert("_mesh_scroll_max_x".into(), format!("{max_x:.2}"));
    node.attributes
        .insert("_mesh_scroll_max_y".into(), format!("{max_y:.2}"));
    node.attributes
        .insert("_mesh_scroll_x".into(), format!("{:.2}", offset.x));
    node.attributes
        .insert("_mesh_scroll_y".into(), format!("{:.2}", offset.y));

    let own_bounds = (
        node.layout.x,
        node.layout.y,
        node.layout.x + node.layout.width.max(0.0),
        node.layout.y + node.layout.height.max(0.0),
    );
    if node_clips_children(node) {
        Some(own_bounds)
    } else {
        Some(union_bounds(
            Some(own_bounds),
            children_bounds.unwrap_or(own_bounds),
        ))
    }
}

fn union_bounds(existing: Option<ContentBounds>, next: ContentBounds) -> ContentBounds {
    match existing {
        Some((min_x, min_y, max_x, max_y)) => (
            min_x.min(next.0),
            min_y.min(next.1),
            max_x.max(next.2),
            max_y.max(next.3),
        ),
        None => next,
    }
}

fn intersect_bounds(a: ContentBounds, b: ContentBounds) -> Option<ContentBounds> {
    let left = a.0.max(b.0);
    let top = a.1.max(b.1);
    let right = a.2.min(b.2);
    let bottom = a.3.min(b.3);
    if right <= left || bottom <= top {
        None
    } else {
        Some((left, top, right, bottom))
    }
}

fn node_rect_with_offset(node: &WidgetNode, offset_x: f32, offset_y: f32) -> ContentBounds {
    (
        node.layout.x + offset_x,
        node.layout.y + offset_y,
        node.layout.x + offset_x + node.layout.width.max(0.0),
        node.layout.y + offset_y + node.layout.height.max(0.0),
    )
}

fn node_scroll_offset(node: &WidgetNode) -> ScrollOffsetState {
    ScrollOffsetState {
        x: parse_node_attr_f32(node, "_mesh_scroll_x"),
        y: parse_node_attr_f32(node, "_mesh_scroll_y"),
    }
}

pub(super) fn scroll_limits(node: &WidgetNode) -> (f32, f32) {
    (
        parse_node_attr_f32(node, "_mesh_scroll_max_x"),
        parse_node_attr_f32(node, "_mesh_scroll_max_y"),
    )
}

fn parse_node_attr_f32(node: &WidgetNode, key: &str) -> f32 {
    node.attributes
        .get(key)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0)
}

fn node_clips_children(node: &WidgetNode) -> bool {
    node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents()
}

fn node_is_scrollable(node: &WidgetNode) -> bool {
    let (max_x, max_y) = scroll_limits(node);
    max_x > f32::EPSILON || max_y > f32::EPSILON
}

fn child_offsets_with_scroll(node: &WidgetNode, offset_x: f32, offset_y: f32) -> (f32, f32) {
    let scroll = node_scroll_offset(node);
    (offset_x - scroll.x, offset_y - scroll.y)
}

/// Translate the incoming offset by this node's CSS `transform.translate_*`,
/// mirroring what the painter does. Hit-testing must apply the same shift so
/// pointer coordinates resolve to the visually displaced bounds, not the
/// untransformed layout box. Scale and rotation are not yet visually
/// rendered (see `mesh_core_render::animation::transform::is_paintable`)
/// and so are not yet inverted here either.
fn apply_transform_offset(node: &WidgetNode, offset_x: f32, offset_y: f32) -> (f32, f32) {
    let t = node.computed_style.transform;
    (offset_x + t.translate_x, offset_y + t.translate_y)
}

fn content_children_bounds(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> Option<ContentBounds> {
    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    let child_clip = if node_clips_children(node) {
        Some(node_rect_with_offset(node, offset_x, offset_y))
    } else {
        None
    };

    let mut bounds: Option<ContentBounds> = None;
    for child in &node.children {
        if let Some(child_bounds) =
            content_bounds_with_clip(child, child_offset_x, child_offset_y, child_clip)
        {
            bounds = Some(union_bounds(bounds, child_bounds));
        }
    }

    bounds
}

fn content_bounds(node: &WidgetNode, offset_x: f32, offset_y: f32) -> Option<ContentBounds> {
    content_bounds_with_clip(node, offset_x, offset_y, None)
}

fn content_bounds_with_clip(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    clip: Option<ContentBounds>,
) -> Option<ContentBounds> {
    if node.computed_style.display == mesh_core_elements::style::Display::None {
        return None;
    }

    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let rect = node_rect_with_offset(node, offset_x, offset_y);
    let own_bounds = match clip {
        Some(clip_bounds) => intersect_bounds(rect, clip_bounds),
        None => Some(rect),
    };
    if clip.is_some() && own_bounds.is_none() {
        return None;
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

    let mut bounds = own_bounds;
    for child in &node.children {
        if let Some(child_bounds) =
            content_bounds_with_clip(child, child_offset_x, child_offset_y, child_clip)
        {
            bounds = Some(union_bounds(bounds, child_bounds));
        }
    }

    bounds
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

fn find_scrollable_at_with_offset(
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
            find_scrollable_at_with_offset(child, x, y, child_offset_x, child_offset_y)
        {
            return Some(found);
        }
    }

    if inside_self && node_is_scrollable(node) {
        return node.attributes.get("_mesh_key").cloned();
    }

    None
}

fn layout_contains_with_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> bool {
    let left = node.layout.x + offset_x;
    let top = node.layout.y + offset_y;
    x >= left && x < left + node.layout.width && y >= top && y < top + node.layout.height
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
    matches!(
        node.tag.as_str(),
        "input" | "button" | "slider" | "switch" | "checkbox"
    )
}

fn node_is_hidden(node: &WidgetNode) -> bool {
    node.computed_style.display == mesh_core_elements::style::Display::None
        || node.layout.width <= 0.0
        || node.layout.height <= 0.0
        || node
            .attributes
            .get("hidden")
            .is_some_and(|value| truthy_attribute(value))
}

fn node_is_disabled(node: &WidgetNode) -> bool {
    node.attributes
        .get("disabled")
        .is_some_and(|value| truthy_attribute(value))
        || node
            .attributes
            .get("aria-disabled")
            .is_some_and(|value| truthy_attribute(value))
}

fn truthy_attribute(value: &str) -> bool {
    matches!(value, "" | "true" | "1" | "disabled" | "checked")
}
