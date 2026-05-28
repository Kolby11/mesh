use crate::display_list::{
    DisplayIconPaint, DisplayInputPaint, DisplayPaintNode, DisplaySliderPaint,
};

use super::*;
use std::borrow::Cow;

fn scrollbar_thumb_extent(raw_extent: f32, track_extent: i32, scale: f32) -> i32 {
    let track_extent = track_extent.max(1);
    let min_extent = ((18.0 * scale).max(10.0) as i32).min(track_extent);
    (raw_extent.round() as i32).clamp(min_extent, track_extent)
}

fn push_slider_commands(
    commands: &mut Vec<PainterCommand>,
    is_vertical: bool,
    pct: f32,
    style_color: Color,
    scale: f32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    clip: ClipRect,
) {
    let track_margin = (16.0 * scale).round() as i32;
    let track_thickness = (4.0 * scale).round().max(2.0) as i32;
    let thumb_radius = (8.0 * scale).round().max(5.0) as i32;

    if is_vertical {
        let track_x = x + (w / 2) - (track_thickness / 2);
        let track_y = y + track_margin;
        let track_h = (h - track_margin * 2).max(8);
        commands.push(PainterCommand::DrawRect {
            rect: ClipRect {
                x: track_x,
                y: track_y,
                width: track_thickness,
                height: track_h,
            },
            paint: PainterPaint::fill(dim_color(style_color, 0.35)),
            clip,
        });

        let active_h = ((track_h as f32) * (1.0 - pct)).round() as i32;
        commands.push(PainterCommand::DrawRect {
            rect: ClipRect {
                x: track_x,
                y: track_y,
                width: track_thickness,
                height: active_h.max(0),
            },
            paint: PainterPaint::fill(style_color),
            clip,
        });

        let thumb_y = track_y + active_h - thumb_radius;
        let thumb_x = x + w / 2 - thumb_radius;
        commands.push(PainterCommand::DrawRoundedRect {
            rect: ClipRect {
                x: thumb_x,
                y: thumb_y,
                width: thumb_radius * 2,
                height: thumb_radius * 2,
            },
            radius: thumb_radius as f32,
            paint: PainterPaint::fill(style_color),
            clip,
        });
    } else {
        let track_x = x + track_margin;
        let track_y = y + (h / 2) - (track_thickness / 2);
        let track_w = (w - track_margin * 2).max(8);
        commands.push(PainterCommand::DrawRect {
            rect: ClipRect {
                x: track_x,
                y: track_y,
                width: track_w,
                height: track_thickness,
            },
            paint: PainterPaint::fill(dim_color(style_color, 0.35)),
            clip,
        });

        let active_w = ((track_w as f32) * pct).round() as i32;
        commands.push(PainterCommand::DrawRect {
            rect: ClipRect {
                x: track_x,
                y: track_y,
                width: active_w.max(0),
                height: track_thickness,
            },
            paint: PainterPaint::fill(style_color),
            clip,
        });

        let thumb_x = track_x + active_w - thumb_radius;
        let thumb_y = y + h / 2 - thumb_radius;
        commands.push(PainterCommand::DrawRoundedRect {
            rect: ClipRect {
                x: thumb_x,
                y: thumb_y,
                width: thumb_radius * 2,
                height: thumb_radius * 2,
            },
            radius: thumb_radius as f32,
            paint: PainterPaint::fill(style_color),
            clip,
        });
    }
}

impl FrontendRenderEngine {
    pub(super) fn render_input_node(
        &self,
        node: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        x: i32,
        y: i32,
        clip: ClipRect,
    ) {
        let style = &node.computed_style;
        let value = node
            .attributes
            .get("value")
            .map(String::as_str)
            .unwrap_or("");
        let input_type = node
            .attributes
            .get("type")
            .map(|value| value.as_str())
            .unwrap_or("text");
        let placeholder = node
            .attributes
            .get("placeholder")
            .map(String::as_str)
            .unwrap_or("");
        let focused = node
            .attributes
            .get("_mesh_focused")
            .is_some_and(|value| value == "true");
        let display_value: Cow<'_, str> = if input_type == "password" && !value.is_empty() {
            Cow::Owned("*".repeat(value.chars().count()))
        } else {
            Cow::Borrowed(value)
        };
        let text = if display_value.is_empty() {
            placeholder
        } else {
            display_value.as_ref()
        };
        let style_color = opacity_color(style.color, style.opacity);
        let text_color = if display_value.is_empty() {
            dim_color(style_color, 0.6)
        } else {
            style_color
        };

        let tx = (x + (style.padding.left * scale) as i32).max(0) as u32;
        let inner_height =
            ((node.layout.height - style.padding.vertical()) * scale).max(0.0) as i32;
        let (text_width, text_height) = self.text_renderer.measure_styled(
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
            let caret_x = tx + text_width.round() as u32;
            self.fill_rect_clipped(
                buffer,
                ClipRect {
                    x: caret_x as i32,
                    y: ty as i32,
                    width: 2,
                    height: glyph_height,
                },
                style_color,
                clip,
            );
        }
    }

    pub(super) fn render_display_input_node(
        &self,
        node: &DisplayPaintNode,
        input: &DisplayInputPaint,
        session: &mut PixelCanvasSession<'_>,
        scale: f32,
        x: i32,
        y: i32,
        clip: ClipRect,
    ) {
        let style = &node.style;
        let display_value: Cow<'_, str> = if input.mask_text && !input.value.is_empty() {
            Cow::Owned("*".repeat(input.value.chars().count()))
        } else {
            Cow::Borrowed(input.value.as_str())
        };
        let text = if display_value.is_empty() {
            input.placeholder.as_str()
        } else {
            display_value.as_ref()
        };
        let text_color = if display_value.is_empty() {
            dim_color(style.color, 0.6)
        } else {
            style.color
        };

        let tx = (x + (style.padding.left * scale) as i32).max(0) as u32;
        let inner_height =
            ((node.layout.height - style.padding.vertical()) * scale).max(0.0) as i32;
        let (text_width, text_height) = self.text_renderer.measure_styled(
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

        session.with_canvas(|canvas| {
            self.text_renderer.render_clipped_on_canvas(
                text,
                &style.font_family,
                style.font_size * scale,
                style.font_weight,
                style.line_height,
                style.text_align,
                text_color,
                canvas,
                tx,
                ty,
                clip_to_tuple(clip),
                None,
            );
        });

        if input.focused {
            let caret_x = tx + text_width.round() as u32;
            let caret_rect = ClipRect {
                x: caret_x as i32,
                y: ty as i32,
                width: 2,
                height: glyph_height,
            };
            self.execute_painter_commands_in_session(
                session,
                &[PainterCommand::DrawRect {
                    rect: caret_rect,
                    paint: PainterPaint::fill(style.color),
                    clip,
                }],
            );
        }
    }

    pub(super) fn render_slider_node(
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
        let style_color = opacity_color(style.color, style.opacity);
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

        let is_vertical = node
            .attributes
            .get("orient")
            .map(|value| value == "vertical")
            .unwrap_or(false);

        let mut commands = Vec::with_capacity(3);
        push_slider_commands(
            &mut commands,
            is_vertical,
            pct,
            style_color,
            scale,
            x,
            y,
            w,
            h,
            clip,
        );
        self.execute_painter_commands(buffer, &commands);
    }

    pub(super) fn render_display_slider_node(
        &self,
        node: &DisplayPaintNode,
        slider: &DisplaySliderPaint,
        session: &mut PixelCanvasSession<'_>,
        commands: &mut Vec<PainterCommand>,
        scale: f32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        clip: ClipRect,
    ) {
        let pct = if slider.max > slider.min {
            ((slider.value - slider.min) / (slider.max - slider.min)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        commands.clear();
        push_slider_commands(
            commands,
            slider.vertical,
            pct,
            node.style.color,
            scale,
            x,
            y,
            w,
            h,
            clip,
        );
        self.execute_painter_commands_in_session(session, commands);
        commands.clear();
    }

    pub(super) fn render_icon_node(
        &self,
        node: &WidgetNode,
        buffer: &mut PixelBuffer,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: Color,
        module_id: Option<&str>,
    ) {
        let src = node.attributes.get("src").map(|value| value.as_str());
        let name = node.attributes.get("name").map(|value| value.as_str());
        let size = node
            .attributes
            .get("size")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(w.max(h) as u32);

        let style = &node.computed_style;
        let axes = super::super::glyph::GlyphAxes {
            fill: style.icon_fill,
            weight: style.icon_weight,
            grade: style.icon_grade,
            optical_size: style.icon_optical_size,
        };

        if let Some(src) = src {
            icon::draw_icon_from_path(buffer, std::path::Path::new(src), x, y, w, h, color);
        } else if let Some(name) = name {
            match module_id {
                Some(id) => icon::draw_named_icon_for_module(
                    buffer, id, name, size, x, y, w, h, color, axes,
                ),
                None => icon::draw_named_icon(buffer, name, size, x, y, w, h, color),
            }
        }
    }

    pub(super) fn render_display_icon_node(
        &self,
        node: &DisplayPaintNode,
        icon_paint: &DisplayIconPaint,
        session: &mut PixelCanvasSession<'_>,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        module_id: Option<&str>,
    ) {
        let size = icon_paint.size.unwrap_or(w.max(h) as u32);
        let style = &node.style;
        let axes = super::super::glyph::GlyphAxes {
            fill: style.icon_fill,
            weight: style.icon_weight,
            grade: style.icon_grade,
            optical_size: style.icon_optical_size,
        };

        if let Some(src) = &icon_paint.src {
            icon::draw_icon_from_path_in_session(
                session,
                std::path::Path::new(src),
                x,
                y,
                w,
                h,
                style.color,
            );
        } else if let Some(name) = &icon_paint.name {
            match module_id {
                Some(id) => icon::draw_named_icon_for_module_in_session(
                    session,
                    id,
                    name,
                    size,
                    x,
                    y,
                    w,
                    h,
                    style.color,
                    axes,
                ),
                None => icon::draw_named_icon_in_session(
                    session,
                    name,
                    size,
                    x,
                    y,
                    w,
                    h,
                    style.color,
                ),
            }
        }
    }

    pub(super) fn render_scrollbars(
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
            self.fill_rounded_rect_clipped(
                buffer,
                track,
                radius,
                track_color,
                intersect_clip(clip, bounds),
            );

            let thumb_height = if content_height <= 0.0 {
                track_height
            } else {
                scrollbar_thumb_extent(
                    (viewport_height / content_height.max(viewport_height)) * track_height as f32,
                    track_height,
                    scale,
                )
            };
            let thumb_range = (track_height - thumb_height).max(0) as f32;
            let thumb_y = track.y
                + if max_y <= f32::EPSILON {
                    0
                } else {
                    ((scroll_y / max_y.max(1.0)) * thumb_range).round() as i32
                };
            self.fill_rounded_rect_clipped(
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
            self.fill_rounded_rect_clipped(
                buffer,
                track,
                radius,
                track_color,
                intersect_clip(clip, bounds),
            );

            let thumb_width = if content_width <= 0.0 {
                track_width
            } else {
                scrollbar_thumb_extent(
                    (viewport_width / content_width.max(viewport_width)) * track_width as f32,
                    track_width,
                    scale,
                )
            };
            let thumb_range = (track_width - thumb_width).max(0) as f32;
            let thumb_x = track.x
                + if max_x <= f32::EPSILON {
                    0
                } else {
                    ((scroll_x / max_x.max(1.0)) * thumb_range).round() as i32
                };
            self.fill_rounded_rect_clipped(
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

    pub(super) fn render_display_scrollbars(
        &self,
        node: &DisplayPaintNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        bounds: ClipRect,
        clip: ClipRect,
    ) {
        let show_vertical = node.style.overflow_y.always_shows_scrollbar()
            || (node.style.overflow_y.shows_scrollbar_when_overflowing()
                && node.scrollbars.max_y > f32::EPSILON);
        let show_horizontal = node.style.overflow_x.always_shows_scrollbar()
            || (node.style.overflow_x.shows_scrollbar_when_overflowing()
                && node.scrollbars.max_x > f32::EPSILON);

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
            self.fill_rounded_rect_clipped(
                buffer,
                track,
                radius,
                track_color,
                intersect_clip(clip, bounds),
            );

            let thumb_height = if node.scrollbars.content_height <= 0.0 {
                track_height
            } else {
                scrollbar_thumb_extent(
                    (viewport_height / node.scrollbars.content_height.max(viewport_height))
                        * track_height as f32,
                    track_height,
                    scale,
                )
            };
            let thumb_range = (track_height - thumb_height).max(0) as f32;
            let thumb_y = track.y
                + if node.scrollbars.max_y <= f32::EPSILON {
                    0
                } else {
                    ((node.scrollbars.scroll_y / node.scrollbars.max_y.max(1.0)) * thumb_range)
                        .round() as i32
                };
            self.fill_rounded_rect_clipped(
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
            self.fill_rounded_rect_clipped(
                buffer,
                track,
                radius,
                track_color,
                intersect_clip(clip, bounds),
            );

            let thumb_width = if node.scrollbars.content_width <= 0.0 {
                track_width
            } else {
                scrollbar_thumb_extent(
                    (viewport_width / node.scrollbars.content_width.max(viewport_width))
                        * track_width as f32,
                    track_width,
                    scale,
                )
            };
            let thumb_range = (track_width - thumb_width).max(0) as f32;
            let thumb_x = track.x
                + if node.scrollbars.max_x <= f32::EPSILON {
                    0
                } else {
                    ((node.scrollbars.scroll_x / node.scrollbars.max_x.max(1.0)) * thumb_range)
                        .round() as i32
                };
            self.fill_rounded_rect_clipped(
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
