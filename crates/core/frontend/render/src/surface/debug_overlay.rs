/// Debug overlay renderer.
///
/// Phase 16 moves the inspector panel into a shell-shipped `.mesh` surface.
/// The native overlay now only owns optional layout-bounds painting.
use super::buffer::PixelBuffer;
use super::painter::{ClipRect, FrontendRenderEngine, fill_rect_clipped};
use mesh_core_elements::style::Color;
use mesh_core_elements::tree::WidgetNode;

// Layout bounds palette — depth 0..7
const BOUNDS_COLORS: [Color; 8] = [
    Color {
        r: 255,
        g: 80,
        b: 80,
        a: 180,
    },
    Color {
        r: 255,
        g: 160,
        b: 60,
        a: 180,
    },
    Color {
        r: 220,
        g: 220,
        b: 60,
        a: 180,
    },
    Color {
        r: 80,
        g: 220,
        b: 80,
        a: 180,
    },
    Color {
        r: 60,
        g: 200,
        b: 255,
        a: 180,
    },
    Color {
        r: 120,
        g: 100,
        b: 255,
        a: 180,
    },
    Color {
        r: 255,
        g: 80,
        b: 200,
        a: 180,
    },
    Color {
        r: 200,
        g: 200,
        b: 200,
        a: 180,
    },
];

pub struct DebugOverlay;

impl DebugOverlay {
    pub fn new() -> Self {
        Self
    }

    /// Draw coloured bounding-box outlines for every node in the widget tree.
    pub fn paint_layout_bounds(&self, root: &WidgetNode, buffer: &mut PixelBuffer, scale: f32) {
        let bw = buffer.width as i32;
        let bh = buffer.height as i32;
        let full = ClipRect {
            x: 0,
            y: 0,
            width: bw,
            height: bh,
        };
        paint_bounds_recursive(None, root, buffer, scale, full, 0);
    }

    pub(crate) fn paint_layout_bounds_with_engine(
        &self,
        engine: &FrontendRenderEngine,
        root: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
    ) {
        let bw = buffer.width as i32;
        let bh = buffer.height as i32;
        let full = ClipRect {
            x: 0,
            y: 0,
            width: bw,
            height: bh,
        };
        paint_bounds_recursive(Some(engine), root, buffer, scale, full, 0);
    }
}

impl Default for DebugOverlay {
    fn default() -> Self {
        Self::new()
    }
}

fn paint_bounds_recursive(
    engine: Option<&FrontendRenderEngine>,
    node: &WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    _clip: ClipRect,
    depth: usize,
) {
    use mesh_core_elements::style::Display;
    if node.computed_style.display == Display::None {
        return;
    }

    let color = BOUNDS_COLORS[depth % BOUNDS_COLORS.len()];
    let x = (node.layout.x * scale) as i32;
    let y = (node.layout.y * scale) as i32;
    let w = (node.layout.width * scale) as i32;
    let h = (node.layout.height * scale) as i32;

    if w > 0 && h > 0 {
        let bw = buffer.width as i32;
        let bh = buffer.height as i32;
        let full = ClipRect {
            x: 0,
            y: 0,
            width: bw,
            height: bh,
        };
        paint_bounds_rect(
            engine,
            buffer,
            ClipRect {
                x,
                y,
                width: w,
                height: 1,
            },
            color,
            full,
        );
        paint_bounds_rect(
            engine,
            buffer,
            ClipRect {
                x,
                y: y + h - 1,
                width: w,
                height: 1,
            },
            color,
            full,
        );
        paint_bounds_rect(
            engine,
            buffer,
            ClipRect {
                x,
                y,
                width: 1,
                height: h,
            },
            color,
            full,
        );
        paint_bounds_rect(
            engine,
            buffer,
            ClipRect {
                x: x + w - 1,
                y,
                width: 1,
                height: h,
            },
            color,
            full,
        );
    }

    for child in &node.children {
        paint_bounds_recursive(engine, child, buffer, scale, _clip, depth + 1);
    }
}

fn paint_bounds_rect(
    engine: Option<&FrontendRenderEngine>,
    buffer: &mut PixelBuffer,
    rect: ClipRect,
    color: Color,
    clip: ClipRect,
) {
    if let Some(engine) = engine {
        engine.fill_rect_clipped(buffer, rect, color, clip);
    } else {
        fill_rect_clipped(buffer, rect, color, clip);
    }
}
