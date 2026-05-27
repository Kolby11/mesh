use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClipRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub(super) fn intersect_clip(a: ClipRect, b: ClipRect) -> ClipRect {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.width).min(b.x + b.width);
    let y2 = (a.y + a.height).min(b.y + b.height);

    ClipRect {
        x: x1,
        y: y1,
        width: (x2 - x1).max(0),
        height: (y2 - y1).max(0),
    }
}

pub(super) fn clip_to_tuple(clip: ClipRect) -> (u32, u32, u32, u32) {
    (
        clip.x.max(0) as u32,
        clip.y.max(0) as u32,
        clip.width.max(0) as u32,
        clip.height.max(0) as u32,
    )
}

pub(super) fn dim_color(color: Color, factor: f32) -> Color {
    Color {
        r: ((color.r as f32) * factor).round().clamp(0.0, 255.0) as u8,
        g: ((color.g as f32) * factor).round().clamp(0.0, 255.0) as u8,
        b: ((color.b as f32) * factor).round().clamp(0.0, 255.0) as u8,
        a: color.a,
    }
}

pub(super) fn opacity_color(color: Color, opacity: f32) -> Color {
    Color {
        a: ((color.a as f32) * opacity.clamp(0.0, 1.0))
            .round()
            .clamp(0.0, 255.0) as u8,
        ..color
    }
}

pub(super) fn node_attr_f32(node: &WidgetNode, key: &str) -> f32 {
    node.attributes
        .get(key)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0)
}

pub(super) fn node_clips_children(node: &WidgetNode) -> bool {
    node.computed_style.overflow_x != Overflow::Visible
        || node.computed_style.overflow_y != Overflow::Visible
}
