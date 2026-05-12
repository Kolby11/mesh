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
    let x = clipped.x.max(0) as u32;
    let y = clipped.y.max(0) as u32;
    let w = clipped.width as u32;
    let h = clipped.height as u32;
    buffer.clear_rect(x, y, w, h, color);
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

    let half_w = (rect.width.max(0) as f32) * 0.5;
    let half_h = (rect.height.max(0) as f32) * 0.5;
    let radius = radius.max(0.0).min(half_w).min(half_h);

    // Solid rectangles (or radius<0.5 px) skip the AA rounded path entirely.
    if radius < 0.5 {
        buffer.clear_rect(
            clipped.x.max(0) as u32,
            clipped.y.max(0) as u32,
            clipped.width as u32,
            clipped.height as u32,
            color,
        );
        return;
    }

    if buffer.fill_rounded_rect_clipped(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        radius,
        color,
        (clipped.x, clipped.y, clipped.width, clipped.height),
    ) {
        return;
    }

    // Fallback: original per-pixel coverage path. Hit only when tiny_skia rejects
    // the geometry (e.g. degenerate sizes that survive earlier clipping).
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

pub(crate) fn stroke_rounded_rect_clipped(
    buffer: &mut PixelBuffer,
    rect: ClipRect,
    radius: f32,
    stroke_width: i32,
    color: Color,
    clip: ClipRect,
) -> bool {
    if stroke_width <= 0 {
        return false;
    }

    let clipped = intersect_clip(rect, clip);
    if clipped.width <= 0 || clipped.height <= 0 {
        return true;
    }

    let half_w = (rect.width.max(0) as f32) * 0.5;
    let half_h = (rect.height.max(0) as f32) * 0.5;
    let radius = radius.max(0.0).min(half_w).min(half_h);
    if radius < 0.5 {
        return false;
    }

    buffer.stroke_rounded_rect_clipped(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        radius,
        stroke_width as f32,
        color,
        (clipped.x, clipped.y, clipped.width, clipped.height),
    )
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
