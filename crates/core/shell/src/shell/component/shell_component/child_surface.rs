use super::*;

pub(super) fn collect_child_surface_requests(
    root: &WidgetNode,
    node: &WidgetNode,
    requests: &mut Vec<ChildSurfaceRequest>,
) {
    if source_element_tag(node) == "popover"
        && popover_is_open(node)
        && let Some(node_key) = node.mesh_key()
        && let Some(anchor) = popover_anchor_bounds(root, node, node_key)
    {
        let content = (
            node.layout.width.ceil().max(1.0) as u32,
            node.layout.height.ceil().max(1.0) as u32,
        );
        let content_padding = popover_content_padding(node);
        requests.push(ChildSurfaceRequest {
            node_key: node_key.to_string(),
            kind: ChildSurfaceKind::Popover,
            anchor_rect: bounds_to_i32_rect(anchor),
            content_size: content,
            content_padding,
            placement: PopoverPlacement::from_node(node),
        });
    }

    // A positioned overlay cannot be painted beyond its parent SHM buffer.
    // Promote only direct, absolutely positioned root children for now: this
    // covers inline overlay UI without mistaking normal flow, scrolling, or a
    // nested clipped region for a new Wayland surface. Explicit `<popover>`
    // nodes above retain their authored anchor/grab behavior.
    if std::ptr::eq(root, node) {
        for child in &node.children {
            if source_element_tag(child) == "popover"
                || child.computed_style.position != mesh_core_elements::style::Position::Absolute
            {
                continue;
            }
            let Some(node_key) = child.mesh_key() else {
                continue;
            };
            let Some(bounds) = find_node_bounds_by_key(root, node_key, 0.0, 0.0) else {
                continue;
            };
            let root_bounds = (
                root.layout.x,
                root.layout.y,
                root.layout.x + root.layout.width,
                root.layout.y + root.layout.height,
            );
            if bounds.0 >= root_bounds.0
                && bounds.1 >= root_bounds.1
                && bounds.2 <= root_bounds.2
                && bounds.3 <= root_bounds.3
            {
                continue;
            }
            let mut placement = PopoverPlacement::default();
            placement.anchor = mesh_core_elements::PopoverAnchor::TopLeft;
            placement.gravity = mesh_core_elements::PopoverGravity::TopLeft;
            requests.push(ChildSurfaceRequest {
                node_key: node_key.to_string(),
                kind: ChildSurfaceKind::Overflow,
                anchor_rect: bounds_to_i32_rect(bounds),
                content_size: (
                    child.layout.width.ceil().max(1.0) as u32,
                    child.layout.height.ceil().max(1.0) as u32,
                ),
                content_padding: popover_content_padding(child),
                placement,
            });
        }
    }

    for child in &node.children {
        collect_child_surface_requests(root, child, requests);
    }
}

pub(super) fn popover_anchor_bounds(
    root: &WidgetNode,
    popover: &WidgetNode,
    popover_key: &str,
) -> Option<(f32, f32, f32, f32)> {
    popover_anchor_reference(popover)
        .and_then(|reference| find_node_bounds_by_reference(root, reference, 0.0, 0.0))
        .or_else(|| find_node_bounds_by_key(root, popover_key, 0.0, 0.0))
}

pub(super) fn popover_anchor_reference(popover: &WidgetNode) -> Option<&str> {
    for name in ["anchor-ref", "anchor-target", "anchor-element", "target"] {
        if let Some(value) = non_empty_attr(popover, name) {
            return Some(value);
        }
    }

    let anchor = non_empty_attr(popover, "anchor")?;
    if mesh_core_elements::PopoverPlacement::from_node(popover).anchor
        == mesh_core_elements::PopoverPlacement::from_attributes(&Default::default()).anchor
        && !matches!(
            anchor.trim().to_ascii_lowercase().as_str(),
            "center"
                | "top"
                | "bottom"
                | "left"
                | "right"
                | "top-left"
                | "top_left"
                | "top-right"
                | "top_right"
                | "bottom-left"
                | "bottom_left"
                | "bottom-right"
                | "bottom_right"
        )
    {
        return Some(anchor);
    }
    None
}

pub(super) fn non_empty_attr<'a>(node: &'a WidgetNode, name: &str) -> Option<&'a str> {
    node.attributes
        .get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

pub(super) fn find_node_bounds_by_reference(
    node: &WidgetNode,
    reference: &str,
    offset_x: f32,
    offset_y: f32,
) -> Option<(f32, f32, f32, f32)> {
    // `node.layout` already stores absolute surface coordinates, so the accumulated
    // offset only carries scroll *deltas* from ancestors. Adding `node.layout.x` per
    // level (as an earlier version did) double-counts and pushes the resolved anchor
    // far off-screen. Transient CSS transforms (a trigger's hover/focus translate
    // bounce) are intentionally ignored so a promoted popup anchors to the trigger's
    // stable layout box and does not jitter with the 1px decorative offset.
    if node.mesh_key().is_some_and(|key| key == reference)
        || node
            .attributes
            .get("ref")
            .is_some_and(|value| value == reference)
        || node
            .attributes
            .get("id")
            .is_some_and(|value| value == reference)
        || node
            .attributes
            .get("bind:this")
            .is_some_and(|value| value == reference)
    {
        return Some((
            node.layout.x + offset_x,
            node.layout.y + offset_y,
            node.layout.x + offset_x + node.layout.width,
            node.layout.y + offset_y + node.layout.height,
        ));
    }

    let scroll = node.resolved_scroll_metrics();
    let scroll_x = scroll.x;
    let scroll_y = scroll.y;
    let child_offset_x = offset_x - scroll_x;
    let child_offset_y = offset_y - scroll_y;
    for child in &node.children {
        if let Some(bounds) =
            find_node_bounds_by_reference(child, reference, child_offset_x, child_offset_y)
        {
            return Some(bounds);
        }
    }
    None
}

pub(super) fn popover_is_open(node: &WidgetNode) -> bool {
    node.attributes.get("open").is_some_and(|value| {
        let value = value.trim().to_ascii_lowercase();
        !matches!(value.as_str(), "" | "false" | "0" | "none")
    })
}

pub(super) fn bounds_to_i32_rect(bounds: (f32, f32, f32, f32)) -> (i32, i32, i32, i32) {
    let left = bounds.0.floor() as i32;
    let top = bounds.1.floor() as i32;
    let right = bounds.2.ceil() as i32;
    let bottom = bounds.3.ceil() as i32;
    (left, top, (right - left).max(1), (bottom - top).max(1))
}

#[cfg(test)]
pub(super) fn translate_child_surface_input(
    input: ComponentInput,
    origin_x: f32,
    origin_y: f32,
) -> ComponentInput {
    match input {
        ComponentInput::PointerMove { x, y } => ComponentInput::PointerMove {
            x: x + origin_x,
            y: y + origin_y,
        },
        ComponentInput::PointerButton { x, y, pressed } => ComponentInput::PointerButton {
            x: x + origin_x,
            y: y + origin_y,
            pressed,
        },
        ComponentInput::Scroll { x, y, dx, dy } => ComponentInput::Scroll {
            x: x + origin_x,
            y: y + origin_y,
            dx,
            dy,
        },
        other => other,
    }
}

pub(super) fn offset_widget_tree_layout(node: &mut WidgetNode, offset_x: f32, offset_y: f32) {
    node.layout.x += offset_x;
    node.layout.y += offset_y;
    for child in &mut node.children {
        offset_widget_tree_layout(child, offset_x, offset_y);
    }
}
