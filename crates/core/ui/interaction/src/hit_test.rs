use super::*;
#[derive(Debug, Clone, PartialEq)]
pub struct PointerHit {
    pub path: Vec<String>,
    pub tooltip: Option<(String, String)>,
    pub bounds: ContentBounds,
}

#[derive(Debug, Clone, Copy)]
pub struct PointerPressNode<'a> {
    pub key: &'a str,
    pub node: &'a WidgetNode,
    pub bounds: ContentBounds,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PointerPressHit<'a> {
    pub target: Option<PointerPressNode<'a>>,
    pub focusable: Option<PointerPressNode<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub struct PointerEventHandlerHit<'a> {
    pub key: &'a str,
    pub node: &'a WidgetNode,
    pub bounds: ContentBounds,
}

/// Deepest visible layout node under a point. Unlike the normal pointer hit,
/// this intentionally includes non-interactive and synthetic nodes so the
/// debug element picker can inspect everything that was painted.
#[derive(Debug, Clone, Copy)]
pub struct InspectHit<'a> {
    pub node: &'a WidgetNode,
    pub bounds: ContentBounds,
}

pub fn inspect_hit_test(node: &WidgetNode, x: f32, y: f32) -> Option<InspectHit<'_>> {
    inspect_hit_test_inner(node, x, y, 0.0, 0.0)
}

fn inspect_hit_test_inner(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<InspectHit<'_>> {
    if node_is_hidden(node) {
        return None;
    }
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside && node_clips_children(node) {
        return None;
    }
    let (child_x, child_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in node.children.iter().rev() {
        if let Some(hit) = inspect_hit_test_inner(child, x, y, child_x, child_y) {
            return Some(hit);
        }
    }
    inside.then(|| InspectHit {
        node,
        bounds: node_rect_with_offset(node, offset_x, offset_y),
    })
}

type TooltipHit = (String, String, ContentBounds);

/// Resolve all pointer-motion metadata in the same tree traversal.
pub fn pointer_hit_test(node: &WidgetNode, x: f32, y: f32) -> Option<PointerHit> {
    let mut hit = pointer_hit_test_reversed(node, x, y, 0.0, 0.0, None)?;
    hit.path.reverse();
    Some(hit)
}

/// Resolve pointer-press targeting in one traversal.
///
/// The shell press path needs the deepest pointer-focusable node, and if none
/// exists, the deepest ancestor under the point with a click handler. The older
/// path computed these with separate full-tree walks (`find_focusable_at`,
/// `find_node_path_at`, and per-key handler lookups), then walked again for
/// target bounds. This returns the same target decision plus bounds directly
/// from the hit traversal.
pub fn pointer_press_hit(node: &WidgetNode, x: f32, y: f32) -> PointerPressHit<'_> {
    let mut hit = pointer_press_hit_inner(node, x, y, 0.0, 0.0).unwrap_or_default();
    hit.target = hit.focusable.or(hit.target);
    hit
}

/// Resolve the deepest node under the pointer that owns a plain event handler.
///
/// This preserves the legacy `find_node_path_at(...).rev().find(find_event_handler)`
/// behavior while avoiding the path allocation plus per-key tree walks.
pub fn pointer_event_handler_hit<'a>(
    node: &'a WidgetNode,
    x: f32,
    y: f32,
    event_name: &str,
) -> Option<PointerEventHandlerHit<'a>> {
    pointer_event_handler_hit_inner(node, x, y, event_name, 0.0, 0.0)
}

fn pointer_event_handler_hit_inner<'a>(
    node: &'a WidgetNode,
    x: f32,
    y: f32,
    event_name: &str,
    offset_x: f32,
    offset_y: f32,
) -> Option<PointerEventHandlerHit<'a>> {
    if node_is_hidden(node) {
        return None;
    }

    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside && node_clips_children(node) {
        return None;
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in node.children.iter().rev() {
        if let Some(hit) =
            pointer_event_handler_hit_inner(child, x, y, event_name, child_offset_x, child_offset_y)
        {
            return Some(hit);
        }
    }

    if inside && node.event_handlers.contains_key(event_name) {
        return node.mesh_key().map(|key| PointerEventHandlerHit {
            key,
            node,
            bounds: node_rect_with_offset(node, offset_x, offset_y),
        });
    }

    None
}

fn pointer_press_hit_inner<'a>(
    node: &'a WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<PointerPressHit<'a>> {
    if node_is_hidden(node) {
        return None;
    }

    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside && node_clips_children(node) {
        return None;
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    let mut hit = PointerPressHit::default();
    for child in node.children.iter().rev() {
        if let Some(child_hit) =
            pointer_press_hit_inner(child, x, y, child_offset_x, child_offset_y)
        {
            hit.focusable = child_hit.focusable;
            hit.target = child_hit.target;
            break;
        }
    }

    if inside && let Some(key) = node.mesh_key() {
        let node_hit = PointerPressNode {
            key,
            node,
            bounds: node_rect_with_offset(node, offset_x, offset_y),
        };
        if hit.focusable.is_none() && crate::focus::node_is_pointer_focusable(node) {
            hit.focusable = Some(node_hit);
        }
        if hit.target.is_none() && node.event_handlers.contains_key("click") {
            hit.target = Some(node_hit);
        }
    }

    (hit.focusable.is_some() || hit.target.is_some()).then_some(hit)
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
            if let Some(key) = node.mesh_key() {
                hit.path.push(key.to_owned());
            }
            return Some(hit);
        }
    }
    let key = node.mesh_key()?;
    inside.then(|| PointerHit {
        path: vec![key.to_owned()],
        tooltip: tooltip.map(|(owner, text, _)| (owner.clone(), text.clone())),
        bounds: tooltip
            .map(|(_, _, bounds)| *bounds)
            .unwrap_or_else(|| node_rect_with_offset(node, offset_x, offset_y)),
    })
}

pub fn find_node_by_key<'a>(node: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
    if node.mesh_key().is_some_and(|value| value == key) {
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
    if let Some(key) = node.mesh_key()
        && keys.contains(key)
    {
        found.insert(
            key.to_owned(),
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
    if node.mesh_key().is_some_and(|value| value == key) {
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

/// Resolves a keyed node and its content bounds in one traversal. This is the
/// allocation-free counterpart to [`find_nodes_by_keys`] for callers that need
/// exactly one active node, such as slider drags.
pub fn find_node_with_bounds_by_key<'a>(
    node: &'a WidgetNode,
    key: &str,
) -> Option<(&'a WidgetNode, ContentBounds)> {
    find_node_with_bounds_by_key_at(node, key, 0.0, 0.0)
}

fn find_node_with_bounds_by_key_at<'a>(
    node: &'a WidgetNode,
    key: &str,
    offset_x: f32,
    offset_y: f32,
) -> Option<(&'a WidgetNode, ContentBounds)> {
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    if node.mesh_key().is_some_and(|value| value == key) {
        return Some((node, node_rect_with_offset(node, offset_x, offset_y)));
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in &node.children {
        if let Some(found) =
            find_node_with_bounds_by_key_at(child, key, child_offset_x, child_offset_y)
        {
            return Some(found);
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
            if let Some(key) = node.mesh_key() {
                path.push(key.to_owned());
            }
            return Some(path);
        }
    }

    if inside {
        return node.mesh_key().map(|key| vec![key.to_owned()]);
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

/// Bounds of the innermost ancestor of the keyed node whose overflow clips
/// contents, in surface coordinates (left, top, right, bottom).
///
/// Tooltip auto-placement uses this as the box the tooltip should stay inside:
/// a clipping container is the visual region the user perceives the element to
/// live in, so a tooltip escaping it reads as overflow. Returns `None` when the
/// key is absent or no ancestor clips — the caller then constrains against the
/// whole paint surface instead. The node itself is never its own container.
pub fn find_tooltip_container_bounds(node: &WidgetNode, key: &str) -> Option<ContentBounds> {
    find_container_bounds_inner(node, key, 0.0, 0.0, None).flatten()
}

/// Outer `Option` = keyed node found; inner = nearest clipping ancestor bounds.
fn find_container_bounds_inner(
    node: &WidgetNode,
    key: &str,
    offset_x: f32,
    offset_y: f32,
    nearest_clip: Option<ContentBounds>,
) -> Option<Option<ContentBounds>> {
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    if node.mesh_key().is_some_and(|k| k == key) {
        return Some(nearest_clip);
    }
    let clips = node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents();
    let nearest_clip = if clips {
        Some(node_rect_with_offset(node, offset_x, offset_y))
    } else {
        nearest_clip
    };
    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in &node.children {
        if let Some(found) =
            find_container_bounds_inner(child, key, child_offset_x, child_offset_y, nearest_clip)
        {
            return Some(found);
        }
    }
    None
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
    if node.mesh_key().is_some_and(|k| k == key) {
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
            .mesh_key()
            .map(str::to_owned)
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
    let mut namespace_prefix = None;
    namespace_event_handlers_with_prefix(node, instance_key, &mut namespace_prefix);
}

fn namespace_event_handlers_with_prefix(
    node: &mut WidgetNode,
    instance_key: &str,
    namespace_prefix: &mut Option<String>,
) {
    for handler in node.event_handlers.values_mut() {
        if !handler.starts_with("__mesh_embed__::") {
            namespace_handler(handler, instance_key, namespace_prefix);
        }
    }
    for call in node.event_handler_calls.values_mut() {
        if !call.handler.starts_with("__mesh_embed__::") {
            namespace_handler(&mut call.handler, instance_key, namespace_prefix);
        }
    }

    for child in &mut node.children {
        namespace_event_handlers_with_prefix(child, instance_key, namespace_prefix);
    }
}

fn namespace_handler(
    handler: &mut String,
    instance_key: &str,
    namespace_prefix: &mut Option<String>,
) {
    let prefix = namespace_prefix.get_or_insert_with(|| {
        let mut prefix = String::with_capacity("__mesh_embed__::".len() + instance_key.len() + 2);
        prefix.push_str("__mesh_embed__::");
        prefix.push_str(instance_key);
        prefix.push_str("::");
        prefix
    });
    let mut namespaced = String::with_capacity(prefix.len() + handler.len());
    namespaced.push_str(prefix);
    namespaced.push_str(handler);
    *handler = namespaced;
}

pub fn parse_namespaced_handler(handler: &str) -> Option<(&str, &str)> {
    let rest = handler.strip_prefix("__mesh_embed__::")?;
    rest.rsplit_once("::")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::{EventHandlerCall, LayoutRect, WidgetNode};

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

    fn legacy_namespace_event_handlers(node: &mut WidgetNode, instance_key: &str) {
        for handler in node.event_handlers.values_mut() {
            if !handler.starts_with("__mesh_embed__::") {
                *handler = format!("__mesh_embed__::{instance_key}::{handler}");
            }
        }
        for call in node.event_handler_calls.values_mut() {
            if !call.handler.starts_with("__mesh_embed__::") {
                call.handler = format!("__mesh_embed__::{instance_key}::{}", call.handler);
            }
        }
        for child in &mut node.children {
            legacy_namespace_event_handlers(child, instance_key);
        }
    }

    fn assert_handler_graph_eq(left: &WidgetNode, right: &WidgetNode) {
        assert_eq!(left.event_handlers, right.event_handlers);
        assert_eq!(left.event_handler_calls, right.event_handler_calls);
        assert_eq!(left.children.len(), right.children.len());
        for (left_child, right_child) in left.children.iter().zip(&right.children) {
            assert_handler_graph_eq(left_child, right_child);
        }
    }

    fn representative_handler_tree(rows: usize, columns: usize) -> WidgetNode {
        let mut tree = indexed_tree(rows, columns);
        for row in &mut tree.children {
            for cell in &mut row.children {
                cell.event_handlers
                    .insert("click".into(), "handlePrimaryAction".into());
                cell.event_handlers
                    .insert("pointerenter".into(), "handlePointerEnter".into());
                cell.event_handlers.insert(
                    "focus".into(),
                    "__mesh_embed__::@mesh/shared::alreadyNamespaced".into(),
                );
                cell.event_handler_calls.insert(
                    "change".into(),
                    EventHandlerCall {
                        handler: "handleValueChange".into(),
                        args: Vec::new(),
                    },
                );
            }
        }
        tree
    }

    #[test]
    fn namespace_event_handlers_matches_legacy_graph_output() {
        let instance_key = "@mesh/settings/local:appearance/import:ThemeControls";
        let tree = representative_handler_tree(3, 4);
        let mut legacy = tree.clone();
        let mut prefixed = tree;

        legacy_namespace_event_handlers(&mut legacy, instance_key);
        namespace_event_handlers(&mut prefixed, instance_key);

        assert_handler_graph_eq(&legacy, &prefixed);
    }

    // cargo test -p mesh-core-interaction --release -- shared_handler_namespace_prefix_beats_per_handler_format --ignored --nocapture
    #[test]
    #[ignore = "release-only handler namespace allocation microbenchmark"]
    fn shared_handler_namespace_prefix_beats_per_handler_format() {
        use std::hint::black_box;
        use std::time::{Duration, Instant};

        let instance_key = "@mesh/settings/local:appearance/import:ThemeControls";
        let template = representative_handler_tree(40, 25);
        let iterations = 200usize;
        let mut legacy_time = Duration::ZERO;
        let mut prefixed_time = Duration::ZERO;
        let mut legacy_total = 0usize;
        let mut prefixed_total = 0usize;

        for iteration in 0..iterations {
            let mut legacy = template.clone();
            let mut prefixed = template.clone();
            if iteration % 2 == 0 {
                let started = Instant::now();
                legacy_namespace_event_handlers(black_box(&mut legacy), black_box(instance_key));
                legacy_time += started.elapsed();
                let started = Instant::now();
                namespace_event_handlers(black_box(&mut prefixed), black_box(instance_key));
                prefixed_time += started.elapsed();
            } else {
                let started = Instant::now();
                namespace_event_handlers(black_box(&mut prefixed), black_box(instance_key));
                prefixed_time += started.elapsed();
                let started = Instant::now();
                legacy_namespace_event_handlers(black_box(&mut legacy), black_box(instance_key));
                legacy_time += started.elapsed();
            }
            assert_handler_graph_eq(&legacy, &prefixed);
            legacy_total = legacy_total
                .wrapping_add(legacy.children[0].children[0].event_handlers["click"].len());
            prefixed_total = prefixed_total
                .wrapping_add(prefixed.children[0].children[0].event_handlers["click"].len());
        }

        eprintln!(
            "ordinary handler graph namespacing: per-handler format {legacy_time:?}; shared prefix {prefixed_time:?}; ratio {:.2}x",
            legacy_time.as_secs_f64() / prefixed_time.as_secs_f64()
        );
        assert_eq!(legacy_total, prefixed_total);
        assert!(prefixed_time < legacy_time);
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
    fn pointer_press_hit_matches_focusable_target_and_bounds() {
        let mut root = WidgetNode::new("surface");
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
        button.layout = LayoutRect {
            x: 24.0,
            y: 20.0,
            width: 40.0,
            height: 20.0,
        };
        root.children.push(button);

        let hit = pointer_press_hit(&root, 48.0, 30.0);

        let target = hit.target.expect("focusable cell target");
        let focusable = hit.focusable.expect("focusable cell");
        assert_eq!(target.key, "button");
        assert_eq!(focusable.key, "button");
        assert_eq!(
            Some(target.key.to_owned()),
            crate::focus::find_focusable_at(&root, 48.0, 30.0)
        );
        assert_eq!(
            target.bounds,
            find_node_bounds_by_key(&root, "button", 0.0, 0.0).unwrap()
        );
    }

    #[test]
    fn pointer_press_hit_uses_clickable_ancestor_when_no_focusable_node_matches() {
        let mut root = WidgetNode::new("surface");
        root.attributes.insert("_mesh_key".into(), "root".into());
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 40.0,
        };
        let mut clickable = WidgetNode::new("box");
        clickable
            .attributes
            .insert("_mesh_key".into(), "clickable".into());
        clickable
            .event_handlers
            .insert("click".into(), "onClick".into());
        clickable.layout = LayoutRect {
            x: 10.0,
            y: 5.0,
            width: 60.0,
            height: 25.0,
        };
        let mut label = WidgetNode::new("label");
        label.attributes.insert("_mesh_key".into(), "label".into());
        label.layout = LayoutRect {
            x: 12.0,
            y: 7.0,
            width: 20.0,
            height: 10.0,
        };
        clickable.children.push(label);
        root.children.push(clickable);

        let hit = pointer_press_hit(&root, 15.0, 10.0);

        assert!(hit.focusable.is_none());
        let target = hit.target.expect("clickable ancestor target");
        assert_eq!(target.key, "clickable");
        assert_eq!(
            target.bounds,
            find_node_bounds_by_key(&root, "clickable", 0.0, 0.0).unwrap()
        );
    }

    #[test]
    fn pointer_event_handler_hit_matches_nearest_handler_ancestor() {
        let mut root = WidgetNode::new("surface");
        root.attributes.insert("_mesh_key".into(), "root".into());
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 80.0,
        };
        let mut scroll_owner = WidgetNode::new("box");
        scroll_owner
            .attributes
            .insert("_mesh_key".into(), "scroll-owner".into());
        scroll_owner
            .event_handlers
            .insert("scroll".into(), "onScroll".into());
        scroll_owner.layout = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 80.0,
            height: 40.0,
        };
        let mut label = WidgetNode::new("label");
        label.attributes.insert("_mesh_key".into(), "label".into());
        label.layout = LayoutRect {
            x: 20.0,
            y: 20.0,
            width: 20.0,
            height: 10.0,
        };
        scroll_owner.children.push(label);
        root.children.push(scroll_owner);

        let hit = pointer_event_handler_hit(&root, 24.0, 24.0, "scroll").expect("scroll hit");
        let old = find_node_path_at(&root, 24.0, 24.0)
            .and_then(|path| {
                path.into_iter()
                    .rev()
                    .find(|key| find_event_handler(&root, key, "scroll").is_some())
            })
            .expect("old scroll target");

        assert_eq!(hit.key, old);
        assert_eq!(hit.node.tag, "box");
        assert_eq!(
            hit.bounds,
            find_node_bounds_by_key(&root, "scroll-owner", 0.0, 0.0).unwrap()
        );
    }

    #[test]
    fn tooltip_container_bounds_finds_innermost_clipping_ancestor() {
        use mesh_core_elements::style::Overflow;

        let mut root = indexed_tree(6, 8);
        // No ancestor clips → no container.
        assert_eq!(find_tooltip_container_bounds(&root, "cell-2-3"), None);
        // Missing key → no container.
        assert_eq!(find_tooltip_container_bounds(&root, "missing"), None);

        // Root clips: it becomes the container for descendants.
        root.computed_style.overflow_y = Overflow::Hidden;
        assert_eq!(
            find_tooltip_container_bounds(&root, "cell-2-3"),
            Some((0.0, 0.0, 160.0, 120.0))
        );

        // An inner clipping row overrides the root for its own children.
        root.children[2].computed_style.overflow_y = Overflow::Scroll;
        assert_eq!(
            find_tooltip_container_bounds(&root, "cell-2-3"),
            Some(find_node_bounds_by_key(&root, "row-2", 0.0, 0.0).unwrap())
        );

        // The clipping node itself is not its own container.
        assert_eq!(
            find_tooltip_container_bounds(&root, "row-2"),
            Some((0.0, 0.0, 160.0, 120.0))
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
        assert_eq!(row_node.mesh_key().unwrap(), "row-2");
        assert_eq!(
            row_bounds,
            &find_node_bounds_by_key(&root, "row-2", 0.0, 0.0).unwrap()
        );

        let (cell_node, cell_bounds) = found.get("cell-4-3").expect("cell-4-3 found");
        assert_eq!(cell_node.mesh_key().unwrap(), "cell-4-3");
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

    // cargo test -p mesh-core-interaction --release -- pointer_press_hit_beats_press_path_tree_walks --ignored --nocapture
    #[test]
    #[ignore = "release-only press-hit microbenchmark"]
    fn pointer_press_hit_beats_press_path_tree_walks() {
        use std::hint::black_box;
        use std::time::Instant;

        let mut tree = indexed_tree(200, 12);
        for row in &mut tree.children {
            row.tag = "box".into();
            for cell in &mut row.children {
                cell.tag = "box".into();
                cell.layout.y = row.layout.y;
                cell.event_handlers
                    .insert("click".into(), "handleClick".into());
            }
        }
        let x = tree.layout.width - 5.0;
        let y = tree.layout.height - 5.0;
        let iterations = 20_000usize;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let focusable = crate::focus::find_focusable_at(black_box(&tree), x, y);
            let target = focusable.clone().or_else(|| {
                find_node_path_at(&tree, x, y).and_then(|path| {
                    path.into_iter()
                        .rev()
                        .find(|key| find_event_handler(&tree, key, "click").is_some())
                })
            });
            if let Some(key) = target.as_deref() {
                black_box(find_node_bounds_by_key(&tree, key, 0.0, 0.0));
            }
            old_total = old_total.wrapping_add(target.map_or(0, |key| key.len()));
            old_total = old_total.wrapping_add(focusable.map_or(0, |key| key.len()));
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let hit = pointer_press_hit(black_box(&tree), x, y);
            new_total = new_total.wrapping_add(hit.target.map_or(0, |node| node.key.len()));
            new_total = new_total.wrapping_add(hit.focusable.map_or(0, |node| node.key.len()));
            black_box(hit.target.map(|node| node.bounds));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "press hit lookup: multi-walk {old_time:?}; fused {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-interaction --release -- pointer_event_handler_hit_beats_path_handler_walks --ignored --nocapture
    #[test]
    #[ignore = "release-only event-handler-hit microbenchmark"]
    fn pointer_event_handler_hit_beats_path_handler_walks() {
        use std::hint::black_box;
        use std::time::Instant;

        let mut tree = indexed_tree(200, 12);
        for row in &mut tree.children {
            for cell in &mut row.children {
                cell.layout.y = row.layout.y;
                cell.event_handlers
                    .insert("scroll".into(), "handleScroll".into());
            }
        }
        let x = tree.layout.width - 5.0;
        let y = tree.layout.height - 5.0;
        let iterations = 20_000usize;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let target = find_node_path_at(black_box(&tree), x, y).and_then(|path| {
                path.into_iter()
                    .rev()
                    .find(|key| find_event_handler(&tree, key, "scroll").is_some())
            });
            if let Some(key) = target.as_deref() {
                old_total = old_total.wrapping_add(key.len());
                old_total = old_total
                    .wrapping_add(find_node_by_key(&tree, key).map_or(0, |node| node.tag.len()));
                old_total = old_total.wrapping_add(
                    find_node_bounds_by_key(&tree, key, 0.0, 0.0)
                        .map_or(0, |bounds| usize::from(bounds.2 > bounds.0)),
                );
            }
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            if let Some(hit) = pointer_event_handler_hit(black_box(&tree), x, y, "scroll") {
                new_total = new_total.wrapping_add(hit.key.len());
                new_total = new_total.wrapping_add(hit.node.tag.len());
                new_total = new_total.wrapping_add(usize::from(hit.bounds.2 > hit.bounds.0));
            }
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "event-handler hit lookup: path/key walks {old_time:?}; fused {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
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

    #[test]
    fn inspector_hit_finds_deepest_non_interactive_node_and_its_bounds() {
        let mut root = WidgetNode::new("column");
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };
        let mut card = WidgetNode::new("box");
        card.layout = LayoutRect {
            x: 20.0,
            y: 10.0,
            width: 80.0,
            height: 40.0,
        };
        let mut label = WidgetNode::new("text");
        label.layout = LayoutRect {
            x: 30.0,
            y: 18.0,
            width: 50.0,
            height: 16.0,
        };
        card.children.push(label);
        root.children.push(card);

        let hit = inspect_hit_test(&root, 40.0, 24.0).expect("text should be inspectable");
        assert_eq!(hit.node.tag, "text");
        assert_eq!(hit.bounds, (30.0, 18.0, 80.0, 34.0));
    }
}
