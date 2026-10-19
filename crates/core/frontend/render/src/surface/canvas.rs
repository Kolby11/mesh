use super::{PixelBuffer, PixelCanvasSession};

/// UI-owned surface state: retained CPU pixels or a complete immutable scene
/// whose GPU targets remain exclusively on the render thread.
pub enum SurfaceCanvas {
    Raster(PixelBuffer),
    Scene(crate::prepared_scene::SceneCanvas),
}

impl SurfaceCanvas {
    pub fn width(&self) -> u32 {
        match self {
            Self::Raster(buffer) => buffer.width(),
            Self::Scene(scene) => scene.width(),
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Self::Raster(buffer) => buffer.height(),
            Self::Scene(scene) => scene.height(),
        }
    }

    pub fn is_gpu(&self) -> bool {
        !matches!(self, Self::Raster(_))
    }

    pub fn is_scene(&self) -> bool {
        matches!(self, Self::Scene(_))
    }

    /// Open a raster session or the current complete scene recording. Scene
    /// recordings must be begun first and cannot read or shift retained pixels.
    pub fn session(&mut self) -> PixelCanvasSession<'_> {
        match self {
            Self::Raster(buffer) => PixelCanvasSession::new(buffer),
            Self::Scene(scene) => scene.session(),
        }
    }

    pub fn as_raster(&self) -> Option<&PixelBuffer> {
        match self {
            Self::Raster(buffer) => Some(buffer),
            Self::Scene(_) => None,
        }
    }

    pub fn as_raster_mut(&mut self) -> Option<&mut PixelBuffer> {
        match self {
            Self::Raster(buffer) => Some(buffer),
            Self::Scene(_) => None,
        }
    }
}

impl From<PixelBuffer> for SurfaceCanvas {
    fn from(buffer: PixelBuffer) -> Self {
        Self::Raster(buffer)
    }
}
