/// Paints a `WidgetNode` tree into a `PixelBuffer`.
use crate::buffer::PixelBuffer;
use crate::text::TextRenderer;
use mesh_ui::style::Display;
use mesh_ui::tree::WidgetNode;

/// Walks a widget tree and paints each node into a pixel buffer.
pub struct Painter {
    text_renderer: TextRenderer,
}

impl Painter {
    pub fn new() -> Self {
        Self {
            text_renderer: TextRenderer::new(),
        }
    }

    /// Paint the entire widget tree into the buffer.
    pub fn paint(&self, root: &WidgetNode, buffer: &mut PixelBuffer, scale: f32) {
        self.paint_node(root, buffer, scale);
    }

    fn paint_node(&self, node: &WidgetNode, buffer: &mut PixelBuffer, scale: f32) {
        let style = &node.computed_style;
        if style.display == Display::None {
            return;
        }

        let layout = &node.layout;
        let x = (layout.x * scale) as u32;
        let y = (layout.y * scale) as u32;
        let w = (layout.width * scale) as u32;
        let h = (layout.height * scale) as u32;

        // Draw background.
        if style.background_color.a > 0 {
            let radius = style.border_radius.top_left * scale;
            if radius > 0.5 {
                buffer.fill_rounded_rect(x, y, w, h, radius, style.background_color);
            } else {
                buffer.fill_rect(x, y, w, h, style.background_color);
            }
        }

        // Draw border.
        if style.border_width.top > 0.0 && style.border_color.a > 0 {
            let bw = (style.border_width.top * scale) as u32;
            // Top edge.
            buffer.fill_rect(x, y, w, bw, style.border_color);
            // Bottom edge.
            buffer.fill_rect(x, y + h.saturating_sub(bw), w, bw, style.border_color);
            // Left edge.
            buffer.fill_rect(x, y, bw, h, style.border_color);
            // Right edge.
            buffer.fill_rect(x + w.saturating_sub(bw), y, bw, h, style.border_color);
        }

        // Draw text content.
        if node.tag == "text" {
            let text = node
                .attributes
                .get("text")
                .map(|s| s.as_str())
                .or_else(|| {
                    // Fall back to the first text child's content.
                    // In a real impl this would walk children, but for now
                    // we check the "content" attribute.
                    node.attributes.get("content").map(|s| s.as_str())
                })
                .unwrap_or("");

            if !text.is_empty() {
                let tx = x + (style.padding.left * scale) as u32;
                let ty = y + (style.padding.top * scale) as u32;
                self.text_renderer.render(
                    text,
                    &style.font_family,
                    style.font_size * scale,
                    style.color,
                    buffer,
                    tx,
                    ty,
                );
            }
        }

        // Recurse into children.
        for child in &node.children {
            self.paint_node(child, buffer, scale);
        }
    }
}

impl Default for Painter {
    fn default() -> Self {
        Self::new()
    }
}
