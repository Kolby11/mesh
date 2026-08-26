#![allow(dead_code)] // The non-diagnostic wrapper remains for component fixtures.

use super::*;
use crate::shell::PopoverTriggerReference;

pub(super) fn collect_child_surface_requests(
    root: &WidgetNode,
    node: &WidgetNode,
    requests: &mut Vec<ChildSurfaceRequest>,
) {
    let mut diagnostics = Vec::new();
    collect_child_surface_requests_with_diagnostics(root, node, requests, &mut diagnostics);
}

pub(super) fn collect_child_surface_requests_with_diagnostics(
    root: &WidgetNode,
    node: &WidgetNode,
    requests: &mut Vec<ChildSurfaceRequest>,
    diagnostics: &mut Vec<ChildSurfaceDiagnostic>,
) {
    if node.is_promoted_window()
        && let Some(node_key) = node.mesh_key()
    {
        let bounds = bounds_to_i32_rect((
            node.layout.x,
            node.layout.y,
            node.layout.x + node.layout.width,
            node.layout.y + node.layout.height,
        ));
        requests.push(ChildSurfaceRequest {
            node_key: node_key.to_string(),
            kind: ChildSurfaceKind::Window,
            anchor_rect: bounds,
            content_size: (
                mesh_core_render::FractionalScale::identity()
                    .physical_extent_f32(node.layout.width),
                mesh_core_render::FractionalScale::identity()
                    .physical_extent_f32(node.layout.height),
            ),
            content_padding: (0, 0, 0, 0),
            placement: PopoverPlacement::default(),
            popover_trigger: None,
        });
        // A promoted widget owns its entire subtree. Descendant popovers are
        // intentionally not promoted as siblings of this window target; they
        // will be handled by the window's own future child-surface scope.
        return;
    }

    if source_element_tag(node) == "popover"
        && popover_is_open(node)
        && let Some(node_key) = node.mesh_key()
    {
        let placement = match PopoverPlacement::from_node(node) {
            Ok(placement) => Some(placement),
            Err(placement_diagnostics) => {
                diagnostics.extend(placement_diagnostics.into_iter().map(|diagnostic| {
                    ChildSurfaceDiagnostic::Placement {
                        node_key: node_key.to_string(),
                        diagnostic,
                    }
                }));
                None
            }
        };
        if let Some(placement) = placement {
            let (popover_trigger, trigger_is_valid) =
                if let Some(reference) = popover_anchor_reference(node) {
                    (
                        Some(PopoverTriggerReference {
                            reference: reference.to_string(),
                        }),
                        true,
                    )
                } else if let Some(reference) = popover_anchor_attribute_value(node) {
                    diagnostics.push(ChildSurfaceDiagnostic::MissingTrigger {
                        node_key: node_key.to_string(),
                        reference: PopoverTriggerReference {
                            reference: reference.to_string(),
                        },
                    });
                    (None, false)
                } else {
                    (None, true)
                };
            if trigger_is_valid {
                let anchor = match popover_trigger.as_ref() {
                    Some(trigger) => {
                        find_node_bounds_by_reference(root, &trigger.reference, 0.0, 0.0)
                    }
                    None => find_node_bounds_by_key(root, node_key, 0.0, 0.0),
                };
                if let Some(anchor) = anchor {
                    let content = (
                        mesh_core_render::FractionalScale::identity()
                            .physical_extent_f32(node.layout.width),
                        mesh_core_render::FractionalScale::identity()
                            .physical_extent_f32(node.layout.height),
                    );
                    let content_padding = popover_content_padding(node);
                    requests.push(ChildSurfaceRequest {
                        node_key: node_key.to_string(),
                        kind: ChildSurfaceKind::Popover,
                        anchor_rect: bounds_to_i32_rect(anchor),
                        content_size: content,
                        content_padding,
                        placement,
                        popover_trigger,
                    });
                } else if let Some(popover_trigger) = popover_trigger {
                    diagnostics.push(ChildSurfaceDiagnostic::MissingTrigger {
                        node_key: node_key.to_string(),
                        reference: popover_trigger,
                    });
                }
            }
        }
    }

    // A positioned overlay cannot be painted beyond its parent SHM buffer.
    // Explicit `<popover>` nodes above retain their authored anchor/grab
    // behavior; other absolutely positioned descendants are derived from
    // their actual escape bounds below.
    if std::ptr::eq(root, node) {
        let root_bounds = (
            root.layout.x,
            root.layout.y,
            root.layout.x + root.layout.width,
            root.layout.y + root.layout.height,
        );
        collect_overflow_surface_requests(root, root, root_bounds, false, requests, diagnostics);
    }

    for child in &node.children {
        collect_child_surface_requests_with_diagnostics(root, child, requests, diagnostics);
    }
}

fn collect_overflow_surface_requests(
    root: &WidgetNode,
    node: &WidgetNode,
    root_bounds: (f32, f32, f32, f32),
    clipped_by_ancestor: bool,
    requests: &mut Vec<ChildSurfaceRequest>,
    _diagnostics: &mut Vec<ChildSurfaceDiagnostic>,
) {
    let children_clipped = clipped_by_ancestor
        || node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents();

    for child in &node.children {
        // An explicit popover owns its whole subtree in the promoted surface;
        // its internal positioned elements must not also become siblings of
        // that popup on the parent surface.
        if source_element_tag(child) == "popover" {
            continue;
        }

        if !children_clipped
            && child.computed_style.position == mesh_core_elements::style::Position::Absolute
            && let Some(node_key) = child.mesh_key()
            && let Some(bounds) = find_node_bounds_by_key(root, node_key, 0.0, 0.0)
            && bounds_escape_container(bounds, root_bounds)
        {
            let mut placement = PopoverPlacement::default();
            placement.anchor = mesh_core_elements::PopoverAnchor::TopLeft;
            placement.gravity = mesh_core_elements::PopoverGravity::TopLeft;
            let anchor_rect = bounds_to_i32_rect(bounds);
            requests.push(ChildSurfaceRequest {
                node_key: node_key.to_string(),
                kind: ChildSurfaceKind::Overflow,
                anchor_rect,
                content_size: (anchor_rect.2 as u32, anchor_rect.3 as u32),
                content_padding: popover_content_padding(child),
                placement,
                popover_trigger: None,
            });
        }

        collect_overflow_surface_requests(
            root,
            child,
            root_bounds,
            children_clipped,
            requests,
            _diagnostics,
        );
    }
}

fn bounds_escape_container(bounds: (f32, f32, f32, f32), container: (f32, f32, f32, f32)) -> bool {
    bounds.0 < container.0
        || bounds.1 < container.1
        || bounds.2 > container.2
        || bounds.3 > container.3
}

pub(super) fn popover_anchor_reference(popover: &WidgetNode) -> Option<&str> {
    for name in ["anchor-ref", "anchor-target", "anchor-element", "target"] {
        if let Some(value) = non_empty_attr(popover, name) {
            return Some(value);
        }
    }

    None
}

fn popover_anchor_attribute_value(popover: &WidgetNode) -> Option<&str> {
    for name in ["anchor-ref", "anchor-target", "anchor-element", "target"] {
        if let Some(value) = popover.attributes.get(name) {
            return Some(value.trim());
        }
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
    let device = mesh_core_render::FractionalScale::identity().device_layout_rect(
        mesh_core_elements::LayoutRect {
            x: bounds.0,
            y: bounds.1,
            width: bounds.2 - bounds.0,
            height: bounds.3 - bounds.1,
        },
    );
    let left = device.x;
    let top = device.y;
    let right = device.right();
    let bottom = device.bottom();
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
        ComponentInput::PointerButton {
            x,
            y,
            button,
            pressed,
        } => ComponentInput::PointerButton {
            x: x + origin_x,
            y: y + origin_y,
            button,
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
