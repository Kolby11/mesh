/// Rendering backend for MESH.
///
/// Takes a laid-out `WidgetNode` tree from `mesh-ui` and paints pixels into
/// a buffer, which is then submitted to a Wayland surface.
///
/// **Separation boundary**: this crate depends on `mesh-ui` (widget tree) and
/// `mesh-wayland` (surface traits) but does NOT depend on `mesh-service` or
/// `mesh-scripting`.
pub mod buffer;
pub mod debug_overlay;
pub mod dev_window;
pub mod icon;
pub mod layer_shell;
pub mod painter;
pub mod surface;
pub mod text;

pub use buffer::PixelBuffer;
pub use debug_overlay::DebugOverlay;
pub use dev_window::{DevWindowBackend, DevWindowEvent, DevWindowKeyEvent, KeyMods};
pub use layer_shell::{LayerShellBackend, LayerSurfaceConfig};
pub use painter::Painter;
pub use surface::{RenderSurface, SurfaceConfig, SurfaceId};
pub use text_measurer::SharedTextMeasurer;

mod text_measurer {
    use crate::text::TextRenderer;
    use std::cell::RefCell;

    thread_local! {
        static RENDERER: RefCell<TextRenderer> = RefCell::new(TextRenderer::new());
    }

    /// Zero-size token; implements [`mesh_ui::TextMeasurer`] via a thread-local
    /// `TextRenderer` so callers avoid creating a new renderer each frame.
    pub struct SharedTextMeasurer;

    impl mesh_ui::TextMeasurer for SharedTextMeasurer {
        fn measure_text(
            &self,
            text: &str,
            font_family: &str,
            font_size: f32,
            font_weight: u16,
            line_height: f32,
            max_width: Option<f32>,
        ) -> (f32, f32) {
            RENDERER.with(|r| {
                r.borrow().measure_styled(
                    text,
                    font_family,
                    font_size,
                    font_weight,
                    line_height,
                    max_width,
                )
            })
        }
    }
}

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
