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

    // Solid rectangles (or radius<0.5 px) skip the AA path entirely — clear_rect
    // is a SIMD-friendly memcpy and produces identical output.
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

    if rounded_rect_via_tiny_skia(buffer, rect, radius, color, clipped).is_some() {
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

fn rounded_rect_via_tiny_skia(
    buffer: &mut PixelBuffer,
    rect: ClipRect,
    radius: f32,
    color: Color,
    clipped: ClipRect,
) -> Option<()> {
    let buffer_width = buffer.width;
    let buffer_height = buffer.height;
    let mut pixmap =
        tiny_skia::PixmapMut::from_bytes(&mut buffer.data, buffer_width, buffer_height)?;

    let path = build_rounded_rect_path(rect, radius)?;
    let path_bounds = path.bounds();
    if path_bounds.width() <= 0.0 || path_bounds.height() <= 0.0 {
        return None;
    }

    let mut paint = tiny_skia::Paint::default();
    // PixelBuffer is BGRA in memory; tiny_skia is RGBA. Swap r<->b in the input
    // so tiny_skia's writes land on our blue/red channels correctly. Alpha is
    // unaffected.
    paint.set_color_rgba8(color.b, color.g, color.r, color.a);
    paint.anti_alias = true;

    let needs_clip_mask = clipped.x > rect.x
        || clipped.y > rect.y
        || clipped.x + clipped.width < rect.x + rect.width
        || clipped.y + clipped.height < rect.y + rect.height;
    let mask = if needs_clip_mask {
        Some(build_clip_mask(buffer_width, buffer_height, clipped)?)
    } else {
        None
    };

    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        mask.as_ref(),
    );
    Some(())
}

fn build_rounded_rect_path(rect: ClipRect, radius: f32) -> Option<tiny_skia::Path> {
    let l = rect.x as f32;
    let t = rect.y as f32;
    let r = (rect.x + rect.width) as f32;
    let b = (rect.y + rect.height) as f32;
    if r <= l || b <= t {
        return None;
    }

    let radius = radius.max(0.0).min((r - l) * 0.5).min((b - t) * 0.5);
    // Cubic Bezier control-point distance approximating a quarter circle.
    const KAPPA: f32 = 0.5522847498307933;
    let cr = radius * KAPPA;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(l + radius, t);
    pb.line_to(r - radius, t);
    pb.cubic_to(r - radius + cr, t, r, t + radius - cr, r, t + radius);
    pb.line_to(r, b - radius);
    pb.cubic_to(r, b - radius + cr, r - radius + cr, b, r - radius, b);
    pb.line_to(l + radius, b);
    pb.cubic_to(l + radius - cr, b, l, b - radius + cr, l, b - radius);
    pb.line_to(l, t + radius);
    pb.cubic_to(l, t + radius - cr, l + radius - cr, t, l + radius, t);
    pb.close();
    pb.finish()
}

fn build_clip_mask(
    buffer_width: u32,
    buffer_height: u32,
    clipped: ClipRect,
) -> Option<tiny_skia::Mask> {
    let mut mask = tiny_skia::Mask::new(buffer_width, buffer_height)?;
    let rect = tiny_skia::Rect::from_xywh(
        clipped.x as f32,
        clipped.y as f32,
        clipped.width as f32,
        clipped.height as f32,
    )?;
    let path = tiny_skia::PathBuilder::from_rect(rect);
    mask.fill_path(
        &path,
        tiny_skia::FillRule::Winding,
        true,
        tiny_skia::Transform::identity(),
    );
    Some(mask)
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
