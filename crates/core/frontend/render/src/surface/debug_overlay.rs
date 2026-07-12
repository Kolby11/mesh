/// Debug overlay renderer.
///
/// Phase 16 moves the inspector panel into a shell-shipped `.mesh` surface.
/// The native overlay now only owns optional layout-bounds painting.
use super::buffer::PixelBuffer;
use super::painter::{ClipRect, FrontendRenderEngine};
use mesh_core_elements::style::Color;
use mesh_core_elements::tree::WidgetNode;

/// Mirrors `node_is_explicitly_hidden` in `painter/tree.rs` — the real
/// painter's definition of "not part of this surface's visible output".
/// Promoted `<popover>` wrappers are tagged `hidden="true"` and collapsed to
/// 0x0-with-overflow-visible so their (still full-size) subtree stays intact
/// for the dedicated child `xdg_popup` surface's own paint/bounds pass, while
/// the parent surface skips painting them. The bounds overlay must apply the
/// same skip, or it walks into that leftover full-size subtree and draws a
/// second, stale set of boxes at the collapsed in-flow position in the parent
/// surface — on top of the correct boxes the child surface already drew.
fn node_is_hidden_from_bounds(node: &WidgetNode) -> bool {
    use mesh_core_elements::style::{Display, Visibility};
    node.computed_style.display == Display::None
        || matches!(
            node.computed_style.visibility,
            Visibility::Hidden | Visibility::Collapse
        )
        || node.attributes.get("hidden").is_some_and(|value| {
            matches!(
                value.as_str(),
                "" | "true" | "1" | "hidden" | "disabled" | "checked"
            )
        })
}

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
        let engine = FrontendRenderEngine::new();
        self.paint_layout_bounds_with_engine(&engine, root, buffer, scale);
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
        paint_bounds_recursive(engine, root, buffer, scale, full, 0, 0.0, 0.0);
    }
}

impl Default for DebugOverlay {
    fn default() -> Self {
        Self::new()
    }
}

fn paint_bounds_recursive(
    engine: &FrontendRenderEngine,
    node: &WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    _clip: ClipRect,
    depth: usize,
    offset_x: f32,
    offset_y: f32,
) {
    if node_is_hidden_from_bounds(node) {
        return;
    }

    // Mirror the real painter's offset accumulation (`render_node_with_filter`
    // in painter/tree.rs): a node's own CSS `transform.translate_*` shifts
    // where it (and its subtree) actually paints, so the debug box must
    // apply the same shift — otherwise it's drawn at the pre-transform layout
    // position, which reads as offset up-left of the visibly transformed
    // element (e.g. bubble/popover entrance-transform elements).
    let transform = node.computed_style.transform;
    let offset_x = offset_x + transform.translate_x;
    let offset_y = offset_y + transform.translate_y;

    let color = BOUNDS_COLORS[depth % BOUNDS_COLORS.len()];
    let x = ((node.layout.x + offset_x) * scale) as i32;
    let y = ((node.layout.y + offset_y) * scale) as i32;
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

    let scroll = node.resolved_scroll_metrics();
    let child_offset_x = offset_x - scroll.x;
    let child_offset_y = offset_y - scroll.y;
    for child in &node.children {
        paint_bounds_recursive(
            engine,
            child,
            buffer,
            scale,
            _clip,
            depth + 1,
            child_offset_x,
            child_offset_y,
        );
    }
}

fn paint_bounds_rect(
    engine: &FrontendRenderEngine,
    buffer: &mut PixelBuffer,
    rect: ClipRect,
    color: Color,
    clip: ClipRect,
) {
    engine.fill_rect_clipped(buffer, rect, color, clip);
}
