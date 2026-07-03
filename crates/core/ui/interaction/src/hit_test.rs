use super::*;
#[derive(Debug, Clone, PartialEq)]
pub struct PointerHit {
    pub path: Vec<String>,
    pub tooltip: Option<(String, String)>,
    pub bounds: ContentBounds,
}

type TooltipHit = (String, String, ContentBounds);

/// Resolve all pointer-motion metadata in the same tree traversal.
pub fn pointer_hit_test(node: &WidgetNode, x: f32, y: f32) -> Option<PointerHit> {
    let mut hit = pointer_hit_test_reversed(node, x, y, 0.0, 0.0, None)?;
    hit.path.reverse();
    Some(hit)
}

fn pointer_hit_test_reversed(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
    inherited_tooltip: Option<&TooltipHit>,
) -> Option<PointerHit> {
    if node_is_hidden(node) {
        return None;
    }
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside && node_clips_children(node) {
        return None;
    }
    let owner_tooltip = node_tooltip_owner_text(node)
        .map(|(owner, text)| (owner, text, node_rect_with_offset(node, offset_x, offset_y)));
    let tooltip = owner_tooltip.as_ref().or(inherited_tooltip);
    let (child_ox, child_oy) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in node.children.iter().rev() {
        if let Some(mut hit) = pointer_hit_test_reversed(child, x, y, child_ox, child_oy, tooltip) {
            if let Some(key) = node.attributes.get("_mesh_key") {
                hit.path.push(key.clone());
            }
            return Some(hit);
        }
    }
    let key = node.attributes.get("_mesh_key")?;
    inside.then(|| PointerHit {
        path: vec![key.clone()],
        tooltip: tooltip.map(|(owner, text, _)| (owner.clone(), text.clone())),
        bounds: tooltip
            .map(|(_, _, bounds)| *bounds)
            .unwrap_or_else(|| node_rect_with_offset(node, offset_x, offset_y)),
    })
}

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

/// Resolves node references and bounds for a set of `_mesh_key`s in one
/// traversal, instead of a separate `find_node_by_key` + `find_node_bounds_by_key`
/// walk per key. Used by hover-transition dispatch, where a path-depth number
/// of keys would otherwise each re-walk the whole tree.
pub fn find_nodes_by_keys<'a>(
    node: &'a WidgetNode,
    keys: &std::collections::HashSet<&str>,
) -> std::collections::HashMap<String, (&'a WidgetNode, ContentBounds)> {
    let mut found = std::collections::HashMap::with_capacity(keys.len());
    collect_nodes_by_keys(node, keys, 0.0, 0.0, &mut found);
    found
}

fn collect_nodes_by_keys<'a>(
    node: &'a WidgetNode,
    keys: &std::collections::HashSet<&str>,
    offset_x: f32,
    offset_y: f32,
    found: &mut std::collections::HashMap<String, (&'a WidgetNode, ContentBounds)>,
) {
    if found.len() == keys.len() {
        return;
    }
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    if let Some(key) = node.attributes.get("_mesh_key")
        && keys.contains(key.as_str())
    {
        found.insert(
            key.clone(),
            (node, node_rect_with_offset(node, offset_x, offset_y)),
        );
    }
    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in &node.children {
        collect_nodes_by_keys(child, keys, child_offset_x, child_offset_y, found);
        if found.len() == keys.len() {
            break;
        }
    }
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
    if node_is_hidden(node) {
        return None;
    }

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
    if node
        .attributes
        .get("data-tooltip-disabled")
        .is_some_and(|value| value == "true" || value == "1")
    {
        return None;
    }

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
    use mesh_core_elements::{LayoutRect, WidgetNode};

    fn indexed_tree(rows: usize, columns: usize) -> WidgetNode {
        let mut root = WidgetNode::new("surface");
        root.attributes.insert("_mesh_key".into(), "root".into());
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: columns as f32 * 20.0,
            height: rows as f32 * 20.0,
        };
        for row_index in 0..rows {
            let mut row = WidgetNode::new("row");
            row.attributes
                .insert("_mesh_key".into(), format!("row-{row_index}"));
            row.layout = LayoutRect {
                x: 0.0,
                y: row_index as f32 * 20.0,
                width: columns as f32 * 20.0,
                height: 20.0,
            };
            for column_index in 0..columns {
                let mut cell = WidgetNode::new("button");
                cell.attributes.insert(
                    "_mesh_key".into(),
                    format!("cell-{row_index}-{column_index}"),
                );
                cell.layout = LayoutRect {
                    x: column_index as f32 * 20.0,
                    y: 0.0,
                    width: 20.0,
                    height: 20.0,
                };
                row.children.push(cell);
            }
            root.children.push(row);
        }
        root
    }

    #[test]
    fn pointer_hit_test_matches_separate_tree_walks() {
        let mut root = indexed_tree(6, 8);
        root.children[2]
            .attributes
            .insert("tooltip".into(), "Third row".into());
        root.children[2]
            .attributes
            .insert("_mesh_scroll_x".into(), "5".into());
        root.children[4].computed_style.transform.translate_x = 7.0;
        for y in (0..120).step_by(3) {
            for x in (0..160).step_by(3) {
                let fused = pointer_hit_test(&root, x as f32, y as f32);
                assert_eq!(
                    fused.as_ref().map(|hit| hit.path.clone()),
                    find_node_path_at(&root, x as f32, y as f32),
                    "mismatch at ({x}, {y})"
                );
            }
        }
        let hit = pointer_hit_test(&root, 48.0, 50.0).unwrap();
        assert_eq!(
            hit.bounds,
            find_node_bounds_by_key(&root, "row-2", 0.0, 0.0).unwrap()
        );
        assert_eq!(
            hit.tooltip,
            find_tooltip_by_key(&root, hit.path.last().unwrap())
        );
    }

    #[test]
    fn find_nodes_by_keys_matches_separate_lookups() {
        let root = indexed_tree(6, 8);
        let keys: std::collections::HashSet<&str> =
            ["row-2", "cell-4-3", "missing-key"].into_iter().collect();

        let found = find_nodes_by_keys(&root, &keys);

        assert_eq!(found.len(), 2, "the missing key must not appear");
        let (row_node, row_bounds) = found.get("row-2").expect("row-2 found");
        assert_eq!(row_node.attributes.get("_mesh_key").unwrap(), "row-2");
        assert_eq!(
            row_bounds,
            &find_node_bounds_by_key(&root, "row-2", 0.0, 0.0).unwrap()
        );

        let (cell_node, cell_bounds) = found.get("cell-4-3").expect("cell-4-3 found");
        assert_eq!(cell_node.attributes.get("_mesh_key").unwrap(), "cell-4-3");
        assert_eq!(
            cell_bounds,
            &find_node_bounds_by_key(&root, "cell-4-3", 0.0, 0.0).unwrap()
        );
    }

    #[test]
    fn find_nodes_by_keys_respects_transform_offsets() {
        let mut root = indexed_tree(3, 3);
        root.children[1].computed_style.transform.translate_x = 7.0;
        let keys: std::collections::HashSet<&str> = ["cell-1-1"].into_iter().collect();

        let found = find_nodes_by_keys(&root, &keys);

        let (_, bounds) = found.get("cell-1-1").expect("cell-1-1 found");
        assert_eq!(
            bounds,
            &find_node_bounds_by_key(&root, "cell-1-1", 0.0, 0.0).unwrap()
        );
    }

    // cargo test -p mesh-core-interaction --release -- find_nodes_by_keys_beats_per_key_lookups --ignored --nocapture
    #[test]
    #[ignore = "release-only per-key-lookup microbenchmark"]
    fn find_nodes_by_keys_beats_per_key_lookups() {
        use std::hint::black_box;
        use std::time::Instant;

        let root = indexed_tree(200, 20);
        let keys = ["row-198", "cell-199-19", "cell-0-0", "row-100"];
        let key_set: std::collections::HashSet<&str> = keys.into_iter().collect();
        let iterations = 20_000;

        let per_key_started = Instant::now();
        for _ in 0..iterations {
            for key in keys {
                black_box(find_node_by_key(black_box(&root), key));
                black_box(find_node_bounds_by_key(black_box(&root), key, 0.0, 0.0));
            }
        }
        let per_key = per_key_started.elapsed();

        let fused_started = Instant::now();
        for _ in 0..iterations {
            black_box(find_nodes_by_keys(black_box(&root), &key_set));
        }
        let fused = fused_started.elapsed();

        eprintln!(
            "per-key find_node_by_key+bounds: {per_key:?}; fused find_nodes_by_keys: {fused:?}; ratio: {:.1}x",
            per_key.as_secs_f64() / fused.as_secs_f64()
        );
        assert!(fused < per_key);
    }

    #[test]
    #[ignore = "release-only pointer-motion microbenchmark"]
    fn fused_pointer_motion_beats_repeated_tree_walks() {
        use std::hint::black_box;
        use std::time::Instant;

        let tree = indexed_tree(100, 10);
        let iterations = 20_000;
        let started = Instant::now();
        for i in 0..iterations {
            let x = ((i * 17) % 200) as f32;
            let y = ((i * 31) % 2_000) as f32;
            let path = find_node_path_at(&tree, x, y).unwrap_or_default();
            let key = path.last().map(String::as_str).unwrap_or_default();
            black_box(find_tooltip_by_key(&tree, key));
            black_box(find_tooltip_by_key(&tree, key));
            black_box(find_node_bounds_by_key(&tree, key, 0.0, 0.0));
        }
        let tree_walk = started.elapsed();

        let started = Instant::now();
        for i in 0..iterations {
            let x = ((i * 17) % 200) as f32;
            let y = ((i * 31) % 2_000) as f32;
            black_box(pointer_hit_test(&tree, x, y));
        }
        let fused = started.elapsed();

        eprintln!("tree_walk={tree_walk:?} fused={fused:?}");
        assert!(fused < tree_walk, "fused lookup must improve pointer time");
    }

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

    #[test]
    fn tooltip_disabled_attribute_suppresses_title_and_accessible_label() {
        let mut node = WidgetNode::new("button");
        node.attributes.insert("_mesh_key".into(), "button".into());
        node.attributes.insert("title".into(), "Open".into());
        node.attributes.insert("aria-label".into(), "Open".into());
        node.attributes
            .insert("data-tooltip-disabled".into(), "true".into());

        assert_eq!(node_tooltip_text(&node), None);
        assert_eq!(find_tooltip_text_by_key(&node, "button"), None);
    }

    #[test]
    fn hidden_portal_placeholder_does_not_block_previous_sibling_hit_target() {
        let mut root = WidgetNode::new("stack");
        root.attributes.insert("_mesh_key".into(), "root".into());
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 80.0,
        };

        let mut button = WidgetNode::new("button");
        button
            .attributes
            .insert("_mesh_key".into(), "button".into());
        button
            .event_handlers
            .insert("click".into(), "onClick".into());
        button.layout = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 40.0,
        };

        let mut placeholder = WidgetNode::new("box");
        placeholder
            .attributes
            .insert("_mesh_key".into(), "portal".into());
        placeholder
            .attributes
            .insert("hidden".into(), "true".into());
        placeholder.layout = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 40.0,
        };

        root.children.push(button);
        root.children.push(placeholder);

        assert_eq!(
            find_node_path_at(&root, 30.0, 30.0),
            Some(vec!["root".into(), "button".into()])
        );
    }
}
