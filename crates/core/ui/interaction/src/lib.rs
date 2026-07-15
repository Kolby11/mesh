use mesh_core_elements::WidgetNode;

mod focus;
mod hit_test;
mod scroll;

pub use focus::{collect_focus_traversal, find_focusable_at, next_focus_target};
pub use hit_test::find_click_handler;
pub use hit_test::{
    PointerEventHandlerHit, PointerHit, PointerPressHit, PointerPressNode, find_event_handler,
    find_node_bounds_by_key, find_node_by_key, find_node_path_at, find_node_with_bounds_by_key,
    find_nodes_by_keys, find_tooltip_by_key, find_tooltip_container_bounds,
    find_tooltip_text_by_key, is_input_key, is_slider_key, namespace_event_handlers,
    node_is_source, parse_namespaced_handler, pointer_event_handler_hit, pointer_hit_test,
    pointer_press_hit, source_element_tag,
};
pub use scroll::{
    ScrollableHit, annotate_overflow_node, annotate_overflow_tree, find_scrollable_at,
    find_scrollable_at_with_limits, measure_content_size, scroll_into_view_offsets, scroll_limits,
};

pub type ContentBounds = (f32, f32, f32, f32);

#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollOffsetState {
    pub x: f32,
    pub y: f32,
}

pub(crate) fn union_bounds(existing: Option<ContentBounds>, next: ContentBounds) -> ContentBounds {
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

pub(crate) fn intersect_bounds(a: ContentBounds, b: ContentBounds) -> Option<ContentBounds> {
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

pub(crate) fn node_rect_with_offset(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> ContentBounds {
    (
        node.layout.x + offset_x,
        node.layout.y + offset_y,
        node.layout.x + offset_x + node.layout.width.max(0.0),
        node.layout.y + offset_y + node.layout.height.max(0.0),
    )
}

fn node_scroll_offset(node: &WidgetNode) -> ScrollOffsetState {
    let scroll = node.resolved_scroll_metrics();
    ScrollOffsetState {
        x: scroll.x,
        y: scroll.y,
    }
}

pub(crate) fn node_clips_children(node: &WidgetNode) -> bool {
    node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents()
}

pub(crate) fn child_offsets_with_scroll(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> (f32, f32) {
    let scroll = node_scroll_offset(node);
    (offset_x - scroll.x, offset_y - scroll.y)
}

/// Translate the incoming offset by this node's CSS `transform.translate_*`,
/// mirroring what the painter does. Hit-testing must apply the same shift so
/// pointer coordinates resolve to the visually displaced bounds, not the
/// untransformed layout box. Scale and rotation are not yet visually
/// rendered and so are not yet inverted here either.
pub(crate) fn apply_transform_offset(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> (f32, f32) {
    let t = node.computed_style.transform;
    (offset_x + t.translate_x, offset_y + t.translate_y)
}

pub(crate) fn layout_contains_with_offset(
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

pub(crate) fn node_is_hidden(node: &WidgetNode) -> bool {
    node.computed_style.display == mesh_core_elements::style::Display::None
        || node.layout.width <= 0.0
        || node.layout.height <= 0.0
        || node
            .attributes
            .get("hidden")
            .is_some_and(|value| truthy_attribute(value))
}

pub(crate) fn node_is_disabled(node: &WidgetNode) -> bool {
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
