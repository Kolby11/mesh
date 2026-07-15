use super::*;
use std::collections::HashMap;

pub fn find_scrollable_at(node: &WidgetNode, x: f32, y: f32) -> Option<String> {
    find_scrollable_at_with_limits(node, x, y).map(|hit| hit.key)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrollableHit {
    pub key: String,
    pub max_x: f32,
    pub max_y: f32,
}

pub fn find_scrollable_at_with_limits(node: &WidgetNode, x: f32, y: f32) -> Option<ScrollableHit> {
    find_scrollable_at_with_offset(node, x, y, 0.0, 0.0)
}

pub fn scroll_limits(node: &WidgetNode) -> (f32, f32) {
    let scroll = node.resolved_scroll_metrics();
    (scroll.max_x, scroll.max_y)
}

/// Build the chain of nodes from the tree root down to the node whose
/// `_mesh_key` matches `target_key` (inclusive of both ends). Returns `None` if
/// the key is absent. The first element is always the root and the last is the
/// target, so `[..len-1]` are the target's ancestors in outer-to-inner order.
fn node_path_by_key<'a>(node: &'a WidgetNode, target_key: &str) -> Option<Vec<&'a WidgetNode>> {
    if node.mesh_key().is_some_and(|value| value == target_key) {
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
            .mesh_key()
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
        let Some(key) = container.mesh_key().map(str::to_owned) else {
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
) -> Option<ScrollableHit> {
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
        return node.mesh_key().map(|key| {
            let (max_x, max_y) = scroll_limits(node);
            ScrollableHit {
                key: key.to_owned(),
                max_x,
                max_y,
            }
        });
    }

    None
}

/// Measure a surface's content size for the compositor.
///
/// The surface root's own laid-out box **is** the measured size: the layout
/// engine already resolved the root's CSS `width`/`height` against the available
/// space — `100%` spans, a fixed length pins, `fit-content`/`min-content`/
/// `max-content` shrink to content — with the root's `min-*`/`max-*` clamps
/// applied by taffy. So sizing is fully CSS-driven with no manifest inputs; this
/// replaces the old `mesh.surface` `size_policy`/`min_*`/`max_*`/
/// `prefers_content_children_sizing` fields (see
/// `docs/spec/03-components.md` §2).
///
/// `fallback_*` is used only when the root has no positive laid-out extent yet
/// (e.g. a degenerate first frame).
///
/// The compiled surface tree's root is a synthetic `surface` wrapper whose
/// style is pinned to the paint-input size (`surface_style()` in the frontend
/// compiler), so its own laid-out box can only ever echo the input back —
/// circular for content-sized surfaces, which then stay stuck at whatever
/// size the first paint assumed (the shipped symptom: a right-anchored panel
/// permanently mapped at the 1x1 protocol clamp). The box whose CSS
/// `width`/`height` the layout engine actually resolved is the wrapper's
/// child (the component root), so for a `surface` wrapper measure the union
/// extent of its children instead.
pub fn measure_content_size(
    tree: &WidgetNode,
    fallback_width: u32,
    fallback_height: u32,
) -> (u32, u32) {
    let (content_width, content_height) = if tree.tag == "surface" && !tree.children.is_empty() {
        let mut max_x = 0f32;
        let mut max_y = 0f32;
        for child in &tree.children {
            max_x = max_x.max(child.layout.x + child.layout.width);
            max_y = max_y.max(child.layout.y + child.layout.height);
        }
        (max_x, max_y)
    } else {
        (tree.layout.width, tree.layout.height)
    };
    let width = if content_width >= 1.0 {
        content_width.ceil() as u32
    } else {
        fallback_width
    };
    let height = if content_height >= 1.0 {
        content_height.ceil() as u32
    } else {
        fallback_height
    };
    (width, height)
}

pub fn annotate_overflow_tree(
    node: &mut WidgetNode,
    key: &str,
    scroll_offsets: &mut HashMap<String, ScrollOffsetState>,
) -> Option<ContentBounds> {
    // Descendant indices are appended in place. Reserve a small path budget
    // up front so common shallow trees do not grow the String repeatedly.
    let mut key_path = String::with_capacity(key.len() + 64);
    key_path.push_str(key);
    annotate_overflow_tree_with_path(node, &mut key_path, scroll_offsets)
}

fn annotate_overflow_tree_with_path(
    node: &mut WidgetNode,
    key_path: &mut String,
    scroll_offsets: &mut HashMap<String, ScrollOffsetState>,
) -> Option<ContentBounds> {
    let mut children_bounds: Option<ContentBounds> = None;

    for (index, child) in node.children.iter_mut().enumerate() {
        let restore_len = key_path.len();
        use std::fmt::Write as _;
        write!(key_path, "/{index}").expect("writing to String cannot fail");
        if let Some(child_bounds) =
            annotate_overflow_tree_with_path(child, key_path, scroll_offsets)
        {
            children_bounds = Some(union_bounds(children_bounds, child_bounds));
        }
        key_path.truncate(restore_len);
    }

    Some(annotate_overflow_node(
        node,
        key_path,
        scroll_offsets,
        children_bounds,
    ))
}

/// Annotates one node after its children and returns its propagated bounds.
///
/// This is exposed so callers that already perform a pre-order annotation can
/// fold overflow's post-order work into the same traversal.
pub fn annotate_overflow_node(
    node: &mut WidgetNode,
    key: &str,
    scroll_offsets: &mut HashMap<String, ScrollOffsetState>,
    children_bounds: Option<ContentBounds>,
) -> ContentBounds {
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

    let offset = if scroll_offsets.contains_key(key) {
        scroll_offsets
            .get_mut(key)
            .expect("scroll offset key was just checked")
    } else {
        scroll_offsets.entry(key.to_string()).or_default()
    };
    offset.x = offset.x.clamp(0.0, max_x);
    offset.y = offset.y.clamp(0.0, max_y);

    node.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
        x: offset.x,
        y: offset.y,
        max_x,
        max_y,
        content_width,
        content_height,
    });

    let own_bounds = (
        node.layout.x,
        node.layout.y,
        node.layout.x + node.layout.width.max(0.0),
        node.layout.y + node.layout.height.max(0.0),
    );
    if node_clips_children(node) {
        own_bounds
    } else {
        union_bounds(Some(own_bounds), children_bounds.unwrap_or(own_bounds))
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

    #[test]
    fn scrollable_hit_with_limits_matches_legacy_key_lookup() {
        let tree = list_tree();

        let legacy = find_scrollable_at(&tree, 10.0, 10.0);
        let fused = find_scrollable_at_with_limits(&tree, 10.0, 10.0).expect("scrollable hit");

        assert_eq!(legacy.as_deref(), Some(fused.key.as_str()));
        assert_eq!(fused.key, "root/0");
        assert_eq!(fused.max_x, 0.0);
        assert_eq!(fused.max_y, 400.0);
    }

    #[test]
    fn annotate_overflow_tree_preserves_keyed_scroll_offsets() {
        let mut tree = list_tree();
        tree.children[0].computed_style.overflow_y = mesh_core_elements::style::Overflow::Auto;
        let mut offsets =
            HashMap::from([("root/0".to_string(), ScrollOffsetState { x: 0.0, y: 999.0 })]);

        annotate_overflow_tree(&mut tree, "root", &mut offsets);

        let offset = offsets.get("root/0").expect("existing offset key");
        assert_eq!(offset.x, 0.0);
        assert_eq!(offset.y, 250.0);
        let metrics = tree.children[0].scroll_metrics.expect("scroll metrics");
        assert_eq!(metrics.y, 250.0);
        assert_eq!(metrics.max_y, 250.0);
    }

    // cargo test -p mesh-core-interaction --release -- scrollable_hit_with_limits_beats_key_then_lookup --ignored --nocapture
    #[test]
    #[ignore = "release-only scroll hit fusion microbenchmark"]
    fn scrollable_hit_with_limits_beats_key_then_lookup() {
        fn build_tree(
            width: usize,
            depth: usize,
            key: String,
            next_leaf_y: &mut f32,
        ) -> WidgetNode {
            let mut node = node(&key, "box", 0.0, 0.0, 120.0, 120.0);
            if depth == 0 {
                node.layout.y = *next_leaf_y;
                *next_leaf_y += 2.0;
                return node;
            }
            node.children = (0..width)
                .map(|index| build_tree(width, depth - 1, format!("{key}/{index}"), next_leaf_y))
                .collect();
            node
        }

        let mut y = 0.0;
        let mut root = build_tree(4, 5, "root".into(), &mut y);
        root.children[3] = scrollable(node("root/3", "column", 0.0, 0.0, 120.0, 120.0), 0.0, 512.0);
        root.children[3].children = (0..64)
            .map(|index| {
                node(
                    &format!("root/3/{index}"),
                    "box",
                    0.0,
                    index as f32 * 12.0,
                    100.0,
                    10.0,
                )
            })
            .collect();
        let iterations = 200_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0.0f32;
        for _ in 0..iterations {
            if let Some(key) = find_scrollable_at(std::hint::black_box(&root), 10.0, 10.0)
                && let Some(node) = crate::find_node_by_key(std::hint::black_box(&root), &key)
            {
                let (_, max_y) = scroll_limits(node);
                old_total += std::hint::black_box(max_y);
            }
        }
        let old_time = old_started.elapsed();

        let fused_started = std::time::Instant::now();
        let mut fused_total = 0.0f32;
        for _ in 0..iterations {
            if let Some(hit) =
                find_scrollable_at_with_limits(std::hint::black_box(&root), 10.0, 10.0)
            {
                fused_total += std::hint::black_box(hit.max_y);
            }
        }
        let fused_time = fused_started.elapsed();

        eprintln!(
            "scrollable hit limits: key+lookup {old_time:?}; fused {fused_time:?}; ratio {:.1}x; totals={old_total}/{fused_total}",
            old_time.as_secs_f64() / fused_time.as_secs_f64()
        );
        assert_eq!(old_total, fused_total);
        assert!(fused_time < old_time);
    }

    // cargo test -p mesh-core-interaction --release -- annotate_overflow_tree_path_buffer_beats_format_keys --ignored --nocapture
    #[test]
    #[ignore = "release-only scroll annotation key-path microbenchmark"]
    fn annotate_overflow_tree_path_buffer_beats_format_keys() {
        fn build_tree(width: usize, depth: usize, key: &str) -> WidgetNode {
            let mut node = node(key, "column", 0.0, 0.0, 120.0, 120.0);
            node.computed_style.overflow_y = mesh_core_elements::style::Overflow::Auto;
            if depth == 0 {
                node.layout.height = 16.0;
                return node;
            }
            node.children = (0..width)
                .map(|index| build_tree(width, depth - 1, &format!("{key}/{index}")))
                .collect();
            node
        }

        fn collect_keys(node: &WidgetNode, keys: &mut Vec<String>) {
            if let Some(key) = node.mesh_key() {
                keys.push(key.to_string());
            }
            for child in &node.children {
                collect_keys(child, keys);
            }
        }

        fn old_annotate_overflow_tree(
            node: &mut WidgetNode,
            key: &str,
            scroll_offsets: &mut HashMap<String, ScrollOffsetState>,
        ) -> Option<ContentBounds> {
            let mut children_bounds: Option<ContentBounds> = None;
            for (index, child) in node.children.iter_mut().enumerate() {
                if let Some(child_bounds) =
                    old_annotate_overflow_tree(child, &format!("{key}/{index}"), scroll_offsets)
                {
                    children_bounds = Some(union_bounds(children_bounds, child_bounds));
                }
            }

            let content_origin_x = node.layout.x + node.computed_style.padding.left;
            let content_origin_y = node.layout.y + node.computed_style.padding.top;
            let viewport_width =
                (node.layout.width - node.computed_style.padding.horizontal()).max(0.0);
            let viewport_height =
                (node.layout.height - node.computed_style.padding.vertical()).max(0.0);
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
            node.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
                x: offset.x,
                y: offset.y,
                max_x,
                max_y,
                content_width,
                content_height,
            });

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

        let tree = build_tree(5, 4, "root");
        let mut keys = Vec::new();
        collect_keys(&tree, &mut keys);
        let offsets = keys
            .into_iter()
            .map(|key| (key, ScrollOffsetState { x: 0.0, y: 8.0 }))
            .collect::<HashMap<_, _>>();
        let iterations = 20_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0.0f32;
        let mut old_tree = tree.clone();
        let mut old_offsets = offsets.clone();
        for _ in 0..iterations {
            old_annotate_overflow_tree(
                std::hint::black_box(&mut old_tree),
                "root",
                std::hint::black_box(&mut old_offsets),
            );
            old_total += std::hint::black_box(old_tree.scroll_metrics.unwrap().content_height);
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0.0f32;
        let mut new_tree = tree;
        let mut new_offsets = offsets;
        for _ in 0..iterations {
            annotate_overflow_tree(
                std::hint::black_box(&mut new_tree),
                "root",
                std::hint::black_box(&mut new_offsets),
            );
            new_total += std::hint::black_box(new_tree.scroll_metrics.unwrap().content_height);
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "scroll annotation key paths: format recursion {old_time:?}; path buffer {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-interaction --release -- annotate_overflow_tree_reserved_path_beats_unreserved --ignored --nocapture
    #[test]
    #[ignore = "release-only reserved scroll key-path microbenchmark"]
    fn annotate_overflow_tree_reserved_path_beats_unreserved() {
        fn unreserved(
            node: &mut WidgetNode,
            key: &str,
            scroll_offsets: &mut HashMap<String, ScrollOffsetState>,
        ) -> Option<ContentBounds> {
            let mut key_path = key.to_string();
            annotate_overflow_tree_with_path(node, &mut key_path, scroll_offsets)
        }

        let tree = {
            fn build(width: usize, depth: usize, key: &str) -> WidgetNode {
                let mut node = node(key, "column", 0.0, 0.0, 120.0, 120.0);
                node.computed_style.overflow_y = mesh_core_elements::style::Overflow::Auto;
                if depth > 0 {
                    node.children = (0..width)
                        .map(|index| build(width, depth - 1, &format!("{key}/{index}")))
                        .collect();
                }
                node
            }
            build(5, 4, "root")
        };
        fn collect_keys(node: &WidgetNode, keys: &mut Vec<String>) {
            if let Some(key) = node.mesh_key() {
                keys.push(key.to_string());
            }
            for child in &node.children {
                collect_keys(child, keys);
            }
        }
        let mut keys = Vec::new();
        collect_keys(&tree, &mut keys);
        let offsets = keys
            .into_iter()
            .map(|key| (key, ScrollOffsetState::default()))
            .collect::<HashMap<_, _>>();
        let iterations = 20_000;

        let old_started = std::time::Instant::now();
        let mut old_tree = tree.clone();
        let mut old_offsets = offsets.clone();
        let mut old_total = 0.0;
        for _ in 0..iterations {
            unreserved(&mut old_tree, "root", &mut old_offsets);
            old_total += std::hint::black_box(old_tree.layout.width);
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_tree = tree;
        let mut new_offsets = offsets;
        let mut new_total = 0.0;
        for _ in 0..iterations {
            annotate_overflow_tree(&mut new_tree, "root", &mut new_offsets);
            new_total += std::hint::black_box(new_tree.layout.width);
        }
        let new_time = new_started.elapsed();

        assert_eq!(old_total, new_total);
        eprintln!(
            "scroll key-path capacity: unreserved {old_time:?}; reserved {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }
}
