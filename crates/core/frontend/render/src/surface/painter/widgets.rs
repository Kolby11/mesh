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

/// Scrollbars inherit the scroll element's resolved foreground color, rather
/// than carrying a second, hard-coded palette in the renderer. This keeps
/// theme tokens and module CSS authoritative for both the thumb and track.
fn scrollbar_colors(color: Color) -> (Color, Color) {
    let with_alpha = |factor: f32| Color {
        a: ((color.a as f32) * factor).round().clamp(0.0, 255.0) as u8,
        ..color
    };
    (with_alpha(0.20), with_alpha(0.64))
}

impl FrontendRenderEngine {
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
            Cow::Borrowed(input.value.as_ref())
        };
        let text = if display_value.is_empty() {
            input.placeholder.as_ref()
        } else {
            display_value.as_ref()
        };
        let text_color = if display_value.is_empty() {
            dim_color(style.color, 0.6)
        } else {
            style.color
        };
        let display_text = display_value.as_ref();
        let preedit = (!input.mask_text)
            .then_some(input.preedit.as_ref())
            .flatten()
            .filter(|preedit| {
                let valid_boundary =
                    |offset| offset <= display_text.len() && display_text.is_char_boundary(offset);
                preedit.start < preedit.end
                    && preedit.end <= display_text.len()
                    && valid_boundary(preedit.start)
                    && valid_boundary(preedit.end)
                    && preedit.cursor_begin >= preedit.start
                    && preedit.cursor_begin <= preedit.end
                    && preedit.cursor_end >= preedit.start
                    && preedit.cursor_end <= preedit.end
                    && valid_boundary(preedit.cursor_begin)
                    && valid_boundary(preedit.cursor_end)
            });

        let tx = (x + (style.padding.left * scale) as i32).max(0) as u32;
        let inner_height =
            ((node.paint_height() - style.padding.vertical()) * scale).max(0.0) as i32;
        let (text_width, text_height) = self.text_renderer.measure_styled_with_font_style(
            text,
            &style.font_family,
            style.font_size * scale,
            style.font_weight,
            style.font_style,
            style.line_height,
            None,
        );
        let glyph_height = text_height.max((style.font_size * scale).max(8.0)) as i32;
        let ty =
            (y + (style.padding.top * scale) as i32 + ((inner_height - glyph_height) / 2).max(0))
                .max(0) as u32;

        session.with_canvas(|canvas| {
            self.text_renderer.render_clipped_on_canvas_with_font_style(
                text,
                &style.font_family,
                style.font_size * scale,
                style.font_weight,
                style.font_style,
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
            if let Some(preedit) = preedit {
                let (prefix_width, _) = self.text_renderer.measure_styled_with_font_style(
                    &display_text[..preedit.start],
                    &style.font_family,
                    style.font_size * scale,
                    style.font_weight,
                    style.font_style,
                    style.line_height,
                    None,
                );
                let (preedit_width, _) = self.text_renderer.measure_styled_with_font_style(
                    &display_text[preedit.start..preedit.end],
                    &style.font_family,
                    style.font_size * scale,
                    style.font_weight,
                    style.font_style,
                    style.line_height,
                    None,
                );
                let underline_rect = ClipRect {
                    x: tx as i32 + prefix_width.round() as i32,
                    y: ty as i32 + glyph_height.saturating_sub(2),
                    width: preedit_width.round().max(1.0) as i32,
                    height: scale.round().max(1.0) as i32,
                };
                self.execute_painter_commands_in_session(
                    session,
                    &[PainterCommand::DrawRect {
                        rect: underline_rect,
                        paint: PainterPaint::fill(style.color),
                        clip,
                    }],
                );
            }

            let caret_width = preedit
                .map(|preedit| {
                    self.text_renderer
                        .measure_styled_with_font_style(
                            &display_text[..preedit.cursor_end],
                            &style.font_family,
                            style.font_size * scale,
                            style.font_weight,
                            style.font_style,
                            style.line_height,
                            None,
                        )
                        .0
                })
                .unwrap_or(text_width);
            let caret_x = tx + caret_width.round() as u32;
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

    pub(super) fn render_display_slider_node_in_session(
        &self,
        node: &DisplayPaintNode,
        slider: &DisplaySliderPaint,
        session: &mut PixelCanvasSession<'_>,
        scale: f32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        clip: ClipRect,
    ) {
        let style = &node.style;
        let pct = if slider.max > slider.min {
            ((slider.value - slider.min) / (slider.max - slider.min)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let track_margin = (16.0 * scale).round() as i32;
        let track_thickness = (4.0 * scale).round().max(2.0) as i32;
        let thumb_radius = (8.0 * scale).round().max(5.0) as i32;

        if slider.vertical {
            let track_x = x + (w / 2) - (track_thickness / 2);
            let track_y = y + track_margin;
            let track_h = (h - track_margin * 2).max(8);

            self.fill_rect_clipped_in_session(
                session,
                ClipRect {
                    x: track_x,
                    y: track_y,
                    width: track_thickness,
                    height: track_h,
                },
                dim_color(style.color, 0.35),
                clip,
            );

            let active_h = ((track_h as f32) * (1.0 - pct)).round() as i32;
            self.fill_rect_clipped_in_session(
                session,
                ClipRect {
                    x: track_x,
                    y: track_y,
                    width: track_thickness,
                    height: active_h.max(0),
                },
                style.color,
                clip,
            );

            let thumb_y = track_y + active_h - thumb_radius;
            let thumb_x = x + w / 2 - thumb_radius;
            self.fill_rounded_rect_clipped_in_session(
                session,
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
        } else {
            let track_x = x + track_margin;
            let track_y = y + (h / 2) - (track_thickness / 2);
            let track_w = (w - track_margin * 2).max(8);
            self.fill_rect_clipped_in_session(
                session,
                ClipRect {
                    x: track_x,
                    y: track_y,
                    width: track_w,
                    height: track_thickness,
                },
                dim_color(style.color, 0.35),
                clip,
            );

            let active_w = ((track_w as f32) * pct).round() as i32;
            self.fill_rect_clipped_in_session(
                session,
                ClipRect {
                    x: track_x,
                    y: track_y,
                    width: active_w.max(0),
                    height: track_thickness,
                },
                style.color,
                clip,
            );

            let thumb_x = track_x + active_w - thumb_radius;
            let thumb_y = y + h / 2 - thumb_radius;
            self.fill_rounded_rect_clipped_in_session(
                session,
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
                std::path::Path::new(src.as_ref()),
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
                    axes,
                ),
            }
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
        let commands = scrollbar_commands(node, scale, bounds, clip);
        self.execute_painter_commands(buffer, &commands);
    }

    pub(super) fn render_display_scrollbars_in_session(
        &self,
        node: &DisplayPaintNode,
        session: &mut PixelCanvasSession<'_>,
        scale: f32,
        bounds: ClipRect,
        clip: ClipRect,
    ) {
        let commands = scrollbar_commands(node, scale, bounds, clip);
        self.execute_painter_commands_in_session(session, &commands);
    }
}

fn scrollbar_commands(
    node: &DisplayPaintNode,
    scale: f32,
    bounds: ClipRect,
    clip: ClipRect,
) -> Vec<PainterCommand> {
    let show_vertical = node.style.overflow_y.always_shows_scrollbar()
        || (node.style.overflow_y.shows_scrollbar_when_overflowing()
            && node.scrollbars.max_y > f32::EPSILON);
    let show_horizontal = node.style.overflow_x.always_shows_scrollbar()
        || (node.style.overflow_x.shows_scrollbar_when_overflowing()
            && node.scrollbars.max_x > f32::EPSILON);

    if !show_vertical && !show_horizontal {
        return Vec::new();
    }

    let clip = intersect_clip(clip, bounds);
    let inset = (4.0 * scale).round().max(2.0) as i32;
    let thickness = (6.0 * scale).round().max(4.0) as i32;
    let radius = (thickness as f32 / 2.0).max(2.0);
    let (track_color, thumb_color) = scrollbar_colors(node.style.color);
    let mut commands = Vec::with_capacity(4);

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
        commands.push(PainterCommand::DrawRoundedRect {
            rect: track,
            radii: Corners::all(radius),
            paint: PainterPaint::fill(track_color),
            clip,
        });

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
                ((node.scrollbars.scroll_y / node.scrollbars.max_y.max(1.0)) * thumb_range).round()
                    as i32
            };
        commands.push(PainterCommand::DrawRoundedRect {
            rect: ClipRect {
                x: track.x,
                y: thumb_y,
                width: thickness,
                height: thumb_height.max(thickness),
            },
            radii: Corners::all(radius),
            paint: PainterPaint::fill(thumb_color),
            clip,
        });
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
        commands.push(PainterCommand::DrawRoundedRect {
            rect: track,
            radii: Corners::all(radius),
            paint: PainterPaint::fill(track_color),
            clip,
        });

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
                ((node.scrollbars.scroll_x / node.scrollbars.max_x.max(1.0)) * thumb_range).round()
                    as i32
            };
        commands.push(PainterCommand::DrawRoundedRect {
            rect: ClipRect {
                x: thumb_x,
                y: track.y,
                width: thumb_width.max(thickness),
                height: thickness,
            },
            radii: Corners::all(radius),
            paint: PainterPaint::fill(thumb_color),
            clip,
        });
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollbar_colors_preserve_the_resolved_theme_color() {
        let source = Color {
            r: 0xc0,
            g: 0xff,
            b: 0xee,
            a: 255,
        };
        let (track, thumb) = scrollbar_colors(source);

        assert_eq!((track.r, track.g, track.b, track.a), (0xc0, 0xff, 0xee, 51));
        assert_eq!(
            (thumb.r, thumb.g, thumb.b, thumb.a),
            (0xc0, 0xff, 0xee, 163)
        );
    }
}
