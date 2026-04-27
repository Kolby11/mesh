/// Shell surface painter: renders components into surfaces.
///
/// This module provides the `CorePainter` which bridges the gap between:
/// - The core's component system (which has the component state to paint)
/// - The render-engine's FrontendRenderEngine (which paints widget trees)
/// - The Wayland surface presentation layer
///
/// The painter is responsible for orchestrating the rendering pipeline:
/// 1. Request component rendering (component builds its widget tree)
/// 2. Delegate tree rendering to the FrontendRenderEngine
/// 3. Handle special overlays (tooltips, debug info)
/// 4. Present the buffer to the surface backend
use super::render::{FrontendRenderEngine, PixelBuffer};

/// Handles rendering of components onto shell surfaces.
pub struct CorePainter {
    engine: FrontendRenderEngine,
}

impl CorePainter {
    /// Create a new core painter.
    pub fn new() -> Self {
        Self {
            engine: FrontendRenderEngine::new(),
        }
    }

    /// Paint a widget tree into the buffer.
    ///
    /// This is the main entry point for rendering a component's tree.
    /// It delegates to the frontend render engine and handles any shell-specific
    /// rendering concerns.
    pub fn paint(&self, tree: &mesh_ui::WidgetNode, buffer: &mut PixelBuffer, scale: f32) {
        self.engine.render_tree(tree, buffer, scale);
    }

    /// Paint a tooltip overlay on top of the rendered tree.
    pub fn paint_tooltip(
        &self,
        text: &str,
        cursor_x: f32,
        cursor_y: f32,
        buffer: &mut PixelBuffer,
        scale: f32,
    ) {
        self.engine
            .render_tooltip(text, cursor_x, cursor_y, buffer, scale);
    }
}

impl Default for CorePainter {
    fn default() -> Self {
        Self::new()
    }
}
