use super::*;

impl FrontendRenderEngine {
    pub(super) fn render_text_node(
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
            .map(|value| value.as_str())
            .or_else(|| node.attributes.get("content").map(|value| value.as_str()))
            .unwrap_or("");

        if text.is_empty() {
            return;
        }

        let tx = (x + (style.padding.left * scale) as i32).max(0) as u32;
        let ty = (y + (style.padding.top * scale) as i32).max(0) as u32;
        let inner_width = ((node.layout.width - style.padding.horizontal()) * scale).max(0.0);

        let display_text: std::borrow::Cow<'_, str> =
            if style.text_overflow == TextOverflow::Ellipsis && inner_width > 0.0 {
                let (text_width, _) = self.text_renderer.measure_styled(
                    text,
                    &style.font_family,
                    style.font_size * scale,
                    style.font_weight,
                    style.line_height,
                    None,
                );
                if text_width > inner_width {
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

        let effective_align =
            if style.text_direction == TextDirection::Rtl && style.text_align == TextAlign::Left {
                TextAlign::Right
            } else {
                style.text_align
            };

        if let Some(selection) = selection_geometry(
            &self.text_renderer,
            node,
            style,
            &display_text,
            effective_align,
            inner_width,
            scale,
        ) {
            render_selection_highlights(
                &self.text_renderer,
                buffer,
                tx as i32,
                ty as i32,
                clip,
                style,
                &display_text,
                effective_align,
                inner_width,
                scale,
                selection,
            );
            return;
        }

        self.text_renderer.render_clipped(
            &display_text,
            &style.font_family,
            style.font_size * scale,
            style.font_weight,
            style.line_height,
            effective_align,
            style.color,
            buffer,
            tx,
            ty,
            clip_to_tuple(clip),
            Some(inner_width),
        );
    }

    pub fn render_tooltip(
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
}

pub(super) fn truncate_with_ellipsis(
    renderer: &TextRenderer,
    text: &str,
    font_family: &str,
    font_size: f32,
    font_weight: u16,
    line_height: f32,
    max_width: f32,
) -> String {
    const ELLIPSIS: &str = "…";
    let (ellipsis_width, _) = renderer.measure_styled(
        ELLIPSIS,
        font_family,
        font_size,
        font_weight,
        line_height,
        None,
    );
    let target = (max_width - ellipsis_width).max(0.0);

    let chars: Vec<char> = text.chars().collect();
    for len in (0..=chars.len()).rev() {
        let truncated: String = chars[..len].iter().collect();
        let (width, _) = renderer.measure_styled(
            &truncated,
            font_family,
            font_size,
            font_weight,
            line_height,
            None,
        );
        if width <= target {
            return format!("{truncated}{ELLIPSIS}");
        }
    }
    ELLIPSIS.to_string()
}

pub(super) fn selection_geometry(
    renderer: &TextRenderer,
    node: &WidgetNode,
    style: &mesh_core_elements::style::ComputedStyle,
    display_text: &str,
    align: TextAlign,
    inner_width: f32,
    scale: f32,
) -> Option<(TextSelectionGeometry, Color, Color)> {
    if display_text.is_empty()
        || style.text_overflow == TextOverflow::Ellipsis
        || style.overflow_x != Overflow::Visible
        || style.overflow_y != Overflow::Visible
    {
        return None;
    }

    let selection_background = node
        .attributes
        .get("_mesh_selection_background")
        .and_then(|value| Color::from_hex(value))?;
    let selection_foreground = node
        .attributes
        .get("_mesh_selection_foreground")
        .and_then(|value| Color::from_hex(value))?;
    let anchor_x = node
        .attributes
        .get("_mesh_selection_anchor_x")?
        .parse::<f32>()
        .ok()?;
    let anchor_y = node
        .attributes
        .get("_mesh_selection_anchor_y")?
        .parse::<f32>()
        .ok()?;
    let focus_x = node
        .attributes
        .get("_mesh_selection_focus_x")?
        .parse::<f32>()
        .ok()?;
    let focus_y = node
        .attributes
        .get("_mesh_selection_focus_y")?
        .parse::<f32>()
        .ok()?;
    let text_x = node_attr_f32(node, "_mesh_selection_text_x");
    let text_y = node_attr_f32(node, "_mesh_selection_text_y");

    let geometry = renderer.selection_geometry(
        display_text,
        &style.font_family,
        style.font_size * scale,
        style.font_weight,
        style.line_height,
        align,
        Some(inner_width),
        (anchor_x - text_x, anchor_y - text_y),
        (focus_x - text_x, focus_y - text_y),
    )?;

    if geometry.highlights.is_empty() {
        return None;
    }

    Some((geometry, selection_background, selection_foreground))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_selection_highlights(
    renderer: &TextRenderer,
    buffer: &mut PixelBuffer,
    tx: i32,
    ty: i32,
    clip: ClipRect,
    style: &mesh_core_elements::style::ComputedStyle,
    display_text: &str,
    align: TextAlign,
    inner_width: f32,
    scale: f32,
    selection: (TextSelectionGeometry, Color, Color),
) {
    let (selection_geometry, selection_background, selection_foreground) = selection;

    renderer.render_clipped(
        display_text,
        &style.font_family,
        style.font_size * scale,
        style.font_weight,
        style.line_height,
        align,
        style.color,
        buffer,
        tx.max(0) as u32,
        ty.max(0) as u32,
        clip_to_tuple(clip),
        Some(inner_width),
    );

    for highlight in &selection_geometry.highlights {
        let rect = ClipRect {
            x: tx + highlight.x.round() as i32,
            y: ty + highlight.y.round() as i32,
            width: highlight.width.ceil() as i32,
            height: highlight.height.ceil() as i32,
        };
        let highlight_clip = intersect_clip(clip, rect);
        fill_rect_clipped(buffer, rect, selection_background, highlight_clip);
        renderer.render_clipped(
            display_text,
            &style.font_family,
            style.font_size * scale,
            style.font_weight,
            style.line_height,
            align,
            selection_foreground,
            buffer,
            tx.max(0) as u32,
            ty.max(0) as u32,
            clip_to_tuple(highlight_clip),
            Some(inner_width),
        );
    }
}
