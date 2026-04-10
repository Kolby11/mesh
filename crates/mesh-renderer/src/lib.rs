/// Rendering backend for MESH.
///
/// Takes a laid-out `WidgetNode` tree from `mesh-ui` and paints pixels into
/// a buffer, which is then submitted to a Wayland surface.
///
/// **Separation boundary**: this crate depends on `mesh-ui` (widget tree) and
/// `mesh-wayland` (surface traits) but does NOT depend on `mesh-service` or
/// `mesh-scripting`.

pub mod buffer;
pub mod painter;
pub mod surface;
pub mod text;

pub use buffer::PixelBuffer;
pub use painter::Painter;
pub use surface::{RenderSurface, SurfaceConfig, SurfaceId};

/// Errors from the rendering subsystem.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("failed to connect to Wayland: {0}")]
    WaylandConnect(String),

    #[error("failed to create surface: {0}")]
    SurfaceCreate(String),

    #[error("protocol not supported: {0}")]
    ProtocolUnsupported(String),

    #[error("buffer allocation failed: {0}")]
    BufferAlloc(String),
}
