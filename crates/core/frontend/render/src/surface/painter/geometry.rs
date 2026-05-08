use super::*;

#[derive(Clone, Copy)]
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

pub(crate) fn fill_rect_clipped(
    buffer: &mut PixelBuffer,
    rect: ClipRect,
    color: Color,
    clip: ClipRect,
) {
    let clipped = intersect_clip(rect, clip);
    if clipped.width <= 0 || clipped.height <= 0 {
        return;
    }

    for py in clipped.y..clipped.y + clipped.height {
        for px in clipped.x..clipped.x + clipped.width {
            buffer.set_pixel(px as u32, py as u32, color);
        }
    }
}

pub(crate) fn fill_rounded_rect_clipped(
    buffer: &mut PixelBuffer,
    rect: ClipRect,
    radius: f32,
    color: Color,
    clip: ClipRect,
) {
    let clipped = intersect_clip(rect, clip);
    if clipped.width <= 0 || clipped.height <= 0 {
        return;
    }

    for py in clipped.y..clipped.y + clipped.height {
        for px in clipped.x..clipped.x + clipped.width {
            let coverage = rounded_rect_coverage(rect, radius, px as f32 + 0.5, py as f32 + 0.5);
            if coverage <= 0.0 {
                continue;
            }
            buffer.blend_pixel_f32(px as u32, py as u32, color, coverage);
        }
    }
}

fn rounded_rect_coverage(rect: ClipRect, radius: f32, px: f32, py: f32) -> f32 {
    let half_w = rect.width.max(0) as f32 * 0.5;
    let half_h = rect.height.max(0) as f32 * 0.5;
    let radius = radius.min(half_w).min(half_h).max(0.0);

    let center_x = rect.x as f32 + half_w;
    let center_y = rect.y as f32 + half_h;
    let local_x = (px - center_x).abs();
    let local_y = (py - center_y).abs();

    let qx = local_x - (half_w - radius);
    let qy = local_y - (half_h - radius);
    let outside_x = qx.max(0.0);
    let outside_y = qy.max(0.0);
    let outside_dist = (outside_x * outside_x + outside_y * outside_y).sqrt();
    let inside_dist = qx.max(qy).min(0.0);
    let signed_distance = outside_dist + inside_dist - radius;

    (0.5 - signed_distance).clamp(0.0, 1.0)
}

pub(super) fn dim_color(color: Color, factor: f32) -> Color {
    Color {
        r: ((color.r as f32) * factor).round().clamp(0.0, 255.0) as u8,
        g: ((color.g as f32) * factor).round().clamp(0.0, 255.0) as u8,
        b: ((color.b as f32) * factor).round().clamp(0.0, 255.0) as u8,
        a: color.a,
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
