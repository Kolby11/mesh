use mesh_core_elements::{
    AffineClipStack, AffineTransform, InteractionTarget, NodeEligibility, WidgetNode,
    child_transform, node_clip, node_eligibility, node_layout_bounds, node_transform,
    root_transform, transformed_layout_at, transformed_offset,
};

mod focus;
mod hit_test;
mod scroll;

pub use focus::{collect_focus_traversal, find_focusable_at, next_focus_target};
pub use hit_test::find_click_handler;
pub use hit_test::{
    InspectHit, PointerEventHandlerHit, PointerHit, PointerPressHit, PointerPressNode,
    TooltipRenderTarget, TooltipTarget, TooltipTargetCache, find_event_handler,
    find_node_bounds_by_key, find_node_by_key, find_node_path_at, find_node_with_bounds_by_key,
    find_nodes_by_keys, find_tooltip_by_key, find_tooltip_container_bounds,
    find_tooltip_target_by_key, find_tooltip_text_by_key, inspect_hit_test, is_input_key,
    is_slider_key, namespace_event_handlers, node_is_source, pointer_event_handler_hit,
    pointer_hit_test, pointer_press_hit, source_element_tag,
};
pub use mesh_core_elements::node_can_receive_target;
pub use scroll::{
    ScrollableHit, ScrollbarAxis, ScrollbarHit, annotate_overflow_node, annotate_overflow_tree,
    find_scrollable_at, find_scrollable_at_with_limits, find_scrollbar_at, measure_content_size,
    scroll_into_view_offsets, scroll_limits,
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

#[allow(dead_code)]
pub(crate) fn node_rect_with_offset(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> ContentBounds {
    let rect = transformed_layout_at(node, offset_x, offset_y);
    (
        rect.x,
        rect.y,
        rect.x + rect.width.max(0.0),
        rect.y + rect.height.max(0.0),
    )
}

pub(crate) fn node_rect_with_transform(
    node: &WidgetNode,
    transform: AffineTransform,
) -> ContentBounds {
    let rect = node_layout_bounds(node, transform);
    (
        rect.x,
        rect.y,
        rect.x + rect.width.max(0.0),
        rect.y + rect.height.max(0.0),
    )
}

pub(crate) fn node_contains_with_transform(
    node: &WidgetNode,
    transform: AffineTransform,
    x: f32,
    y: f32,
) -> bool {
    let Some(inverse) = transform.inverse() else {
        return false;
    };
    let (local_x, local_y) = inverse.transform_point(x, y);
    local_x >= 0.0 && local_x < node.layout.width && local_y >= 0.0 && local_y < node.layout.height
}

pub(crate) fn node_world_transform(parent: AffineTransform, node: &WidgetNode) -> AffineTransform {
    node_transform(parent, node)
}

pub(crate) fn child_world_transform(
    node_world: AffineTransform,
    node: &WidgetNode,
) -> AffineTransform {
    let scroll = node_scroll_offset(node);
    child_transform(node_world, node, scroll.x, scroll.y)
}

pub(crate) fn push_node_clip(
    clips: &AffineClipStack,
    node: &WidgetNode,
    node_world: AffineTransform,
) -> AffineClipStack {
    if node_clips_children(node) {
        clips.push(node_clip(node, node_world))
    } else {
        clips.clone()
    }
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

#[allow(dead_code)]
pub(crate) fn child_offsets_with_scroll(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> (f32, f32) {
    let scroll = node_scroll_offset(node);
    (offset_x - scroll.x, offset_y - scroll.y)
}

#[allow(dead_code)]
pub(crate) fn apply_transform_offset(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> (f32, f32) {
    transformed_offset(node, offset_x, offset_y)
}

#[allow(dead_code)]
pub(crate) fn layout_contains_with_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> bool {
    let rect = transformed_layout_at(node, offset_x, offset_y);
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

pub(crate) fn node_allows(node: &WidgetNode, target: InteractionTarget) -> bool {
    node_eligibility(node).allows(target)
}
