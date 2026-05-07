use super::*;

pub(in crate::shell) fn find_node_by_key<'a>(node: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
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

pub(in crate::shell) fn find_node_bounds_by_key(
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

/// Return the root-to-deepest key path under the cursor, regardless of type.
pub(in crate::shell) fn find_node_path_at(node: &WidgetNode, x: f32, y: f32) -> Option<Vec<String>> {
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
pub(in crate::shell) fn node_tooltip_text(node: &WidgetNode) -> Option<String> {
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
pub(in crate::shell) fn find_tooltip_text_by_key(node: &WidgetNode, key: &str) -> Option<String> {
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

pub(in crate::shell) fn is_input_key(tree: &WidgetNode, key: &str) -> bool {
    find_node_by_key(tree, key).is_some_and(|node| node.tag == "input")
}

pub(in crate::shell) fn is_slider_key(tree: &WidgetNode, key: &str) -> bool {
    find_node_by_key(tree, key).is_some_and(|node| node.tag == "slider")
}

pub(in crate::shell) fn find_click_handler(tree: &WidgetNode, key: &str) -> Option<String> {
    find_event_handler(tree, key, "click")
}

pub(in crate::shell) fn find_event_handler(
    tree: &WidgetNode,
    key: &str,
    event_name: &str,
) -> Option<String> {
    find_node_by_key(tree, key)
        .and_then(|node| node.event_handlers.get(event_name))
        .cloned()
}

pub(in crate::shell) fn namespace_event_handlers(node: &mut WidgetNode, instance_key: &str) {
    for handler in node.event_handlers.values_mut() {
        if !handler.starts_with("__mesh_embed__::") {
            *handler = format!("__mesh_embed__::{instance_key}::{handler}");
        }
    }

    for child in &mut node.children {
        namespace_event_handlers(child, instance_key);
    }
}

pub(in crate::shell) fn parse_namespaced_handler(handler: &str) -> Option<(&str, &str)> {
    let rest = handler.strip_prefix("__mesh_embed__::")?;
    rest.rsplit_once("::")
}
