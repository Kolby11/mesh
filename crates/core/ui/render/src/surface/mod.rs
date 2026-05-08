mod buffer;
mod debug_overlay;
mod glyph;
mod icon;
mod painter;
mod text;

pub mod bridge;

use std::cell::RefCell;
use std::collections::HashMap;

pub use bridge::{LayerSurfaceConfig, WindowEvent, WindowKeyEvent};
pub use buffer::PixelBuffer;
pub use debug_overlay::DebugOverlay;
pub use glyph::GlyphAxes;
pub use painter::FrontendRenderEngine;
pub use text::{SharedTextMeasurer, TextRenderer};

thread_local! {
    static FRONTEND_RENDERER: RefCell<FrontendRenderEngine> = RefCell::new(FrontendRenderEngine::new());
}

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

pub struct RenderEngine {
    bridge: bridge::PresentationBridge,
}

impl RenderEngine {
    pub fn select() -> Self {
        Self {
            bridge: bridge::PresentationBridge::select(),
        }
    }

    pub fn configure(&mut self, surface_id: &str, cfg: LayerSurfaceConfig) {
        self.bridge.configure(surface_id, cfg);
    }

    pub fn present(
        &mut self,
        surface_id: &str,
        title: &str,
        visible: bool,
        buffer: &PixelBuffer,
    ) -> Result<(), RenderError> {
        self.bridge.present(surface_id, title, visible, buffer)
    }

    pub fn surface_size(&mut self, surface_id: &str) -> Result<Option<(u32, u32)>, RenderError> {
        self.bridge.surface_size(surface_id)
    }

    pub fn pump(&mut self) {
        self.bridge.pump();
    }

    pub fn poll_events(&mut self) -> Vec<WindowEvent> {
        self.bridge.poll_events()
    }
}

impl Default for RenderEngine {
    fn default() -> Self {
        Self::select()
    }
}

pub fn paint_frontend_tree(
    tree: &mesh_core_elements::WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    tooltip: Option<(&str, f32, f32)>,
) {
    paint_frontend_tree_at(tree, buffer, scale, 0.0, 0.0, tooltip);
}

pub fn paint_frontend_tree_at(
    tree: &mesh_core_elements::WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    tooltip: Option<(&str, f32, f32)>,
) {
    paint_frontend_tree_at_for_module(tree, buffer, scale, offset_x, offset_y, tooltip, None);
}

/// Paint a frontend tree, telling the icon resolver which module owns the
/// tree. Lets the painter consult per-module icon bindings (preferred pack,
/// declared mappings, user overrides) before falling back to shell-wide
/// defaults. Pass `None` for `module_id` to use the legacy shell-wide
/// resolution path.
pub fn paint_frontend_tree_at_for_module(
    tree: &mesh_core_elements::WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    tooltip: Option<(&str, f32, f32)>,
    module_id: Option<&str>,
) {
    FRONTEND_RENDERER.with(|engine| {
        let engine = engine.borrow();
        engine.render_tree_at_for_module(tree, buffer, scale, offset_x, offset_y, module_id);
        if let Some((tooltip_text, x, y)) = tooltip {
            engine.render_tooltip(tooltip_text, x + offset_x, y + offset_y, buffer, scale);
        }
    });
}

pub fn coalesce_pointer_moves(events: Vec<WindowEvent>) -> Vec<WindowEvent> {
    if events.len() < 2 {
        return events;
    }

    let mut output = Vec::with_capacity(events.len());
    let mut pending_moves: HashMap<String, WindowEvent> = HashMap::new();

    for event in events {
        match event {
            WindowEvent::PointerMove { surface_id, x, y } => {
                pending_moves.insert(
                    surface_id.clone(),
                    WindowEvent::PointerMove { surface_id, x, y },
                );
            }
            event => {
                let surface_id = event_surface_id(&event).to_string();
                if let Some(pointer_move) = pending_moves.remove(&surface_id) {
                    output.push(pointer_move);
                }
                output.push(event);
            }
        }
    }

    output.extend(pending_moves.into_values());
    output
}

pub fn event_surface_id(event: &WindowEvent) -> &str {
    match event {
        WindowEvent::PointerMove { surface_id, .. }
        | WindowEvent::PointerButton { surface_id, .. }
        | WindowEvent::Scroll { surface_id, .. }
        | WindowEvent::Key { surface_id, .. }
        | WindowEvent::Char { surface_id, .. } => surface_id,
    }
}
