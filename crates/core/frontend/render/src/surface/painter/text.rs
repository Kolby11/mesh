use crate::display_list::{DisplayPaintNode, DisplayTextPaint};
use mesh_core_elements::lru::LruCache;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use super::*;

static ELLIPSIS_CACHE: OnceLock<Mutex<LruCache<u64, EllipsisCacheEntry>>> = OnceLock::new();
const ELLIPSIS_CACHE_CAPACITY: usize = 512;

fn ellipsis_cache() -> &'static Mutex<LruCache<u64, EllipsisCacheEntry>> {
    ELLIPSIS_CACHE.get_or_init(|| Mutex::new(LruCache::new(ELLIPSIS_CACHE_CAPACITY)))
}

pub(super) trait TextRenderCache {
    fn measure_styled(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32);

    #[allow(clippy::too_many_arguments)]
    fn render_clipped(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        align: TextAlign,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
        clip: (u32, u32, u32, u32),
        max_width: Option<f32>,
    );

    #[allow(clippy::too_many_arguments)]
    fn selection_geometry(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        align: TextAlign,
        max_width: Option<f32>,
        anchor: (f32, f32),
        focus: (f32, f32),
    ) -> Option<TextSelectionGeometry>;
}

impl TextRenderCache for TextRenderer {
    fn measure_styled(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        TextRenderer::measure_styled(
            self,
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
        )
    }

    fn render_clipped(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        align: TextAlign,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
        clip: (u32, u32, u32, u32),
        max_width: Option<f32>,
    ) {
        TextRenderer::render_clipped(
            self,
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            align,
            color,
            buffer,
            x,
            y,
            clip,
            max_width,
        );
    }

    fn selection_geometry(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        align: TextAlign,
        max_width: Option<f32>,
        anchor: (f32, f32),
        focus: (f32, f32),
    ) -> Option<TextSelectionGeometry> {
        TextRenderer::selection_geometry(
            self,
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            align,
            max_width,
            anchor,
            focus,
        )
    }
}

impl TextRenderCache for SharedTextMeasurer {
    fn measure_styled(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        SharedTextMeasurer::measure_styled(
            self,
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
        )
    }

    fn render_clipped(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        align: TextAlign,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
        clip: (u32, u32, u32, u32),
        max_width: Option<f32>,
    ) {
        SharedTextMeasurer::render_clipped(
            self,
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            align,
            color,
            buffer,
            x,
            y,
            clip,
            max_width,
        );
    }

    fn selection_geometry(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        align: TextAlign,
        max_width: Option<f32>,
        anchor: (f32, f32),
        focus: (f32, f32),
    ) -> Option<TextSelectionGeometry> {
        SharedTextMeasurer::selection_geometry(
            self,
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            align,
            max_width,
            anchor,
            focus,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EllipsisCacheEntry {
    text: String,
    font_family: String,
    font_size: u32,
    font_weight: u16,
    line_height: u32,
    max_width: u32,
    value: String,
}

impl EllipsisCacheEntry {
    fn matches(
        &self,
        text: &str,
        font_family: &str,
        font_size: u32,
        font_weight: u16,
        line_height: u32,
        max_width: u32,
    ) -> bool {
        self.text == text
            && self.font_family == font_family
            && self.font_size == font_size
            && self.font_weight == font_weight
            && self.line_height == line_height
            && self.max_width == max_width
    }
}

struct EllipsisHasher(u64);

impl Default for EllipsisHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for EllipsisHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

fn ellipsis_cache_key(
    text: &str,
    font_family: &str,
    font_size: u32,
    font_weight: u16,
    line_height: u32,
    max_width: u32,
) -> u64 {
    let mut state = EllipsisHasher::default();
    text.hash(&mut state);
    font_family.hash(&mut state);
    font_size.hash(&mut state);
    font_weight.hash(&mut state);
    line_height.hash(&mut state);
    max_width.hash(&mut state);
    state.finish()
}

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
                self,
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
            opacity_color(style.color, style.opacity),
            buffer,
            tx,
            ty,
            clip_to_tuple(clip),
            Some(inner_width),
        );
    }

    pub(super) fn render_display_text_node(
        &self,
        node: &DisplayPaintNode,
        text: &DisplayTextPaint,
        buffer: &mut PixelBuffer,
        scale: f32,
        x: i32,
        y: i32,
        clip: ClipRect,
    ) {
        let style = &node.style;
        if text.text.is_empty() {
            return;
        }

        let tx = (x + (style.padding.left * scale) as i32).max(0) as u32;
        let ty = (y + (style.padding.top * scale) as i32).max(0) as u32;
        let inner_width = ((node.layout.width - style.padding.horizontal()) * scale).max(0.0);

        let display_text: std::borrow::Cow<'_, str> =
            if style.text_overflow == TextOverflow::Ellipsis && inner_width > 0.0 {
                let (text_width, _) = self.text_renderer.measure_styled(
                    &text.text,
                    &style.font_family,
                    style.font_size * scale,
                    style.font_weight,
                    style.line_height,
                    None,
                );
                if text_width > inner_width {
                    std::borrow::Cow::Owned(truncate_with_ellipsis(
                        &self.text_renderer,
                        &text.text,
                        &style.font_family,
                        style.font_size * scale,
                        style.font_weight,
                        style.line_height,
                        inner_width,
                    ))
                } else {
                    std::borrow::Cow::Borrowed(text.text.as_str())
                }
            } else {
                std::borrow::Cow::Borrowed(text.text.as_str())
            };

        let effective_align =
            if style.text_direction == TextDirection::Rtl && style.text_align == TextAlign::Left {
                TextAlign::Right
            } else {
                style.text_align
            };

        if let Some(selection) = selection_geometry_for_display(
            &self.text_renderer,
            node,
            &display_text,
            effective_align,
            inner_width,
            scale,
        ) {
            render_display_selection_highlights(
                self,
                &self.text_renderer,
                buffer,
                tx as i32,
                ty as i32,
                clip,
                node,
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

        let colors = self.tooltip_colors();
        let bg = colors.background;
        let border = colors.border;
        let text_color = colors.foreground;
        let radius = (6.0 * scale).max(3.0);

        self.fill_rounded_rect_clipped(
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
        self.fill_rounded_rect_clipped(
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
    renderer: &impl TextRenderCache,
    text: &str,
    font_family: &str,
    font_size: f32,
    font_weight: u16,
    line_height: f32,
    max_width: f32,
) -> String {
    let font_size_bits = font_size.to_bits();
    let line_height_bits = line_height.to_bits();
    let max_width_bits = max_width.to_bits();
    let cache_key = ellipsis_cache_key(
        text,
        font_family,
        font_size_bits,
        font_weight,
        line_height_bits,
        max_width_bits,
    );
    let cache = ellipsis_cache();
    if let Ok(mut guard) = cache.lock()
        && let Some(cached) = guard.get(&cache_key)
        && cached.matches(
            text,
            font_family,
            font_size_bits,
            font_weight,
            line_height_bits,
            max_width_bits,
        )
    {
        return cached.value.clone();
    }

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
    let char_count = text.chars().count();

    if char_count == 0 {
        return ELLIPSIS.to_string();
    }

    let mut low = 0usize;
    let mut high = char_count;
    let mut boundaries: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
    boundaries.push(text.len());
    while low < high {
        let mid = (low + high) / 2;
        let split = boundaries[mid];
        let truncated = &text[..split];
        let (width, _) = renderer.measure_styled(
            truncated,
            font_family,
            font_size,
            font_weight,
            line_height,
            None,
        );
        if width <= target {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    let best = low.saturating_sub(1);
    let split = boundaries[best];
    let mut output = String::with_capacity(split + ELLIPSIS.len());
    output.push_str(&text[..split]);
    output.push_str(ELLIPSIS);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            cache_key,
            EllipsisCacheEntry {
                text: text.to_string(),
                font_family: font_family.to_string(),
                font_size: font_size_bits,
                font_weight,
                line_height: line_height_bits,
                max_width: max_width_bits,
                value: output.clone(),
            },
        );
    }
    output
}

pub(super) fn selection_geometry(
    renderer: &impl TextRenderCache,
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

fn selection_geometry_for_display(
    renderer: &impl TextRenderCache,
    node: &DisplayPaintNode,
    display_text: &str,
    align: TextAlign,
    inner_width: f32,
    scale: f32,
) -> Option<(TextSelectionGeometry, Color, Color)> {
    let style = &node.style;
    if display_text.is_empty()
        || style.text_overflow == TextOverflow::Ellipsis
        || style.overflow_x != Overflow::Visible
        || style.overflow_y != Overflow::Visible
    {
        return None;
    }

    let selection = match &node.content {
        crate::display_list::DisplayPaintContent::Text(text) => text.selection?,
        _ => return None,
    };

    let geometry = renderer.selection_geometry(
        display_text,
        &style.font_family,
        style.font_size * scale,
        style.font_weight,
        style.line_height,
        align,
        Some(inner_width),
        (
            selection.anchor_x - selection.text_x,
            selection.anchor_y - selection.text_y,
        ),
        (
            selection.focus_x - selection.text_x,
            selection.focus_y - selection.text_y,
        ),
    )?;

    if geometry.highlights.is_empty() {
        return None;
    }

    Some((geometry, selection.background, selection.foreground))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_selection_highlights(
    paint_engine: &FrontendRenderEngine,
    renderer: &impl TextRenderCache,
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
        paint_engine.fill_rect_clipped(buffer, rect, selection_background, highlight_clip);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_with_ellipsis_appends_ellipsis_for_short_space() {
        let renderer = TextRenderer::new();
        let text = "hello world";
        let (char_width, _) = renderer.measure_styled("h", "Inter", 14.0, 400, 1.4, None);
        let (ellipsis_width, _) = renderer.measure_styled("…", "Inter", 14.0, 400, 1.4, None);
        let max_width = char_width + ellipsis_width;

        let truncated = truncate_with_ellipsis(&renderer, text, "Inter", 14.0, 400, 1.4, max_width);

        let prefix = truncated
            .strip_suffix("…")
            .expect("truncated text should include ellipsis");
        assert!(!prefix.is_empty());
        assert!(text.starts_with(prefix));
        let (truncated_width, _) =
            renderer.measure_styled(&truncated, "Inter", 14.0, 400, 1.4, None);
        assert!(truncated_width <= max_width);
    }

    #[test]
    fn truncate_with_ellipsis_handles_non_ascii_boundaries() {
        let renderer = TextRenderer::new();
        let text = "😊😊😊";
        let (char_width, _) = renderer.measure_styled("😊", "Inter", 14.0, 400, 1.4, None);
        let (ellipsis_width, _) = renderer.measure_styled("…", "Inter", 14.0, 400, 1.4, None);
        let max_width = char_width + ellipsis_width;

        let truncated = truncate_with_ellipsis(&renderer, text, "Inter", 14.0, 400, 1.4, max_width);

        assert_eq!(truncated, "😊…");
        let (truncated_width, _) =
            renderer.measure_styled(&truncated, "Inter", 14.0, 400, 1.4, None);
        assert!(truncated_width <= max_width);
    }

    #[test]
    fn truncate_with_ellipsis_empty_text_returns_ellipsis() {
        let renderer = TextRenderer::new();
        let truncated = truncate_with_ellipsis(&renderer, "", "Inter", 14.0, 400, 1.4, 20.0);
        assert_eq!(truncated, "…");
    }
}

#[allow(clippy::too_many_arguments)]
fn render_display_selection_highlights(
    paint_engine: &FrontendRenderEngine,
    renderer: &impl TextRenderCache,
    buffer: &mut PixelBuffer,
    tx: i32,
    ty: i32,
    clip: ClipRect,
    node: &DisplayPaintNode,
    display_text: &str,
    align: TextAlign,
    inner_width: f32,
    scale: f32,
    selection: (TextSelectionGeometry, Color, Color),
) {
    let style = &node.style;
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
        paint_engine.fill_rect_clipped(buffer, rect, selection_background, highlight_clip);
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
