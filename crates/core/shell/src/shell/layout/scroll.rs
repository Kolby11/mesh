use super::*;
use mesh_core_module::manifest::SurfaceLayoutSection;
use std::collections::HashMap;

pub(in crate::shell) fn find_scrollable_at(node: &WidgetNode, x: f32, y: f32) -> Option<String> {
    find_scrollable_at_with_offset(node, x, y, 0.0, 0.0)
}

pub(in crate::shell) fn scroll_limits(node: &WidgetNode) -> (f32, f32) {
    (
        parse_node_attr_f32(node, "_mesh_scroll_max_x"),
        parse_node_attr_f32(node, "_mesh_scroll_max_y"),
    )
}

fn node_is_scrollable(node: &WidgetNode) -> bool {
    let (max_x, max_y) = scroll_limits(node);
    max_x > f32::EPSILON || max_y > f32::EPSILON
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

pub(in crate::shell) fn measure_content_size(
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

pub(in crate::shell) fn annotate_overflow_tree(
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
