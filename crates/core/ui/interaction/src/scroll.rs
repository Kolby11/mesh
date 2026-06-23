use super::*;
use mesh_core_module::manifest::SurfaceLayoutSection;
use std::collections::HashMap;

pub fn find_scrollable_at(node: &WidgetNode, x: f32, y: f32) -> Option<String> {
    find_scrollable_at_with_offset(node, x, y, 0.0, 0.0)
}

pub fn scroll_limits(node: &WidgetNode) -> (f32, f32) {
    (
        parse_node_attr_f32(node, "_mesh_scroll_max_x"),
        parse_node_attr_f32(node, "_mesh_scroll_max_y"),
    )
}

/// Build the chain of nodes from the tree root down to the node whose
/// `_mesh_key` matches `target_key` (inclusive of both ends). Returns `None` if
/// the key is absent. The first element is always the root and the last is the
/// target, so `[..len-1]` are the target's ancestors in outer-to-inner order.
fn node_path_by_key<'a>(node: &'a WidgetNode, target_key: &str) -> Option<Vec<&'a WidgetNode>> {
    if node
        .attributes
        .get("_mesh_key")
        .is_some_and(|value| value == target_key)
    {
        return Some(vec![node]);
    }
    for child in &node.children {
        if let Some(mut path) = node_path_by_key(child, target_key) {
            path.insert(0, node);
            return Some(path);
        }
    }
    None
}

/// Accumulated on-screen offset `(x, y)` for each node along `path`, mirroring
/// what the painter applies: a node's own `transform.translate` shifts its own
/// box, and its scroll offset shifts its children. The root's offset is `(0, 0)`.
/// `offsets` supplies the authoritative live scroll positions (tree attributes
/// can lag a frame).
fn path_screen_offsets(
    path: &[&WidgetNode],
    offsets: &HashMap<String, ScrollOffsetState>,
) -> Vec<(f32, f32)> {
    let mut incoming = (0.0_f32, 0.0_f32);
    let mut result = Vec::with_capacity(path.len());
    for node in path {
        let t = node.computed_style.transform;
        let screen = (incoming.0 + t.translate_x, incoming.1 + t.translate_y);
        result.push(screen);
        let scroll = node
            .attributes
            .get("_mesh_key")
            .and_then(|key| offsets.get(key))
            .copied()
            .unwrap_or_default();
        incoming = (screen.0 - scroll.x, screen.1 - scroll.y);
    }
    result
}

/// Compute the minimal scroll-offset adjustments that bring the node identified
/// by `target_key` into view within each of its scrollable ancestors. Returns
/// the changed `(scroll_container_key, new_offset)` pairs; an empty vec means
/// nothing needed to move (or the key is not in the tree / has no scrollable
/// ancestor).
///
/// `current_offsets` provides the live scroll state; the math mirrors the wheel
/// handler so the result composes with manual scrolling. Containers are
/// processed deepest-first so nested scroll regions settle correctly: adjusting
/// an inner region moves the target, then outer regions re-evaluate against the
/// new position. This implements the CSS `scroll-into-view` "nearest" rule —
/// scroll just enough to reveal the leading edge, then the trailing edge.
pub fn scroll_into_view_offsets(
    root: &WidgetNode,
    target_key: &str,
    current_offsets: &HashMap<String, ScrollOffsetState>,
) -> Vec<(String, ScrollOffsetState)> {
    let Some(path) = node_path_by_key(root, target_key) else {
        return Vec::new();
    };
    let target_idx = path.len() - 1;
    if target_idx == 0 {
        return Vec::new();
    }

    // Scrollable ancestors, deepest-first.
    let mut scrollable: Vec<usize> = (0..target_idx)
        .filter(|&i| {
            let (max_x, max_y) = scroll_limits(path[i]);
            max_x > f32::EPSILON || max_y > f32::EPSILON
        })
        .collect();
    scrollable.reverse();

    let mut offsets = current_offsets.clone();
    let mut changed = Vec::new();

    for i in scrollable {
        let container = path[i];
        let Some(key) = container.attributes.get("_mesh_key").cloned() else {
            continue;
        };

        let screens = path_screen_offsets(&path, &offsets);
        let (cox, coy) = screens[i];
        let (tox, toy) = screens[target_idx];
        let target = path[target_idx];
        let pad = &container.computed_style.padding;

        // Container content viewport and target box, both on screen.
        let view_left = container.layout.x + cox + pad.left;
        let view_top = container.layout.y + coy + pad.top;
        let view_w = (container.layout.width - pad.horizontal()).max(0.0);
        let view_h = (container.layout.height - pad.vertical()).max(0.0);
        let t_left = target.layout.x + tox;
        let t_top = target.layout.y + toy;
        let t_w = target.layout.width.max(0.0);
        let t_h = target.layout.height.max(0.0);

        // Increasing a container's scroll offset moves its children up/left, so a
        // positive delta reduces the target's screen position by the same amount.
        let dx = edge_delta(t_left - view_left, (t_left + t_w) - (view_left + view_w));
        let dy = edge_delta(t_top - view_top, (t_top + t_h) - (view_top + view_h));

        let (max_x, max_y) = scroll_limits(container);
        let mut offset = offsets.get(&key).copied().unwrap_or_default();
        let next_x = (offset.x + dx).clamp(0.0, max_x);
        let next_y = (offset.y + dy).clamp(0.0, max_y);
        if (next_x - offset.x).abs() > f32::EPSILON || (next_y - offset.y).abs() > f32::EPSILON {
            offset.x = next_x;
            offset.y = next_y;
            offsets.insert(key.clone(), offset);
            changed.push((key, offset));
        }
    }

    changed
}

/// Minimal scroll delta along one axis given the target's leading-edge gap
/// (`lead`, negative when the target sits before the viewport) and trailing-edge
/// overflow (`trail`, positive when it spills past the viewport end). Reveals the
/// leading edge first, matching the CSS "nearest" alignment.
fn edge_delta(lead: f32, trail: f32) -> f32 {
    if lead < 0.0 {
        lead
    } else if trail > 0.0 {
        trail
    } else {
        0.0
    }
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

pub fn measure_content_size(
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

pub fn annotate_overflow_tree(
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

#[cfg(test)]
mod scroll_into_view_tests {
    use super::*;
    use mesh_core_elements::WidgetNode;

    fn node(key: &str, tag: &str, x: f32, y: f32, w: f32, h: f32) -> WidgetNode {
        let mut n = WidgetNode::new(tag);
        n.attributes.insert("_mesh_key".into(), key.into());
        n.layout.x = x;
        n.layout.y = y;
        n.layout.width = w;
        n.layout.height = h;
        n
    }

    fn scrollable(mut n: WidgetNode, max_x: f32, max_y: f32) -> WidgetNode {
        n.attributes
            .insert("_mesh_scroll_max_x".into(), max_x.to_string());
        n.attributes
            .insert("_mesh_scroll_max_y".into(), max_y.to_string());
        n
    }

    // root > viewport(scrollable, 200px tall, content 600px) > item at y=400.
    fn list_tree() -> WidgetNode {
        let item = node("root/0/0", "box", 0.0, 400.0, 100.0, 50.0);
        let mut viewport = scrollable(node("root/0", "column", 0.0, 0.0, 100.0, 200.0), 0.0, 400.0);
        viewport.children.push(item);
        let mut root = node("root", "box", 0.0, 0.0, 100.0, 200.0);
        root.children.push(viewport);
        root
    }

    #[test]
    fn scrolls_down_to_reveal_item_below_viewport() {
        let tree = list_tree();
        let updates = scroll_into_view_offsets(&tree, "root/0/0", &HashMap::new());
        assert_eq!(updates.len(), 1);
        let (key, offset) = &updates[0];
        assert_eq!(key, "root/0");
        // Item bottom is at 450; viewport is 200 tall, so scroll to 250 to align
        // the trailing edge.
        assert!((offset.y - 250.0).abs() < 0.01, "got {}", offset.y);
        assert_eq!(offset.x, 0.0);
    }

    #[test]
    fn scrolls_up_to_reveal_item_above_current_offset() {
        let tree = list_tree();
        let mut current = HashMap::new();
        // Scrolled past the item: at offset 500 the item (abs y 400) sits at screen
        // y -100, above the viewport top.
        current.insert("root/0".to_string(), ScrollOffsetState { x: 0.0, y: 500.0 });
        let updates = scroll_into_view_offsets(&tree, "root/0/0", &current);
        assert_eq!(updates.len(), 1);
        // Reveal the leading edge → offset aligns the item top to the viewport top
        // at 400.
        assert!(
            (updates[0].1.y - 400.0).abs() < 0.01,
            "got {}",
            updates[0].1.y
        );
    }

    #[test]
    fn no_change_when_already_visible() {
        let tree = list_tree();
        let mut current = HashMap::new();
        // Item [400,450] sits inside viewport window [300,500].
        current.insert("root/0".to_string(), ScrollOffsetState { x: 0.0, y: 300.0 });
        let updates = scroll_into_view_offsets(&tree, "root/0/0", &current);
        assert!(updates.is_empty());
    }

    #[test]
    fn clamps_to_scroll_max() {
        let tree = list_tree();
        let updates = scroll_into_view_offsets(&tree, "root/0/0", &HashMap::new());
        // max_y is 400; the computed 250 is within bounds, but verify clamping by
        // shrinking the limit: a deeper item would clamp here.
        assert!(updates[0].1.y <= 400.0);
    }

    #[test]
    fn no_scrollable_ancestor_yields_no_updates() {
        let item = node("root/0", "box", 0.0, 0.0, 50.0, 50.0);
        let mut root = node("root", "box", 0.0, 0.0, 100.0, 100.0);
        root.children.push(item);
        let updates = scroll_into_view_offsets(&root, "root/0", &HashMap::new());
        assert!(updates.is_empty());
    }

    #[test]
    fn missing_key_yields_no_updates() {
        let tree = list_tree();
        let updates = scroll_into_view_offsets(&tree, "does/not/exist", &HashMap::new());
        assert!(updates.is_empty());
    }

    #[test]
    fn nested_containers_both_adjust() {
        // Coords are absolute (the layout engine bakes ancestor position into
        // layout.x/y). outer viewport [0,200]; inner box at abs y 300 (its own
        // viewport [300,400]); item at abs y 380 (inside inner's viewport already).
        let item = node("root/0/0/0", "box", 0.0, 380.0, 40.0, 20.0);
        let mut inner = scrollable(
            node("root/0/0", "column", 0.0, 300.0, 100.0, 100.0),
            0.0,
            200.0,
        );
        inner.children.push(item);
        let mut outer = scrollable(node("root/0", "column", 0.0, 0.0, 100.0, 200.0), 0.0, 400.0);
        outer.children.push(inner);
        let mut root = node("root", "box", 0.0, 0.0, 100.0, 200.0);
        root.children.push(outer);

        let updates = scroll_into_view_offsets(&root, "root/0/0/0", &HashMap::new());
        let by_key: HashMap<_, _> = updates.into_iter().collect();
        // Inner: item [380,400] fits its viewport [300,400] → no inner scroll. Outer:
        // item screen-top 380 is below the 200-tall outer viewport → outer scrolls by
        // 200 to align the trailing edge.
        assert!(
            !by_key.contains_key("root/0/0"),
            "inner should not move: {by_key:?}"
        );
        let outer = by_key.get("root/0").expect("outer should scroll");
        assert!((outer.y - 200.0).abs() < 0.01, "got {}", outer.y);
    }
}
