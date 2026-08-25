use mesh_core_render::DamageRect;
use smallvec::SmallVec;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BufferCopyError {
    SourceTooShort,
    CanvasTooShort,
    ArithmeticOverflow,
}

impl std::fmt::Display for BufferCopyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::SourceTooShort => "source storage is shorter than its declared dimensions",
            Self::CanvasTooShort => "SHM canvas is shorter than its declared dimensions",
            Self::ArithmeticOverflow => "buffer dimensions overflow the byte-length calculation",
        };
        f.write_str(message)
    }
}

pub(super) fn copy_bgra_to_canvas(
    src: &[u8],
    canvas: &mut [u8],
    width: u32,
    height: u32,
    canvas_width: u32,
) -> Result<(), BufferCopyError> {
    // wl_shm Argb8888 is B,G,R,A in little-endian memory, matching PixelBuffer.
    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or(BufferCopyError::ArithmeticOverflow)?;
    let row_count = usize::try_from(height).map_err(|_| BufferCopyError::ArithmeticOverflow)?;
    let src_len = row_bytes
        .checked_mul(row_count)
        .ok_or(BufferCopyError::ArithmeticOverflow)?;
    let canvas_stride = usize::try_from(canvas_width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or(BufferCopyError::ArithmeticOverflow)?;
    let canvas_len = canvas_stride
        .checked_mul(row_count)
        .ok_or(BufferCopyError::ArithmeticOverflow)?;
    if src.len() < src_len {
        return Err(BufferCopyError::SourceTooShort);
    }
    if canvas_stride < row_bytes || canvas.len() < canvas_len {
        return Err(BufferCopyError::CanvasTooShort);
    }
    for row in 0..height as usize {
        let src_start = row * row_bytes;
        let canvas_start = row * canvas_stride;
        let canvas_end = canvas_start + row_bytes;
        canvas[canvas_start..canvas_end].copy_from_slice(&src[src_start..src_start + row_bytes]);
    }
    Ok(())
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

/// Restore the copied portion of a frame after a later presentation step
/// failed. Use the same bounded accumulation rule as new damage so retries do
/// not grow a slot's pending list without limit.
pub(super) fn restore_pending_damage(
    pending: &mut SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]>,
    copied_damage: &[DamageRect],
    bounds: DamageRect,
) {
    extend_pending_damage(pending, copied_damage, bounds);
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
#[cfg(test)]
pub(super) fn scale_damage_rect_to_physical(rect: DamageRect, scale: f32) -> DamageRect {
    mesh_core_render::FractionalScale::new(scale)
        .device_rect(rect)
        .to_nonnegative_damage_rect()
        .unwrap_or_default()
}

/// Clip a damage rect to the physical buffer bounds. Sending out-of-bounds
/// damage is a Wayland protocol error.
pub(super) fn clip_damage_rect_to_buffer(
    rect: DamageRect,
    buffer_w: u32,
    buffer_h: u32,
) -> DamageRect {
    mesh_core_render::DeviceRect::from_damage_rect(rect)
        .clip_to_buffer(buffer_w, buffer_h)
        .unwrap_or_default()
}

pub(super) fn copy_bgra_damage_to_canvas(
    src: &[u8],
    canvas: &mut [u8],
    width: u32,
    height: u32,
    canvas_width: u32,
    damage: DamageRect,
) -> Result<(), BufferCopyError> {
    let src_stride = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or(BufferCopyError::ArithmeticOverflow)?;
    let canvas_stride = usize::try_from(canvas_width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or(BufferCopyError::ArithmeticOverflow)?;
    let row_count = usize::try_from(height).map_err(|_| BufferCopyError::ArithmeticOverflow)?;
    let source_len = src_stride
        .checked_mul(row_count)
        .ok_or(BufferCopyError::ArithmeticOverflow)?;
    let canvas_len = canvas_stride
        .checked_mul(row_count)
        .ok_or(BufferCopyError::ArithmeticOverflow)?;
    if src.len() < source_len {
        return Err(BufferCopyError::SourceTooShort);
    }
    if canvas_width < width || canvas.len() < canvas_len {
        return Err(BufferCopyError::CanvasTooShort);
    }
    let Some(damage) = clip_damage(damage, full_damage(width, height)) else {
        return Ok(());
    };
    let row_bytes = usize::try_from(damage.width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or(BufferCopyError::ArithmeticOverflow)?;
    let x_offset = usize::try_from(damage.x)
        .ok()
        .and_then(|x| x.checked_mul(4))
        .ok_or(BufferCopyError::ArithmeticOverflow)?;
    for row in damage.y as usize..damage.y.saturating_add(damage.height) as usize {
        let src_start = row
            .checked_mul(src_stride)
            .and_then(|offset| offset.checked_add(x_offset))
            .ok_or(BufferCopyError::ArithmeticOverflow)?;
        let canvas_start = row
            .checked_mul(canvas_stride)
            .and_then(|offset| offset.checked_add(x_offset))
            .ok_or(BufferCopyError::ArithmeticOverflow)?;
        let src_end = src_start
            .checked_add(row_bytes)
            .ok_or(BufferCopyError::ArithmeticOverflow)?;
        let canvas_end = canvas_start
            .checked_add(row_bytes)
            .ok_or(BufferCopyError::ArithmeticOverflow)?;
        if src_end > src.len() {
            return Err(BufferCopyError::SourceTooShort);
        }
        if canvas_end > canvas.len() {
            return Err(BufferCopyError::CanvasTooShort);
        }
        canvas[canvas_start..canvas_end].copy_from_slice(&src[src_start..src_end]);
    }
    Ok(())
}
