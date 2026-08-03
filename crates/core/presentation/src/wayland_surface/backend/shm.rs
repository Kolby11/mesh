use super::*;
use mesh_core_render::DamageRect;
use smallvec::{SmallVec, smallvec};

pub(super) const SHM_BUFFER_POOL_DEPTH: usize = 2;
pub(super) const SHM_BUFFER_POOL_MAX: usize = 3;
/// Buffers are rounded only when a viewport crops the excess before the
/// compositor sees it. This absorbs the small resize jitter emitted by
/// content-measured surfaces without changing their visible geometry.
pub(super) const SHM_SIZE_CLASS_STEP: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ShmPoolConfig {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) stride: i32,
}

pub(super) fn shm_pool_config_for(
    width: u32,
    height: u32,
    viewport_available: bool,
) -> ShmPoolConfig {
    let round_up = |value: u32| {
        value.max(1).saturating_add(SHM_SIZE_CLASS_STEP - 1) / SHM_SIZE_CLASS_STEP
            * SHM_SIZE_CLASS_STEP
    };
    let (width, height) = if viewport_available {
        (round_up(width), round_up(height))
    } else {
        (width.max(1), height.max(1))
    };
    ShmPoolConfig {
        width,
        height,
        stride: width as i32 * 4,
    }
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
