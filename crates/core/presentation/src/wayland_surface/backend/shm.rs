use super::*;
use mesh_core_render::DamageRect;
use smallvec::{SmallVec, smallvec};

pub(super) const SHM_BUFFER_POOL_DEPTH: usize = 2;
pub(super) const SHM_BUFFER_POOL_MAX: usize = 3;
/// Buffers are rounded only when a viewport crops the excess before the
/// compositor sees it. This absorbs the small resize jitter emitted by
/// content-measured surfaces without changing their visible geometry.
pub(super) const SHM_SIZE_CLASS_STEP: u32 = 64;
pub(super) const MAX_SHM_DIMENSION: u32 = 16_384;
pub(super) const MAX_SHM_BUFFER_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_SHM_SURFACE_BYTES: usize = MAX_SHM_BUFFER_BYTES * SHM_BUFFER_POOL_MAX;
pub(super) const MAX_SHM_POOL_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ShmPoolConfig {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) stride: i32,
    pub(super) bytes: usize,
}

pub(super) fn try_shm_pool_config_for(
    width: u32,
    height: u32,
    viewport_available: bool,
) -> Result<ShmPoolConfig, PresentationError> {
    let round_up = |value: u32| {
        value
            .max(1)
            .checked_add(SHM_SIZE_CLASS_STEP - 1)?
            .checked_div(SHM_SIZE_CLASS_STEP)?
            .checked_mul(SHM_SIZE_CLASS_STEP)
    };
    let (width, height) = if viewport_available {
        (
            round_up(width).ok_or_else(|| invalid_shm_config(width, height, "width overflow"))?,
            round_up(height).ok_or_else(|| invalid_shm_config(width, height, "height overflow"))?,
        )
    } else {
        (width.max(1), height.max(1))
    };

    if width > MAX_SHM_DIMENSION || height > MAX_SHM_DIMENSION {
        return Err(invalid_shm_config(
            width,
            height,
            "dimensions exceed the SHM limit",
        ));
    }

    let stride_u32 = width
        .checked_mul(4)
        .ok_or_else(|| invalid_shm_config(width, height, "stride overflow"))?;
    let stride = i32::try_from(stride_u32)
        .map_err(|_| invalid_shm_config(width, height, "stride exceeds protocol range"))?;
    let bytes = usize::try_from(stride_u32)
        .ok()
        .and_then(|stride| stride.checked_mul(usize::try_from(height).ok()?))
        .ok_or_else(|| invalid_shm_config(width, height, "byte length overflow"))?;
    if bytes > MAX_SHM_BUFFER_BYTES {
        return Err(invalid_shm_config(
            width,
            height,
            "buffer bytes exceed the SHM limit",
        ));
    }

    Ok(ShmPoolConfig {
        width,
        height,
        stride,
        bytes,
    })
}

fn invalid_shm_config(width: u32, height: u32, reason: &str) -> PresentationError {
    PresentationError::BufferAlloc(format!("SHM buffer {width}x{height} rejected: {reason}"))
}

pub(super) fn shm_pool_growth_allowed(pool_len: usize, allocation_bytes: usize) -> bool {
    let doubled_len = pool_len.saturating_mul(2);
    let required_len = pool_len.saturating_add(allocation_bytes);
    doubled_len.max(required_len) <= MAX_SHM_POOL_BYTES
}

pub(super) fn viewport_source_dimensions(
    physical_width: u32,
    physical_height: u32,
    buffer_scale: i32,
) -> (f64, f64) {
    let scale = f64::from(buffer_scale.max(1));
    (
        physical_width as f64 / scale,
        physical_height as f64 / scale,
    )
}

#[derive(Debug)]
pub(super) struct SurfaceShmBuffer {
    pub(super) buffer: Buffer,
    pub(super) pending_damage: SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]>,
}

pub(super) fn create_surface_shm_buffer(
    pool: &mut SlotPool,
    config: ShmPoolConfig,
    initial_damage: DamageRect,
) -> Result<SurfaceShmBuffer, PresentationError> {
    let (buffer, _) = pool
        .create_buffer(
            config.width as i32,
            config.height as i32,
            config.stride,
            wl_shm::Format::Argb8888,
        )
        .map_err(|e| PresentationError::BufferAlloc(format!("create_buffer: {e}")))?;
    Ok(SurfaceShmBuffer {
        buffer,
        // Newly allocated SHM memory contains no usable frame. Seed the
        // visible (cropped) extent as dirty even when this frame itself is
        // sparse, otherwise untouched pixels can expose stale memory.
        pending_damage: smallvec![initial_damage],
    })
}
