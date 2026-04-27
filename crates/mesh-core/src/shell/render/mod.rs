mod buffer;
mod debug_overlay;
mod icon;
mod painter;
mod text;

pub mod bridge;

use std::cell::RefCell;
use std::collections::HashMap;

pub use bridge::{LayerSurfaceConfig, WindowEvent, WindowKeyEvent};
pub use buffer::PixelBuffer;
pub use debug_overlay::DebugOverlay;
pub use painter::FrontendRenderEngine;
pub use text::SharedTextMeasurer;

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
    tree: &mesh_ui::WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    tooltip: Option<(&str, f32, f32)>,
) {
    FRONTEND_RENDERER.with(|engine| {
        let engine = engine.borrow();
        engine.render_tree(tree, buffer, scale);
        if let Some((tooltip_text, x, y)) = tooltip {
            engine.render_tooltip(tooltip_text, x, y, buffer, scale);
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
