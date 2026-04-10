/// Paints a `WidgetNode` tree into a `PixelBuffer`.
use crate::buffer::PixelBuffer;
use crate::text::TextRenderer;
use mesh_ui::style::{Color, Display};
use mesh_ui::tree::WidgetNode;

/// Walks a widget tree and paints each node into a pixel buffer.
pub struct Painter {
    text_renderer: TextRenderer,
}

#[derive(Clone, Copy)]
struct ClipRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Painter {
    pub fn new() -> Self {
        Self {
            text_renderer: TextRenderer::new(),
        }
    }

    /// Paint the entire widget tree into the buffer.
    pub fn paint(&self, root: &WidgetNode, buffer: &mut PixelBuffer, scale: f32) {
        let clip = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width as i32,
            height: buffer.height as i32,
        };
        self.paint_node(root, buffer, scale, 0.0, 0.0, clip);
    }

    fn paint_node(
        &self,
        node: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        offset_x: f32,
        offset_y: f32,
        clip: ClipRect,
    ) {
        let style = &node.computed_style;
        if style.display == Display::None {
            return;
        }

        let layout = &node.layout;
        let x = ((layout.x + offset_x) * scale).round() as i32;
        let y = ((layout.y + offset_y) * scale).round() as i32;
        let w = (layout.width * scale).round().max(0.0) as i32;
        let h = (layout.height * scale).round().max(0.0) as i32;
        let bounds = ClipRect {
            x,
            y,
            width: w,
            height: h,
        };
        let node_clip = intersect_clip(clip, bounds);

        if node_clip.width <= 0 || node_clip.height <= 0 {
            return;
        }

        if style.background_color.a > 0 {
            let radius = style.border_radius.top_left * scale;
            if radius > 0.5 {
                fill_rounded_rect_clipped(buffer, bounds, radius, style.background_color, node_clip);
            } else {
                fill_rect_clipped(buffer, bounds, style.background_color, node_clip);
            }
        }

        if style.border_width.top > 0.0 && style.border_color.a > 0 {
            let bw = (style.border_width.top * scale).max(1.0) as i32;
            fill_rect_clipped(
                buffer,
                ClipRect { x, y, width: w, height: bw },
                style.border_color,
                node_clip,
            );
            fill_rect_clipped(
                buffer,
                ClipRect {
                    x,
                    y: y + h.saturating_sub(bw),
                    width: w,
                    height: bw,
                },
                style.border_color,
                node_clip,
            );
            fill_rect_clipped(
                buffer,
                ClipRect { x, y, width: bw, height: h },
                style.border_color,
                node_clip,
            );
            fill_rect_clipped(
                buffer,
                ClipRect {
                    x: x + w.saturating_sub(bw),
                    y,
                    width: bw,
                    height: h,
                },
                style.border_color,
                node_clip,
            );
        }

        match node.tag.as_str() {
            "text" => self.paint_text_node(node, buffer, scale, x, y, node_clip),
            "input" => self.paint_input_node(node, buffer, scale, x, y, node_clip),
            "slider" => self.paint_slider_node(node, buffer, scale, x, y, w, h, node_clip),
            _ => {}
        }

        let child_offset_x = offset_x;
        let mut child_offset_y = offset_y;
        let mut child_clip = node_clip;

        if node.tag == "scroll" {
            let scroll_y = node
                .attributes
                .get("_mesh_scroll_y")
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or(0.0);
            child_offset_y -= scroll_y;
            child_clip = intersect_clip(clip, bounds);
        }

        for child in &node.children {
            self.paint_node(child, buffer, scale, child_offset_x, child_offset_y, child_clip);
        }
    }

    fn paint_text_node(
        &self,
        node: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        x: i32,
        y: i32,
        clip: ClipRect,
    ) {
        let style = &node.computed_style;
        let text = node
            .attributes
            .get("text")
            .map(|s| s.as_str())
            .or_else(|| node.attributes.get("content").map(|s| s.as_str()))
            .unwrap_or("");

        if text.is_empty() {
            return;
        }

        let tx = (x + (style.padding.left * scale) as i32).max(0) as u32;
        let ty = (y + (style.padding.top * scale) as i32).max(0) as u32;
        self.text_renderer.render_clipped(
            text,
            style.font_size * scale,
            style.color,
            buffer,
            tx,
            ty,
            clip_to_tuple(clip),
        );
    }

    fn paint_input_node(
        &self,
        node: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        x: i32,
        y: i32,
        clip: ClipRect,
    ) {
        let style = &node.computed_style;
        let value = node.attributes.get("value").cloned().unwrap_or_default();
        let placeholder = node
            .attributes
            .get("placeholder")
            .cloned()
            .unwrap_or_default();
        let focused = node
            .attributes
            .get("_mesh_focused")
            .is_some_and(|value| value == "true");
        let text = if value.is_empty() { &placeholder } else { &value };
        let text_color = if value.is_empty() {
            dim_color(style.color, 0.6)
        } else {
            style.color
        };

        let tx = (x + (style.padding.left * scale) as i32).max(0) as u32;
        let inner_height = ((node.layout.height - style.padding.vertical()) * scale).max(0.0) as i32;
        let glyph_height = (style.font_size * scale).max(8.0) as i32;
        let ty = (y
            + (style.padding.top * scale) as i32
            + ((inner_height - glyph_height) / 2).max(0))
            .max(0) as u32;

        self.text_renderer.render_clipped(
            text,
            style.font_size * scale,
            text_color,
            buffer,
            tx,
            ty,
            clip_to_tuple(clip),
        );

        if focused {
            let caret_x = tx
                + ((text.chars().count() as f32 * ((style.font_size * scale / 8.0).round().max(1.0) * 9.0))
                    as u32);
            fill_rect_clipped(
                buffer,
                ClipRect {
                    x: caret_x as i32,
                    y: ty as i32,
                    width: 2,
                    height: glyph_height,
                },
                style.color,
                clip,
            );
        }
    }

    fn paint_slider_node(
        &self,
        node: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        clip: ClipRect,
    ) {
        let style = &node.computed_style;
        let min = node
            .attributes
            .get("min")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let max = node
            .attributes
            .get("max")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(100.0);
        let value = node
            .attributes
            .get("value")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(50.0);
        let pct = if max > min {
            ((value - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let track_margin = (16.0 * scale).round() as i32;
        let track_height = (4.0 * scale).round().max(2.0) as i32;
        let track_x = x + track_margin;
        let track_y = y + (h / 2) - (track_height / 2);
        let track_w = (w - track_margin * 2).max(8);
        fill_rect_clipped(
            buffer,
            ClipRect {
                x: track_x,
                y: track_y,
                width: track_w,
                height: track_height,
            },
            dim_color(style.color, 0.35),
            clip,
        );

        let active_w = ((track_w as f32) * pct).round() as i32;
        fill_rect_clipped(
            buffer,
            ClipRect {
                x: track_x,
                y: track_y,
                width: active_w.max(0),
                height: track_height,
            },
            style.color,
            clip,
        );

        let thumb_radius = (8.0 * scale).round().max(5.0) as i32;
        let thumb_x = track_x + active_w - thumb_radius;
        let thumb_y = y + h / 2 - thumb_radius;
        fill_rounded_rect_clipped(
            buffer,
            ClipRect {
                x: thumb_x,
                y: thumb_y,
                width: thumb_radius * 2,
                height: thumb_radius * 2,
            },
            thumb_radius as f32,
            style.color,
            clip,
        );
    }
}

impl Default for Painter {
    fn default() -> Self {
        Self::new()
    }
}

fn intersect_clip(a: ClipRect, b: ClipRect) -> ClipRect {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.width).min(b.x + b.width);
    let y2 = (a.y + a.height).min(b.y + b.height);

    ClipRect {
        x: x1,
        y: y1,
        width: (x2 - x1).max(0),
        height: (y2 - y1).max(0),
    }
}

fn clip_to_tuple(clip: ClipRect) -> (u32, u32, u32, u32) {
    (
        clip.x.max(0) as u32,
        clip.y.max(0) as u32,
        clip.width.max(0) as u32,
        clip.height.max(0) as u32,
    )
}

fn fill_rect_clipped(buffer: &mut PixelBuffer, rect: ClipRect, color: Color, clip: ClipRect) {
    let clipped = intersect_clip(rect, clip);
    if clipped.width <= 0 || clipped.height <= 0 {
        return;
    }

    for py in clipped.y..clipped.y + clipped.height {
        for px in clipped.x..clipped.x + clipped.width {
            buffer.set_pixel(px as u32, py as u32, color);
        }
    }
}

fn fill_rounded_rect_clipped(
    buffer: &mut PixelBuffer,
    rect: ClipRect,
    radius: f32,
    color: Color,
    clip: ClipRect,
) {
    let clipped = intersect_clip(rect, clip);
    if clipped.width <= 0 || clipped.height <= 0 {
        return;
    }

    let r = radius.min(rect.width as f32 / 2.0).min(rect.height as f32 / 2.0);
    let ri = r.max(0.0) as i32;

    for py in clipped.y..clipped.y + clipped.height {
        for px in clipped.x..clipped.x + clipped.width {
            let lx = px - rect.x;
            let ly = py - rect.y;

            let in_corner = (lx < ri && ly < ri)
                || (lx >= rect.width - ri && ly < ri)
                || (lx < ri && ly >= rect.height - ri)
                || (lx >= rect.width - ri && ly >= rect.height - ri);

            if in_corner {
                let cx = if lx < ri {
                    rect.x + ri
                } else {
                    rect.x + rect.width - ri - 1
                } as f32;
                let cy = if ly < ri {
                    rect.y + ri
                } else {
                    rect.y + rect.height - ri - 1
                } as f32;
                let dx = px as f32 - cx;
                let dy = py as f32 - cy;
                if dx * dx + dy * dy <= r * r {
                    buffer.set_pixel(px as u32, py as u32, color);
                }
            } else {
                buffer.set_pixel(px as u32, py as u32, color);
            }
        }
    }
}

fn dim_color(color: Color, factor: f32) -> Color {
    Color {
        r: ((color.r as f32) * factor).round().clamp(0.0, 255.0) as u8,
        g: ((color.g as f32) * factor).round().clamp(0.0, 255.0) as u8,
        b: ((color.b as f32) * factor).round().clamp(0.0, 255.0) as u8,
        a: color.a,
    }
}
