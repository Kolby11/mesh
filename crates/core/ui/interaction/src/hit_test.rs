use super::*;

pub fn find_node_by_key<'a>(node: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
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

pub fn source_element_tag(node: &WidgetNode) -> &str {
    node.attributes
        .get("data-mesh-element")
        .map(String::as_str)
        .unwrap_or(node.tag.as_str())
}

pub fn node_is_source(node: &WidgetNode, tags: &[&str]) -> bool {
    let source = source_element_tag(node);
    tags.iter().any(|tag| *tag == source)
}

pub fn find_node_bounds_by_key(
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
pub fn find_node_path_at(node: &WidgetNode, x: f32, y: f32) -> Option<Vec<String>> {
    find_node_path_at_offset(node, x, y, 0.0, 0.0)
}

fn find_node_path_at_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<Vec<String>> {
    let mut reversed = find_node_path_reversed(node, x, y, offset_x, offset_y)?;
    reversed.reverse();
    Some(reversed)
}

/// Collects the hit path in deepest-first order. The caller reverses once
/// at the top, avoiding O(n) `Vec::insert(0, ...)` at every ancestor.
fn find_node_path_reversed(
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
        if let Some(mut path) = find_node_path_reversed(child, x, y, child_ox, child_oy) {
            if let Some(key) = node.attributes.get("_mesh_key") {
                path.push(key.clone());
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
pub fn node_tooltip_text(node: &WidgetNode) -> Option<String> {
    node_tooltip_text_ref(node).map(str::to_owned)
}

fn node_tooltip_text_ref(node: &WidgetNode) -> Option<&str> {
    for key in [
        "title",
        "tooltip",
        "aria-label",
        "description",
        "aria-description",
    ] {
        if let Some(value) = non_empty_tooltip_text(node.attributes.get(key).map(String::as_str)) {
            return Some(value);
        }
    }

    non_empty_tooltip_text(node.accessibility.label.as_deref())
        .or_else(|| non_empty_tooltip_text(node.accessibility.description.as_deref()))
}

fn non_empty_tooltip_text(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

/// Find tooltip text for a specific node key in the tree.
pub fn find_tooltip_text_by_key(node: &WidgetNode, key: &str) -> Option<String> {
    find_tooltip_by_key(node, key).map(|(_, text)| text)
}

/// Find the tooltip owner key and text for a specific node key in the tree.
pub fn find_tooltip_by_key(node: &WidgetNode, key: &str) -> Option<(String, String)> {
    find_tooltip_by_key_with_inherited(node, key, None).flatten()
}

fn find_tooltip_by_key_with_inherited(
    node: &WidgetNode,
    key: &str,
    inherited: Option<&(String, String)>,
) -> Option<Option<(String, String)>> {
    let owner_text = node_tooltip_owner_text(node);
    let inherited = owner_text.as_ref().or(inherited);
    if node.attributes.get("_mesh_key").is_some_and(|k| k == key) {
        return Some(inherited.cloned());
    }
    for child in &node.children {
        if let Some(text) = find_tooltip_by_key_with_inherited(child, key, inherited) {
            return Some(text);
        }
    }
    None
}

fn node_tooltip_owner_text(node: &WidgetNode) -> Option<(String, String)> {
    node_tooltip_text(node).map(|text| {
        let owner = node
            .attributes
            .get("_mesh_key")
            .cloned()
            .unwrap_or_else(|| format!("anonymous-tooltip-owner:{:p}", node));
        (owner, text)
    })
}

pub fn is_input_key(tree: &WidgetNode, key: &str) -> bool {
    find_node_by_key(tree, key).is_some_and(|node| {
        node.tag == "input"
            && node_is_source(
                node,
                &[
                    "input",
                    "textarea",
                    "search",
                    "password",
                    "number-input",
                    "stepper",
                    "text-input",
                    "password-input",
                    "search-input",
                    "email-input",
                    "url-input",
                ],
            )
    })
}

pub fn is_slider_key(tree: &WidgetNode, key: &str) -> bool {
    find_node_by_key(tree, key).is_some_and(|node| node.tag == "slider")
}

pub fn find_click_handler(tree: &WidgetNode, key: &str) -> Option<String> {
    find_event_handler(tree, key, "click")
}

pub fn find_event_handler(tree: &WidgetNode, key: &str, event_name: &str) -> Option<String> {
    find_node_by_key(tree, key)
        .and_then(|node| node.event_handlers.get(event_name))
        .cloned()
}

pub fn namespace_event_handlers(node: &mut WidgetNode, instance_key: &str) {
    for handler in node.event_handlers.values_mut() {
        if !handler.starts_with("__mesh_embed__::") {
            *handler = format!("__mesh_embed__::{instance_key}::{handler}");
        }
    }

    for child in &mut node.children {
        namespace_event_handlers(child, instance_key);
    }
}

pub fn parse_namespaced_handler(handler: &str) -> Option<(&str, &str)> {
    let rest = handler.strip_prefix("__mesh_embed__::")?;
    rest.rsplit_once("::")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::WidgetNode;

    #[test]
    fn phase87_tooltip_attribute_participates_in_inherited_tooltip_lookup() {
        let mut owner = WidgetNode::new("box");
        owner.attributes.insert("_mesh_key".into(), "owner".into());
        owner
            .attributes
            .insert("tooltip".into(), "Open details".into());

        let mut child = WidgetNode::new("icon");
        child.attributes.insert("_mesh_key".into(), "child".into());
        owner.children.push(child);

        assert_eq!(
            find_tooltip_text_by_key(&owner, "child").as_deref(),
            Some("Open details")
        );
        assert_eq!(
            find_tooltip_by_key(&owner, "child"),
            Some(("owner".into(), "Open details".into()))
        );
    }
}
