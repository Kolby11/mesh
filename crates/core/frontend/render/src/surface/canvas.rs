use super::{PixelBuffer, PixelCanvasSession};

/// The retained pixels of one presented surface: a CPU buffer, or a GPU
/// render target when the presentation backend can show GPU buffers.
pub enum SurfaceCanvas {
    Raster(PixelBuffer),
    #[cfg(feature = "renderer-gpu")]
    Gpu(crate::gpu::GpuTarget),
}

impl SurfaceCanvas {
    pub fn width(&self) -> u32 {
        match self {
            Self::Raster(buffer) => buffer.width(),
            #[cfg(feature = "renderer-gpu")]
            Self::Gpu(target) => target.width(),
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Self::Raster(buffer) => buffer.height(),
            #[cfg(feature = "renderer-gpu")]
            Self::Gpu(target) => target.height(),
        }
    }

    pub fn is_gpu(&self) -> bool {
        !matches!(self, Self::Raster(_))
    }

    /// A paint session over the canvas; painting, damage clears and scroll
    /// shifts behave alike on both kinds.
    pub fn session(&mut self) -> PixelCanvasSession<'_> {
        match self {
            Self::Raster(buffer) => PixelCanvasSession::new(buffer),
            #[cfg(feature = "renderer-gpu")]
            Self::Gpu(target) => target.session(),
        }
    }

    pub fn as_raster(&self) -> Option<&PixelBuffer> {
        match self {
            Self::Raster(buffer) => Some(buffer),
            #[cfg(feature = "renderer-gpu")]
            Self::Gpu(_) => None,
        }
    }

    pub fn as_raster_mut(&mut self) -> Option<&mut PixelBuffer> {
        match self {
            Self::Raster(buffer) => Some(buffer),
            #[cfg(feature = "renderer-gpu")]
            Self::Gpu(_) => None,
        }
    }
}

impl From<PixelBuffer> for SurfaceCanvas {
    fn from(buffer: PixelBuffer) -> Self {
        Self::Raster(buffer)
    }
}
