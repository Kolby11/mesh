/// Paints a `WidgetNode` tree into a `PixelBuffer`.
use crate::buffer::PixelBuffer;
use crate::text::TextRenderer;
use mesh_ui::style::{Color, Display, Overflow, TextAlign, TextOverflow};
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
                fill_rounded_rect_clipped(
                    buffer,
                    bounds,
                    radius,
                    style.background_color,
                    node_clip,
                );
            } else {
                fill_rect_clipped(buffer, bounds, style.background_color, node_clip);
            }
        }

        if style.border_width.top > 0.0 && style.border_color.a > 0 {
            let bw = (style.border_width.top * scale).max(1.0) as i32;
            fill_rect_clipped(
                buffer,
                ClipRect {
                    x,
                    y,
                    width: w,
                    height: bw,
                },
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
                ClipRect {
                    x,
                    y,
                    width: bw,
                    height: h,
                },
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

        let scroll_x = node_attr_f32(node, "_mesh_scroll_x");
        let scroll_y = node_attr_f32(node, "_mesh_scroll_y");
        let child_offset_x = offset_x - scroll_x;
        let child_offset_y = offset_y - scroll_y;
        let child_clip = if node_clips_children(node) {
            intersect_clip(clip, bounds)
        } else {
            clip
        };

        for child in &node.children {
            self.paint_node(
                child,
                buffer,
                scale,
                child_offset_x,
                child_offset_y,
                child_clip,
            );
        }

        self.paint_scrollbars(node, buffer, scale, bounds, clip);
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
        let inner_width = ((node.layout.width - style.padding.horizontal()) * scale).max(0.0);

        let display_text: std::borrow::Cow<'_, str> =
            if style.text_overflow == TextOverflow::Ellipsis && inner_width > 0.0 {
                let (tw, _) = self.text_renderer.measure_styled(
                    text,
                    &style.font_family,
                    style.font_size * scale,
                    style.font_weight,
                    style.line_height,
                    None,
                );
                if tw > inner_width {
                    std::borrow::Cow::Owned(truncate_with_ellipsis(
                        &self.text_renderer,
                        text,
                        &style.font_family,
                        style.font_size * scale,
                        style.font_weight,
                        style.line_height,
                        inner_width,
                    ))
                } else {
                    std::borrow::Cow::Borrowed(text)
                }
            } else {
                std::borrow::Cow::Borrowed(text)
            };

        self.text_renderer.render_clipped(
            &display_text,
            &style.font_family,
            style.font_size * scale,
            style.font_weight,
            style.line_height,
            style.text_align,
            style.color,
            buffer,
            tx,
            ty,
            clip_to_tuple(clip),
            Some(inner_width),
        );
    }

    /// Paint a tooltip overlay at the given logical position.
    pub fn paint_tooltip(
        &self,
        text: &str,
        cursor_x: f32,
        cursor_y: f32,
        buffer: &mut PixelBuffer,
        scale: f32,
    ) {
        let font_size = 12.0 * scale;
        let pad_h = (8.0 * scale) as i32;
        let pad_v = (5.0 * scale) as i32;
        let max_text_w = 220.0 * scale;

        let (text_w, text_h) =
            self.text_renderer
                .measure_styled(text, "Inter", font_size, 400, 1.3, Some(max_text_w));

        let box_w =
            (text_w.ceil() as i32 + pad_h * 2).min((max_text_w + pad_h as f32 * 2.0) as i32);
        let box_h = (text_h.ceil() as i32 + pad_v * 2).max((font_size + pad_v as f32 * 2.0) as i32);

        let cx = ((cursor_x + 14.0) * scale) as i32;
        let cy = ((cursor_y + 18.0) * scale) as i32;
        let tx = cx.min(buffer.width as i32 - box_w - 6).max(4);
        let ty = cy.min(buffer.height as i32 - box_h - 6).max(4);

        let full_clip = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width as i32,
            height: buffer.height as i32,
        };

        let bg = Color::from_hex("#1c1822").unwrap_or(Color::BLACK);
        let border = Color::from_hex("#3d3648").unwrap_or(Color::WHITE);
        let text_color = Color::from_hex("#e2d9f0").unwrap_or(Color::WHITE);
        let radius = (6.0 * scale).max(3.0);

        fill_rounded_rect_clipped(
            buffer,
            ClipRect {
                x: tx - 1,
                y: ty - 1,
                width: box_w + 2,
                height: box_h + 2,
            },
            radius + 1.0,
            border,
            full_clip,
        );
        fill_rounded_rect_clipped(
            buffer,
            ClipRect {
                x: tx,
                y: ty,
                width: box_w,
                height: box_h,
            },
            radius,
            bg,
            full_clip,
        );
        self.text_renderer.render_clipped(
            text,
            "Inter",
            font_size,
            400,
            1.3,
            TextAlign::Left,
            text_color,
            buffer,
            (tx + pad_h) as u32,
            (ty + pad_v) as u32,
            (0, 0, buffer.width, buffer.height),
            Some(max_text_w),
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
        let text = if value.is_empty() {
            &placeholder
        } else {
            &value
        };
        let text_color = if value.is_empty() {
            dim_color(style.color, 0.6)
        } else {
            style.color
        };

        let tx = (x + (style.padding.left * scale) as i32).max(0) as u32;
        let inner_height =
            ((node.layout.height - style.padding.vertical()) * scale).max(0.0) as i32;
        let (_text_width, text_height) = self.text_renderer.measure_styled(
            text,
            &style.font_family,
            style.font_size * scale,
            style.font_weight,
            style.line_height,
            None,
        );
        let glyph_height = text_height.max((style.font_size * scale).max(8.0)) as i32;
        let ty =
            (y + (style.padding.top * scale) as i32 + ((inner_height - glyph_height) / 2).max(0))
                .max(0) as u32;

        self.text_renderer.render_clipped(
            text,
            &style.font_family,
            style.font_size * scale,
            style.font_weight,
            style.line_height,
            style.text_align,
            text_color,
            buffer,
            tx,
            ty,
            clip_to_tuple(clip),
            None,
        );

        if focused {
            let (text_width, _text_height) = self.text_renderer.measure_styled(
                text,
                &style.font_family,
                style.font_size * scale,
                style.font_weight,
                style.line_height,
                None,
            );
            let caret_x = tx + text_width.round() as u32;
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

    fn paint_scrollbars(
        &self,
        node: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        bounds: ClipRect,
        clip: ClipRect,
    ) {
        let max_x = node_attr_f32(node, "_mesh_scroll_max_x");
        let max_y = node_attr_f32(node, "_mesh_scroll_max_y");
        let scroll_x = node_attr_f32(node, "_mesh_scroll_x");
        let scroll_y = node_attr_f32(node, "_mesh_scroll_y");
        let content_width = node_attr_f32(node, "_mesh_content_width");
        let content_height = node_attr_f32(node, "_mesh_content_height");

        let show_vertical = node.computed_style.overflow_y.always_shows_scrollbar()
            || (node
                .computed_style
                .overflow_y
                .shows_scrollbar_when_overflowing()
                && max_y > f32::EPSILON);
        let show_horizontal = node.computed_style.overflow_x.always_shows_scrollbar()
            || (node
                .computed_style
                .overflow_x
                .shows_scrollbar_when_overflowing()
                && max_x > f32::EPSILON);

        if !show_vertical && !show_horizontal {
            return;
        }

        let inset = (4.0 * scale).round().max(2.0) as i32;
        let thickness = (6.0 * scale).round().max(4.0) as i32;
        let radius = (thickness as f32 / 2.0).max(2.0);
        let track_color = Color::from_hex("#24202b").unwrap_or(Color::BLACK);
        let thumb_color = Color::from_hex("#8f879c").unwrap_or(Color::WHITE);

        if show_vertical {
            let viewport_height = bounds.height.max(1) as f32;
            let track_height = (bounds.height
                - inset * 2
                - if show_horizontal {
                    thickness + inset
                } else {
                    0
                })
            .max(thickness);
            let track = ClipRect {
                x: bounds.x + bounds.width - inset - thickness,
                y: bounds.y + inset,
                width: thickness,
                height: track_height,
            };
            fill_rounded_rect_clipped(
                buffer,
                track,
                radius,
                track_color,
                intersect_clip(clip, bounds),
            );

            let thumb_height = if content_height <= 0.0 {
                track_height
            } else {
                ((viewport_height / content_height.max(viewport_height)) * track_height as f32)
                    .round()
                    .clamp((18.0 * scale).max(10.0), track_height as f32) as i32
            };
            let thumb_range = (track_height - thumb_height).max(0) as f32;
            let thumb_y = track.y
                + if max_y <= f32::EPSILON {
                    0
                } else {
                    ((scroll_y / max_y.max(1.0)) * thumb_range).round() as i32
                };
            fill_rounded_rect_clipped(
                buffer,
                ClipRect {
                    x: track.x,
                    y: thumb_y,
                    width: thickness,
                    height: thumb_height.max(thickness),
                },
                radius,
                thumb_color,
                intersect_clip(clip, bounds),
            );
        }

        if show_horizontal {
            let viewport_width = bounds.width.max(1) as f32;
            let track_width =
                (bounds.width - inset * 2 - if show_vertical { thickness + inset } else { 0 })
                    .max(thickness);
            let track = ClipRect {
                x: bounds.x + inset,
                y: bounds.y + bounds.height - inset - thickness,
                width: track_width,
                height: thickness,
            };
            fill_rounded_rect_clipped(
                buffer,
                track,
                radius,
                track_color,
                intersect_clip(clip, bounds),
            );

            let thumb_width = if content_width <= 0.0 {
                track_width
            } else {
                ((viewport_width / content_width.max(viewport_width)) * track_width as f32)
                    .round()
                    .clamp((18.0 * scale).max(10.0), track_width as f32) as i32
            };
            let thumb_range = (track_width - thumb_width).max(0) as f32;
            let thumb_x = track.x
                + if max_x <= f32::EPSILON {
                    0
                } else {
                    ((scroll_x / max_x.max(1.0)) * thumb_range).round() as i32
                };
            fill_rounded_rect_clipped(
                buffer,
                ClipRect {
                    x: thumb_x,
                    y: track.y,
                    width: thumb_width.max(thickness),
                    height: thickness,
                },
                radius,
                thumb_color,
                intersect_clip(clip, bounds),
            );
        }
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

    let r = radius
        .min(rect.width as f32 / 2.0)
        .min(rect.height as f32 / 2.0);
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

fn node_attr_f32(node: &WidgetNode, key: &str) -> f32 {
    node.attributes
        .get(key)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0)
}

fn node_clips_children(node: &WidgetNode) -> bool {
    node.computed_style.overflow_x != Overflow::Visible
        || node.computed_style.overflow_y != Overflow::Visible
}

fn truncate_with_ellipsis(
    renderer: &crate::text::TextRenderer,
    text: &str,
    font_family: &str,
    font_size: f32,
    font_weight: u16,
    line_height: f32,
    max_width: f32,
) -> String {
    const ELLIPSIS: &str = "…";
    let (ellipsis_w, _) = renderer.measure_styled(
        ELLIPSIS,
        font_family,
        font_size,
        font_weight,
        line_height,
        None,
    );
    let target = (max_width - ellipsis_w).max(0.0);

    let chars: Vec<char> = text.chars().collect();
    for len in (0..=chars.len()).rev() {
        let s: String = chars[..len].iter().collect();
        let (w, _) =
            renderer.measure_styled(&s, font_family, font_size, font_weight, line_height, None);
        if w <= target {
            return format!("{s}{ELLIPSIS}");
        }
    }
    ELLIPSIS.to_string()
}
