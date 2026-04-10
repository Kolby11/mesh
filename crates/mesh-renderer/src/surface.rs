/// Render surface management.
///
/// Each render surface wraps a Wayland surface with a pixel buffer.
/// This module provides the configuration and management types.
/// The actual Wayland protocol integration will use `smithay-client-toolkit`.
use crate::buffer::PixelBuffer;
use crate::RenderError;
use mesh_wayland::{Edge, KeyboardMode, Layer};

/// Unique identifier for a render surface.
pub type SurfaceId = u64;

/// Configuration for creating a new render surface.
#[derive(Debug, Clone)]
pub struct SurfaceConfig {
    /// Which screen edge to anchor to.
    pub edge: Edge,
    /// Layer for stacking order.
    pub layer: Layer,
    /// Surface width in pixels.
    pub width: u32,
    /// Surface height in pixels.
    pub height: u32,
    /// Exclusive zone (screen space reserved by this surface).
    pub exclusive_zone: i32,
    /// Keyboard interactivity mode.
    pub keyboard_mode: KeyboardMode,
    /// Namespace identifier (e.g. "mesh-panel").
    pub namespace: String,
}

impl Default for SurfaceConfig {
    fn default() -> Self {
        Self {
            edge: Edge::Top,
            layer: Layer::Top,
            width: 1920,
            height: 32,
            exclusive_zone: 32,
            keyboard_mode: KeyboardMode::None,
            namespace: "mesh".to_string(),
        }
    }
}

/// A render surface: pixel buffer + Wayland surface state.
#[derive(Debug)]
pub struct RenderSurface {
    pub id: SurfaceId,
    pub config: SurfaceConfig,
    pub buffer: PixelBuffer,
    pub scale: f32,
    pub dirty: bool,
}

impl RenderSurface {
    /// Create a new render surface with the given config.
    pub fn new(id: SurfaceId, config: SurfaceConfig) -> Self {
        let buffer = PixelBuffer::new(config.width, config.height);
        Self {
            id,
            config,
            buffer,
            scale: 1.0,
            dirty: true,
        }
    }

    /// Mark the surface as needing a repaint.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Resize the surface and reallocate the buffer.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        self.buffer = PixelBuffer::new(width, height);
        self.dirty = true;
    }
}

/// Manages all render surfaces.
///
/// Stub backend for development. Will be replaced with real Wayland
/// integration using `smithay-client-toolkit` and `wayland-protocols-wlr`.
#[derive(Debug)]
pub struct RenderBackend {
    surfaces: Vec<RenderSurface>,
    next_id: SurfaceId,
}

impl RenderBackend {
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self {
            surfaces: Vec::new(),
            next_id: 1,
        })
    }

    /// Create a new surface.
    pub fn create_surface(&mut self, config: SurfaceConfig) -> Result<SurfaceId, RenderError> {
        let id = self.next_id;
        self.next_id += 1;
        let surface = RenderSurface::new(id, config);
        self.surfaces.push(surface);
        tracing::info!("created render surface {id}");
        Ok(id)
    }

    /// Get a mutable reference to a surface.
    pub fn surface_mut(&mut self, id: SurfaceId) -> Option<&mut RenderSurface> {
        self.surfaces.iter_mut().find(|s| s.id == id)
    }

    /// Get a reference to a surface.
    pub fn surface(&self, id: SurfaceId) -> Option<&RenderSurface> {
        self.surfaces.iter().find(|s| s.id == id)
    }

    /// Remove a surface.
    pub fn destroy_surface(&mut self, id: SurfaceId) {
        self.surfaces.retain(|s| s.id != id);
    }

    /// List all surface IDs.
    pub fn surface_ids(&self) -> Vec<SurfaceId> {
        self.surfaces.iter().map(|s| s.id).collect()
    }
}

impl Default for RenderBackend {
    fn default() -> Self {
        Self::new().expect("failed to create render backend")
    }
}
