use super::text::display_text_style;
use crate::display_list::{DisplayIconPaint, DisplayInputPaint, DisplayPaintNode};

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

fn input_display_offset(value: &str, masked: bool, offset: usize) -> Option<usize> {
    if offset > value.len() || !value.is_char_boundary(offset) {
        return None;
    }
    Some(if masked {
        value[..offset].chars().count()
    } else {
        offset
    })
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
        let text_style = display_text_style(style, scale);
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
            style
                .paint_parts
                .get("placeholder")
                .map_or_else(|| dim_color(style.color, 0.6), |part| part.color)
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
        let (text_width, text_height) = self
            .text_renderer
            .measure_text_style(text, text_style, None);
        let glyph_height = text_height.max((style.font_size * scale).max(8.0)) as i32;
        let ty =
            (y + (style.padding.top * scale) as i32 + ((inner_height - glyph_height) / 2).max(0))
                .max(0) as u32;

        let selected = input.selection.and_then(|(start, end)| {
            let start = input_display_offset(&input.value, input.mask_text, start)?;
            let end = input_display_offset(&input.value, input.mask_text, end)?;
            (start != end).then_some((start.min(end), start.max(end)))
        });
        let mut selection_rect = None;
        if input.focused
            && let Some((start, end)) = selected
        {
            let prefix = self
                .text_renderer
                .measure_text_style(&display_text[..start], text_style, None)
                .0;
            let extent = self
                .text_renderer
                .measure_text_style(&display_text[start..end], text_style, None)
                .0;
            let selection_color = style
                .paint_parts
                .get("selection")
                .map_or(dim_color(style.color, 0.3), |part| part.background_color);
            let rect = ClipRect {
                x: tx as i32 + prefix.round() as i32,
                y: ty as i32,
                width: extent.ceil() as i32,
                height: glyph_height,
            };
            selection_rect = Some(rect);
            self.execute_painter_commands_in_session(
                session,
                &[PainterCommand::DrawRect {
                    rect,
                    paint: PainterPaint::fill(selection_color),
                    clip,
                }],
            );
        }

        session.with_canvas(|canvas| {
            self.text_renderer.render_clipped_on_canvas_with_text_style(
                text,
                text_style,
                style.text_align,
                text_color,
                canvas,
                tx,
                ty,
                clip_to_tuple(clip),
                None,
            );
        });

        if let Some(rect) = selection_rect {
            let selected_color = style
                .paint_parts
                .get("selection")
                .map_or(style.color, |part| part.color);
            session.with_canvas(|canvas| {
                self.text_renderer.render_clipped_on_canvas_with_text_style(
                    text,
                    text_style,
                    style.text_align,
                    selected_color,
                    canvas,
                    tx,
                    ty,
                    clip_to_tuple(intersect_clip(clip, rect)),
                    None,
                );
            });
        }

        if input.focused {
            if let Some(preedit) = preedit {
                let (prefix_width, _) = self.text_renderer.measure_text_style(
                    &display_text[..preedit.start],
                    text_style,
                    None,
                );
                let (preedit_width, _) = self.text_renderer.measure_text_style(
                    &display_text[preedit.start..preedit.end],
                    text_style,
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

            let cursor = input
                .cursor
                .and_then(|cursor| input_display_offset(&input.value, input.mask_text, cursor));
            let caret_width = preedit
                .map(|preedit| {
                    self.text_renderer
                        .measure_text_style(&display_text[..preedit.cursor_end], text_style, None)
                        .0
                })
                .unwrap_or_else(|| {
                    cursor
                        .filter(|cursor| {
                            *cursor <= display_text.len() && display_text.is_char_boundary(*cursor)
                        })
                        .map_or(
                            if display_text.is_empty() {
                                0.0
                            } else {
                                text_width
                            },
                            |cursor| {
                                self.text_renderer
                                    .measure_text_style(&display_text[..cursor], text_style, None)
                                    .0
                            },
                        )
                });
            let caret_x = tx + caret_width.round() as u32;
            let caret_rect = ClipRect {
                x: caret_x as i32,
                y: ty as i32,
                width: style
                    .paint_parts
                    .get("caret")
                    .and_then(|part| match part.width {
                        mesh_core_elements::Dimension::Px(width) => {
                            Some((width * scale).round().max(1.0) as i32)
                        }
                        _ => None,
                    })
                    .unwrap_or(2),
                height: glyph_height,
            };
            self.execute_painter_commands_in_session(
                session,
                &[PainterCommand::DrawRect {
                    rect: caret_rect,
                    paint: PainterPaint::fill(
                        style
                            .paint_parts
                            .get("caret")
                            .map_or(style.color, |part| part.color),
                    ),
                    clip,
                }],
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
        clip: ClipRect,
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

        let save = session.with_canvas(|canvas| {
            let save = canvas.save();
            canvas.clip_rect(
                skia_safe::Rect::from_xywh(
                    clip.x as f32,
                    clip.y as f32,
                    clip.width as f32,
                    clip.height as f32,
                ),
                None,
                false,
            );
            save
        });
        if let Some(src) = &icon_paint.src {
            icon::draw_icon_from_path_in_session(
                session,
                std::path::Path::new(src.as_ref()),
                x,
                y,
                w,
                h,
                style.color,
                icon_paint.multicolor,
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
        if let Some(save) = save {
            session.with_canvas(|canvas| {
                canvas.restore_to_count(save);
            });
        }
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
    let inset = (4.0 * scale * node.style.ui_scale).round().max(2.0) as i32;
    let thickness = (6.0 * scale * node.style.ui_scale).round().max(4.0) as i32;
    let radius = (thickness as f32 / 2.0).max(2.0);
    let (default_track, default_thumb) = scrollbar_colors(node.style.color);
    let track_style = node.style.paint_parts.get("track");
    let thumb_style = node.style.paint_parts.get("thumb");
    let track_color = track_style.map_or(default_track, |part| part.background_color);
    let thumb_color = thumb_style.map_or(default_thumb, |part| part.background_color);

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
        push_scroll_part_commands(
            &mut commands,
            track_style,
            track,
            radius,
            track_color,
            scale,
            clip,
        );

        let thumb_height = if node.scrollbars.content_height <= 0.0 {
            track_height
        } else {
            scrollbar_thumb_extent(
                (viewport_height / node.scrollbars.content_height.max(viewport_height))
                    * track_height as f32,
                track_height,
                scale * node.style.ui_scale,
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
        push_scroll_part_commands(
            &mut commands,
            thumb_style,
            ClipRect {
                x: track.x,
                y: thumb_y,
                width: thickness,
                height: thumb_height.max(thickness),
            },
            radius,
            thumb_color,
            scale,
            clip,
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
        push_scroll_part_commands(
            &mut commands,
            track_style,
            track,
            radius,
            track_color,
            scale,
            clip,
        );

        let thumb_width = if node.scrollbars.content_width <= 0.0 {
            track_width
        } else {
            scrollbar_thumb_extent(
                (viewport_width / node.scrollbars.content_width.max(viewport_width))
                    * track_width as f32,
                track_width,
                scale * node.style.ui_scale,
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
        push_scroll_part_commands(
            &mut commands,
            thumb_style,
            ClipRect {
                x: thumb_x,
                y: track.y,
                width: thumb_width.max(thickness),
                height: thickness,
            },
            radius,
            thumb_color,
            scale,
            clip,
        );
    }
    commands
}

fn push_scroll_part_commands(
    commands: &mut Vec<PainterCommand>,
    style: Option<&mesh_core_elements::ComputedStyle>,
    rect: ClipRect,
    default_radius: f32,
    color: Color,
    scale: f32,
    clip: ClipRect,
) {
    let radii = style.map_or(Corners::all(default_radius), |style| Corners {
        top_left: style.border_radius.top_left * scale,
        top_right: style.border_radius.top_right * scale,
        bottom_left: style.border_radius.bottom_left * scale,
        bottom_right: style.border_radius.bottom_right * scale,
    });
    commands.push(PainterCommand::DrawRoundedRect {
        rect,
        radii,
        paint: PainterPaint::fill(color),
        clip,
    });
    if let Some(style) = style {
        super::tree::push_background_paint_command(
            commands,
            &style.background_paint,
            rect,
            radii,
            clip,
        );
        super::tree::push_border_commands(
            commands,
            rect,
            &style.border_width,
            radii,
            style.border_color,
            scale,
            clip,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masked_input_offsets_follow_displayed_characters() {
        let value = "aé🦀z";
        assert_eq!(input_display_offset(value, true, 1), Some(1));
        assert_eq!(input_display_offset(value, true, 3), Some(2));
        assert_eq!(input_display_offset(value, true, 7), Some(3));
        assert_eq!(input_display_offset(value, true, value.len()), Some(4));
        assert_eq!(input_display_offset(value, false, 7), Some(7));
        for invalid in [2, 4, 5, 6, 9] {
            assert_eq!(input_display_offset(value, true, invalid), None);
            assert_eq!(input_display_offset(value, false, invalid), None);
        }
    }

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
