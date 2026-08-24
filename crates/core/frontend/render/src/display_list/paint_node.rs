use std::sync::Arc;

use mesh_core_elements::style::Color;
use mesh_core_elements::{
    AffineClipStack, AffineTransform, LayoutRect, WidgetNode, node_layout_bounds, node_transform,
    root_transform,
};

use super::types::*;

pub(super) fn build_paint_node(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> DisplayPaintNode {
    build_paint_node_with_previous(node, offset_x, offset_y, None)
}

pub(super) fn build_paint_node_with_previous(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    previous: Option<&DisplayPaintNode>,
) -> DisplayPaintNode {
    build_paint_node_with_previous_transform(
        node,
        node_transform(root_transform(offset_x, offset_y), node),
        previous,
    )
}

pub(super) fn build_paint_node_with_previous_transform(
    node: &WidgetNode,
    world_transform: AffineTransform,
    previous: Option<&DisplayPaintNode>,
) -> DisplayPaintNode {
    build_paint_node_with_previous_transform_and_clips(
        node,
        world_transform,
        previous,
        &AffineClipStack::default(),
    )
}

pub(super) fn build_paint_node_with_previous_transform_and_clips(
    node: &WidgetNode,
    world_transform: AffineTransform,
    previous: Option<&DisplayPaintNode>,
    ancestor_clips: &AffineClipStack,
) -> DisplayPaintNode {
    let opacity = node.computed_style.opacity;
    DisplayPaintNode {
        id: node.id,
        transform: world_transform,
        local_layout: LayoutRect {
            x: 0.0,
            y: 0.0,
            width: node.layout.width.max(0.0),
            height: node.layout.height.max(0.0),
        },
        layout: node_layout_bounds(node, world_transform),
        ancestor_clips: ancestor_clips.as_slice().into(),
        style: DisplayPaintStyle {
            // Opacity is applied once when the whole node subtree is
            // composited. Keeping primitive colors untouched is important for
            // overlapping descendants and for non-solid paint payloads.
            background_color: node.computed_style.background_color,
            background_paint: node.computed_style.background_paint.clone(),
            border_color: node.computed_style.border_color,
            border_width: node.computed_style.border_width,
            border_radius: node.computed_style.border_radius,
            color: node.computed_style.color,
            padding: node.computed_style.padding,
            overflow_x: node.computed_style.overflow_x,
            overflow_y: node.computed_style.overflow_y,
            font_family: node.computed_style.font_family.clone(),
            font_size: node.computed_style.font_size,
            font_weight: node.computed_style.font_weight,
            font_style: node.computed_style.font_style,
            line_height: node.computed_style.line_height,
            text_align: node.computed_style.text_align,
            text_overflow: node.computed_style.text_overflow,
            text_direction: node.computed_style.text_direction,
            opacity,
            box_shadow: node.computed_style.box_shadow,
            filter: node.computed_style.filter,
            backdrop_filter: node.computed_style.backdrop_filter,
            mix_blend_mode: node.computed_style.mix_blend_mode,
            icon_fill: node.computed_style.icon_fill,
            icon_weight: node.computed_style.icon_weight,
            icon_grade: node.computed_style.icon_grade,
            icon_optical_size: node.computed_style.icon_optical_size,
        },
        content: build_paint_content_with_previous(node, previous.map(|node| &node.content)),
        scrollbars: {
            let scroll = node.resolved_scroll_metrics();
            DisplayScrollbars {
                max_x: scroll.max_x,
                max_y: scroll.max_y,
                scroll_x: scroll.x,
                scroll_y: scroll.y,
                content_width: scroll.content_width,
                content_height: scroll.content_height,
            }
        },
    }
}

pub(super) fn transformed_layout_at(node: &WidgetNode, offset_x: f32, offset_y: f32) -> LayoutRect {
    mesh_core_elements::transformed_layout_at(node, offset_x, offset_y)
}

#[cfg(test)]
pub(super) fn build_paint_content(node: &WidgetNode) -> DisplayPaintContent {
    build_paint_content_with_previous(node, None)
}

pub(super) fn build_paint_content_with_previous(
    node: &WidgetNode,
    previous: Option<&DisplayPaintContent>,
) -> DisplayPaintContent {
    match node.tag.as_str() {
        "text" => DisplayPaintContent::Text(DisplayTextPaint {
            text: retained_display_str(
                node.attributes
                    .get("text")
                    .or_else(|| node.attributes.get("content"))
                    .map_or("", String::as_str),
                match previous {
                    Some(DisplayPaintContent::Text(text)) => Some(&text.text),
                    _ => None,
                },
            ),
            selection: build_text_selection(node),
        }),
        "input" => {
            let value = retained_display_str(
                node.attributes.get("value").map_or("", String::as_str),
                match previous {
                    Some(DisplayPaintContent::Input(input)) => Some(&input.value),
                    _ => None,
                },
            );
            let placeholder = retained_display_str(
                node.attributes
                    .get("placeholder")
                    .map_or("", String::as_str),
                match previous {
                    Some(DisplayPaintContent::Input(input)) => Some(&input.placeholder),
                    _ => None,
                },
            );
            DisplayPaintContent::Input(DisplayInputPaint {
                preedit: build_input_preedit(node, value.as_ref()),
                value,
                placeholder,
                mask_text: node
                    .attributes
                    .get("type")
                    .is_some_and(|value| value == "password"),
                focused: node
                    .attributes
                    .get("_mesh_focused")
                    .is_some_and(|value| value == "true"),
            })
        }
        "slider" => DisplayPaintContent::Slider(DisplaySliderPaint {
            min: attr_f32_with_default(node, "min", 0.0),
            max: attr_f32_with_default(node, "max", 100.0),
            value: attr_f32_with_default(node, "value", 50.0),
            vertical: node
                .attributes
                .get("orient")
                .is_some_and(|value| value == "vertical"),
        }),
        "icon" => DisplayPaintContent::Icon(DisplayIconPaint {
            src: retained_optional_display_str(
                node.attributes.get("src").map(String::as_str),
                match previous {
                    Some(DisplayPaintContent::Icon(icon)) => icon.src.as_ref(),
                    _ => None,
                },
            ),
            name: retained_optional_display_str(
                node.attributes.get("name").map(String::as_str),
                match previous {
                    Some(DisplayPaintContent::Icon(icon)) => icon.name.as_ref(),
                    _ => None,
                },
            ),
            size: node
                .attributes
                .get("size")
                .and_then(|value| value.parse::<u32>().ok()),
        }),
        "checkbox" if node_is_checked(node) => {
            DisplayPaintContent::Checkmark(DisplayCheckmarkPaint {
                kind: CheckmarkKind::Check,
            })
        }
        "radio" if node_is_checked(node) => DisplayPaintContent::Checkmark(DisplayCheckmarkPaint {
            kind: CheckmarkKind::Dot,
        }),
        _ => DisplayPaintContent::None,
    }
}

fn build_input_preedit(node: &WidgetNode, value: &str) -> Option<DisplayInputPreedit> {
    let parse = |attribute| node.attributes.get(attribute)?.parse::<usize>().ok();
    let preedit = DisplayInputPreedit {
        start: parse("_mesh_preedit_start")?,
        end: parse("_mesh_preedit_end")?,
        cursor_begin: parse("_mesh_preedit_cursor_begin")?,
        cursor_end: parse("_mesh_preedit_cursor_end")?,
    };
    let valid_boundary = |offset| offset <= value.len() && value.is_char_boundary(offset);
    (preedit.start < preedit.end
        && preedit.end <= value.len()
        && valid_boundary(preedit.start)
        && valid_boundary(preedit.end)
        && preedit.cursor_begin >= preedit.start
        && preedit.cursor_begin <= preedit.end
        && preedit.cursor_end >= preedit.start
        && preedit.cursor_end <= preedit.end
        && valid_boundary(preedit.cursor_begin)
        && valid_boundary(preedit.cursor_end))
    .then_some(preedit)
}

pub(super) fn retained_display_str(value: &str, previous: Option<&Arc<str>>) -> Arc<str> {
    match previous {
        Some(previous) if previous.as_ref() == value => Arc::clone(previous),
        _ => Arc::from(value),
    }
}

pub(super) fn retained_optional_display_str(
    value: Option<&str>,
    previous: Option<&Arc<str>>,
) -> Option<Arc<str>> {
    value.map(|value| retained_display_str(value, previous))
}

/// A `checkbox`/`radio` is checked when its `checked` attribute is present and
/// not an explicit false value (`checked`, `checked="true"`, `checked="1"`).
pub(crate) fn node_is_checked(node: &WidgetNode) -> bool {
    node.attributes
        .get("checked")
        .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1" | "checked"))
}

pub(super) fn build_text_selection(node: &WidgetNode) -> Option<DisplayTextSelectionPaint> {
    Some(DisplayTextSelectionPaint {
        background: Color::from_hex(node.attributes.get("_mesh_selection_background")?)?,
        foreground: Color::from_hex(node.attributes.get("_mesh_selection_foreground")?)?,
        anchor_x: node
            .attributes
            .get("_mesh_selection_anchor_x")?
            .parse::<f32>()
            .ok()?,
        anchor_y: node
            .attributes
            .get("_mesh_selection_anchor_y")?
            .parse::<f32>()
            .ok()?,
        focus_x: node
            .attributes
            .get("_mesh_selection_focus_x")?
            .parse::<f32>()
            .ok()?,
        focus_y: node
            .attributes
            .get("_mesh_selection_focus_y")?
            .parse::<f32>()
            .ok()?,
        text_x: attr_f32(node, "_mesh_selection_text_x"),
        text_y: attr_f32(node, "_mesh_selection_text_y"),
    })
}

pub(super) fn attr_f32(node: &WidgetNode, key: &str) -> f32 {
    attr_f32_with_default(node, key, 0.0)
}

pub(super) fn attr_f32_with_default(node: &WidgetNode, key: &str, default: f32) -> f32 {
    node.attributes
        .get(key)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(default)
}

pub(super) fn union_display_clip(a: DisplayListClip, b: DisplayListClip) -> DisplayListClip {
    if a.width <= 0 || a.height <= 0 {
        return b;
    }
    if b.width <= 0 || b.height <= 0 {
        return a;
    }
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    DisplayListClip {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

pub(super) fn intersect_display_clip(a: DisplayListClip, b: DisplayListClip) -> DisplayListClip {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.width).min(b.x + b.width);
    let y2 = (a.y + a.height).min(b.y + b.height);

    DisplayListClip {
        x: x1,
        y: y1,
        width: (x2 - x1).max(0),
        height: (y2 - y1).max(0),
    }
}
