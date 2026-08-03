use mesh_core_render::DamageRect;
use smallvec::SmallVec;
use std::borrow::Cow;

pub(super) fn copy_bgra_to_canvas(
    src: &[u8],
    canvas: &mut [u8],
    width: u32,
    height: u32,
    canvas_width: u32,
) {
    // wl_shm Argb8888 is B,G,R,A in little-endian memory, matching PixelBuffer.
    let row_bytes = width as usize * 4;
    let src_len = row_bytes * height as usize;
    let canvas_stride = canvas_width as usize * 4;
    if src.len() < src_len || canvas_stride < row_bytes {
        return;
    }
    for row in 0..height as usize {
        let src_start = row * row_bytes;
        let canvas_start = row * canvas_stride;
        let canvas_end = canvas_start + row_bytes;
        if canvas_end > canvas.len() {
            return;
        }
        canvas[canvas_start..canvas_end].copy_from_slice(&src[src_start..src_start + row_bytes]);
    }
}

pub(super) fn full_damage(width: u32, height: u32) -> DamageRect {
    DamageRect {
        x: 0,
        y: 0,
        width: width.max(1),
        height: height.max(1),
    }
}

pub(super) fn clip_damage(rect: DamageRect, bounds: DamageRect) -> Option<DamageRect> {
    let x1 = rect.x.max(bounds.x);
    let y1 = rect.y.max(bounds.y);
    let x2 = rect
        .x
        .saturating_add(rect.width)
        .min(bounds.x.saturating_add(bounds.width));
    let y2 = rect
        .y
        .saturating_add(rect.height)
        .min(bounds.y.saturating_add(bounds.height));
    (x2 > x1 && y2 > y1).then_some(DamageRect {
        x: x1,
        y: y1,
        width: x2 - x1,
        height: y2 - y1,
    })
}

pub(super) fn extend_pending_damage(
    pending: &mut SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]>,
    damage: &[DamageRect],
    bounds: DamageRect,
) {
    if pending.as_slice() == [bounds] {
        return;
    }

    if damage.is_empty() {
        pending.clear();
        pending.push(bounds);
        return;
    }

    for rect in damage.iter().filter_map(|rect| clip_damage(*rect, bounds)) {
        if rect == bounds {
            pending.clear();
            pending.push(bounds);
            return;
        }
        pending.push(rect);
    }

    if pending.len() > MAX_PROTOCOL_DAMAGE_RECTS {
        let union = pending
            .iter()
            .copied()
            .fold(None, |acc, rect| Some(union_damage(acc, rect)))
            .unwrap_or(bounds);
        pending.clear();
        pending.push(union);
    }
}

/// Maximum number of `wl_surface::damage_buffer` calls allowed per commit.
/// When the damage list exceeds this cap the entire surface is marked dirty
/// with a single bounding-union call to avoid unbounded protocol overhead.
pub(super) const MAX_PROTOCOL_DAMAGE_RECTS: usize = 16;

/// Select the rects to pass to `wl_surface::damage_buffer`.
///
/// When `rects.len() <= MAX_PROTOCOL_DAMAGE_RECTS` every rect is forwarded
/// unchanged (same count, same order). When the count exceeds the cap all
/// rects are collapsed into a single bounding union. An empty input yields
/// an empty output — the caller is responsible for skipping the present.
pub(super) fn protocol_damage_rects(
    rects: &[DamageRect],
    width: u32,
    height: u32,
) -> Cow<'_, [DamageRect]> {
    if rects.is_empty() {
        return Cow::Borrowed(&[]);
    }
    if rects.len() <= MAX_PROTOCOL_DAMAGE_RECTS {
        return Cow::Borrowed(rects);
    }
    let union = rects
        .iter()
        .copied()
        .fold(None, |acc, r| Some(union_damage(acc, r)))
        .unwrap_or_else(|| full_damage(width, height));
    Cow::Owned(vec![union])
}

pub(super) fn union_damage(current: Option<DamageRect>, next: DamageRect) -> DamageRect {
    let Some(current) = current else {
        return next;
    };
    if current.width == 0 || current.height == 0 {
        return next;
    }
    if next.width == 0 || next.height == 0 {
        return current;
    }
    let left = current.x.min(next.x);
    let top = current.y.min(next.y);
    let right = current
        .x
        .saturating_add(current.width)
        .max(next.x.saturating_add(next.width));
    let bottom = current
        .y
        .saturating_add(current.height)
        .max(next.y.saturating_add(next.height));
    DamageRect {
        x: left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    }
}

/// Scale a damage rect from logical (CSS) coordinates to physical (device) coordinates.
pub(super) fn scale_damage_rect_to_physical(rect: DamageRect, scale: f32) -> DamageRect {
    let left = (rect.x as f32 * scale).floor() as u32;
    let top = (rect.y as f32 * scale).floor() as u32;
    let right = (rect.x.saturating_add(rect.width) as f32 * scale).ceil() as u32;
    let bottom = (rect.y.saturating_add(rect.height) as f32 * scale).ceil() as u32;
    DamageRect {
        x: left,
        y: top,
        width: right.saturating_sub(left).max(1),
        height: bottom.saturating_sub(top).max(1),
    }
}

/// Clip a damage rect to the physical buffer bounds. Sending out-of-bounds
/// damage is a Wayland protocol error.
pub(super) fn clip_damage_rect_to_buffer(
    rect: DamageRect,
    buffer_w: u32,
    buffer_h: u32,
) -> DamageRect {
    let x = rect.x.min(buffer_w.saturating_sub(1));
    let y = rect.y.min(buffer_h.saturating_sub(1));
    let w = rect.width.min(buffer_w.saturating_sub(x));
    let h = rect.height.min(buffer_h.saturating_sub(y));
    DamageRect {
        x,
        y,
        width: w.max(1),
        height: h.max(1),
    }
}

pub(super) fn copy_bgra_damage_to_canvas(
    src: &[u8],
    canvas: &mut [u8],
    width: u32,
    height: u32,
    canvas_width: u32,
    damage: DamageRect,
) {
    let Some(damage) = clip_damage(damage, full_damage(width, height)) else {
        return;
    };
    let src_stride = width as usize * 4;
    let canvas_stride = canvas_width as usize * 4;
    let row_bytes = damage.width as usize * 4;
    let x_offset = damage.x as usize * 4;
    for row in damage.y as usize..damage.y.saturating_add(damage.height) as usize {
        let src_start = row * src_stride + x_offset;
        let canvas_start = row * canvas_stride + x_offset;
        let src_end = src_start + row_bytes;
        let canvas_end = canvas_start + row_bytes;
        if src_end <= src.len() && canvas_end <= canvas.len() {
            canvas[canvas_start..canvas_end].copy_from_slice(&src[src_start..src_end]);
        }
    }
}
