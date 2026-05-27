//! Text measurement and rendering for the frontend render engine.

use super::PixelBuffer;
use cosmic_text::{
    Align, Attrs, Buffer, Cursor, Family, FontSystem, Metrics, Shaping, Style as CosmicStyle,
    SwashCache, Weight, Wrap,
};
use mesh_core_elements::Color;
use mesh_core_elements::lru::LruCache;
use mesh_core_elements::style::TextAlign;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const TEXT_LAYOUT_CACHE_CAPACITY: usize = 512;

pub struct TextRenderer {
    engine: RefCell<TextEngine>,
}

struct TextEngine {
    font_system: FontSystem,
    swash_cache: SwashCache,
    layout_cache: LruCache<u64, TextLayoutEntry>,
    metrics: TextCacheMetrics,
}

thread_local! {
    static RENDERER: RefCell<TextRenderer> = RefCell::new(TextRenderer::new());
}

pub struct SharedTextMeasurer;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextCacheMetrics {
    pub layout_hits: u64,
    pub layout_misses: u64,
    pub layout_invalidations: u64,
    pub shaped_entries: u64,
    pub glyph_cache_active: bool,
    pub shaping_micros: u64,
}

struct TextLayoutEntry {
    text: String,
    font_family: String,
    font_size: u32,
    font_weight: u16,
    line_height: u32,
    max_width: Option<u32>,
    align: TextAlign,
    buffer: Buffer,
}

#[derive(Debug, Clone, Copy)]
struct TextLayoutParams<'a> {
    text: &'a str,
    font_family: &'a str,
    font_size: u32,
    font_weight: u16,
    line_height: u32,
    max_width: Option<u32>,
    align: TextAlign,
    cache_key: u64,
}

impl<'a> TextLayoutParams<'a> {
    fn new(
        text: &'a str,
        font_family: &'a str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
        align: TextAlign,
    ) -> Self {
        let font_size = font_size.to_bits();
        let line_height = line_height.to_bits();
        let max_width = max_width.map(f32::to_bits);
        let cache_key = text_layout_cache_key(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            align,
        );
        Self {
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            align,
            cache_key,
        }
    }
}

impl TextLayoutEntry {
    fn matches(&self, params: &TextLayoutParams<'_>) -> bool {
        self.text == params.text
            && self.font_family == params.font_family
            && self.font_size == params.font_size
            && self.font_weight == params.font_weight
            && self.line_height == params.line_height
            && self.max_width == params.max_width
            && self.align == params.align
    }
}

fn text_layout_cache_key(
    text: &str,
    font_family: &str,
    font_size: u32,
    font_weight: u16,
    line_height: u32,
    max_width: Option<u32>,
    align: TextAlign,
) -> u64 {
    let mut state = DefaultHasher::new();
    text.hash(&mut state);
    font_family.hash(&mut state);
    font_size.hash(&mut state);
    font_weight.hash(&mut state);
    line_height.hash(&mut state);
    max_width.hash(&mut state);
    match align {
        TextAlign::Left => 0u8,
        TextAlign::Center => 1u8,
        TextAlign::Right => 2u8,
    }
    .hash(&mut state);
    state.finish()
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextSelectionRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextSelectionGeometry {
    pub start: Cursor,
    pub end: Cursor,
    pub selected_text: String,
    pub highlights: Vec<TextSelectionRect>,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            engine: RefCell::new(TextEngine {
                font_system: FontSystem::new(),
                swash_cache: SwashCache::new(),
                layout_cache: LruCache::new(TEXT_LAYOUT_CACHE_CAPACITY),
                metrics: TextCacheMetrics {
                    glyph_cache_active: true,
                    ..Default::default()
                },
            }),
        }
    }

    pub fn cache_metrics(&self) -> TextCacheMetrics {
        let mut engine = self.engine.borrow_mut();
        engine.metrics.shaped_entries = engine.layout_cache.len() as u64;
        engine.metrics
    }

    pub fn reset_cache_metrics(&self) {
        let mut engine = self.engine.borrow_mut();
        let shaped_entries = engine.layout_cache.len() as u64;
        engine.metrics = TextCacheMetrics {
            shaped_entries,
            glyph_cache_active: true,
            ..Default::default()
        };
    }

    pub fn measure(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        self.measure_styled(text, font_family, font_size, 400, 1.0, max_width)
    }

    pub fn render(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
    ) {
        let clip = (0, 0, buffer.width, buffer.height);
        self.render_clipped(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            TextAlign::Left,
            color,
            buffer,
            x,
            y,
            clip,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_clipped(
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
        let mut engine = self.engine.borrow_mut();
        let (_, metrics, width, text_align) = text_config(
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            align,
        );
        let params = TextLayoutParams::new(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            align,
        );
        let mut cosmic = engine.take_layout(&params, metrics, width, text_align);

        let base_x = x as i32;
        let base_y = y as i32;
        let (clip_x, clip_y, clip_w, clip_h) = clip;
        let clip_right = clip_x.saturating_add(clip_w);
        let clip_bottom = clip_y.saturating_add(clip_h);

        {
            let TextEngine {
                font_system,
                swash_cache,
                ..
            } = &mut *engine;
            let mut cosmic_borrow = cosmic.borrow_with(font_system);
            cosmic_borrow.draw(
                swash_cache,
                cosmic_color(color),
                |glyph_x, glyph_y, glyph_w, glyph_h, glyph_color| {
                    let draw_x = base_x + glyph_x;
                    let draw_y = base_y + glyph_y;

                    let (r, g, b, a) = glyph_color.as_rgba_tuple();
                    if a == 0 || glyph_w == 0 || glyph_h == 0 {
                        return;
                    }

                    let start_x = (clip_x as i32 - draw_x).max(0) as u32;
                    let start_y = (clip_y as i32 - draw_y).max(0) as u32;
                    let end_x = (clip_right as i32 - draw_x).min(glyph_w as i32).max(0) as u32;
                    let end_y = (clip_bottom as i32 - draw_y).min(glyph_h as i32).max(0) as u32;
                    if start_x >= end_x || start_y >= end_y {
                        return;
                    }

                    let src_alpha = u16::from(a);
                    let inv_alpha = 255u16.saturating_sub(src_alpha);
                    let src_b = u16::from(b) * src_alpha;
                    let src_g = u16::from(g) * src_alpha;
                    let src_r = u16::from(r) * src_alpha;
                    for off_y in start_y..end_y {
                        let py = draw_y + off_y as i32;
                        let px = draw_x + start_x as i32;
                        let mut offset = (py as u32 * buffer.stride + px as u32 * 4) as usize;
                        for _ in start_x..end_x {
                            if offset + 3 >= buffer.data.len() {
                                break;
                            }
                            if src_alpha == 255 {
                                buffer.data[offset] = b;
                                buffer.data[offset + 1] = g;
                                buffer.data[offset + 2] = r;
                                buffer.data[offset + 3] = 255;
                            } else {
                                let dst_b = u16::from(buffer.data[offset]);
                                let dst_g = u16::from(buffer.data[offset + 1]);
                                let dst_r = u16::from(buffer.data[offset + 2]);
                                let dst_a = u16::from(buffer.data[offset + 3]);
                                buffer.data[offset] = ((src_b + dst_b * inv_alpha) / 255) as u8;
                                buffer.data[offset + 1] = ((src_g + dst_g * inv_alpha) / 255) as u8;
                                buffer.data[offset + 2] = ((src_r + dst_r * inv_alpha) / 255) as u8;
                                buffer.data[offset + 3] =
                                    (src_alpha + ((dst_a * inv_alpha) / 255)).min(255) as u8;
                            }
                            offset += 4;
                        }
                    }
                },
            );
        }
        engine.store_layout(&params, cosmic);
    }

    pub fn measure_styled(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        let mut engine = self.engine.borrow_mut();
        let (_, metrics, width, _) = text_config(
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            TextAlign::Left,
        );
        let params = TextLayoutParams::new(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            TextAlign::Left,
        );
        let mut cosmic = engine.take_layout(&params, metrics, width, Align::Left);

        let mut measured_width = 0.0f32;
        let mut measured_height = 0.0f32;
        {
            let cosmic = cosmic.borrow_with(&mut engine.font_system);
            for run in cosmic.layout_runs() {
                measured_width = measured_width.max(run.line_w);
                measured_height = measured_height.max(run.line_top + run.line_height);
            }
        }

        if measured_height <= 0.0 {
            measured_height = metrics.line_height;
        }

        engine.store_layout(&params, cosmic);
        (measured_width, measured_height)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn selection_geometry(
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
        let mut engine = self.engine.borrow_mut();
        let (_, metrics, width, text_align) = text_config(
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            align,
        );
        let params = TextLayoutParams::new(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            align,
        );
        let mut cosmic = engine.take_layout(&params, metrics, width, text_align);

        let result = {
            let cosmic = cosmic.borrow_with(&mut engine.font_system);
            let anchor_cursor = cosmic.hit(anchor.0, anchor.1);
            let focus_cursor = cosmic.hit(focus.0, focus.1);
            if let (Some(anchor_cursor), Some(focus_cursor)) = (anchor_cursor, focus_cursor) {
                let (start, end) = order_cursors(anchor_cursor, focus_cursor);
                let selected_text = extract_selected_text(text, start, end);
                let highlights = cosmic
                    .layout_runs()
                    .filter_map(|run| {
                        run.highlight(start, end)
                            .map(|(x, width)| TextSelectionRect {
                                x,
                                y: run.line_top,
                                width,
                                height: run.line_height,
                            })
                    })
                    .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
                    .collect();

                Some(TextSelectionGeometry {
                    start,
                    end,
                    selected_text,
                    highlights,
                })
            } else {
                None
            }
        };

        engine.store_layout(&params, cosmic);
        result
    }
}

impl TextEngine {
    fn take_layout(
        &mut self,
        params: &TextLayoutParams<'_>,
        metrics: Metrics,
        width: Option<f32>,
        align: Align,
    ) -> Buffer {
        if let Some(entry) = self.layout_cache.remove(&params.cache_key)
            && entry.matches(params)
        {
            self.metrics.layout_hits = self.metrics.layout_hits.saturating_add(1);
            return entry.buffer;
        }

        self.metrics.layout_misses = self.metrics.layout_misses.saturating_add(1);
        let shaping_started = std::time::Instant::now();
        let (attrs, _, _, _) = text_config(
            params.font_family,
            f32::from_bits(params.font_size),
            params.font_weight,
            f32::from_bits(params.line_height),
            params.max_width.map(f32::from_bits),
            params.align,
        );
        let mut cosmic = Buffer::new(&mut self.font_system, metrics);
        {
            let mut cosmic_borrow = cosmic.borrow_with(&mut self.font_system);
            cosmic_borrow.set_wrap(wrap_for(params.max_width.map(f32::from_bits)));
            cosmic_borrow.set_size(width, None);
            cosmic_borrow.set_text(params.text, &attrs, Shaping::Advanced, Some(align));
        }
        self.metrics.shaping_micros = self.metrics.shaping_micros.saturating_add(
            shaping_started
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64,
        );
        cosmic
    }

    fn store_layout(&mut self, params: &TextLayoutParams<'_>, cosmic: Buffer) {
        let evicting = self.layout_cache.len() >= TEXT_LAYOUT_CACHE_CAPACITY
            && !self.layout_cache.contains_key(&params.cache_key);
        self.layout_cache.insert(
            params.cache_key,
            TextLayoutEntry {
                text: params.text.to_string(),
                font_family: params.font_family.to_string(),
                font_size: params.font_size,
                font_weight: params.font_weight,
                line_height: params.line_height,
                max_width: params.max_width,
                align: params.align,
                buffer: cosmic,
            },
        );
        if evicting {
            self.metrics.layout_invalidations = self.metrics.layout_invalidations.saturating_add(1);
        }
        self.metrics.shaped_entries = self.layout_cache.len() as u64;
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl mesh_core_elements::TextMeasurer for TextRenderer {
    fn measure_text(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        self.measure_styled(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
        )
    }
}

impl mesh_core_elements::TextMeasurer for SharedTextMeasurer {
    fn measure_text(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        RENDERER.with(|renderer| {
            renderer.borrow().measure_styled(
                text,
                font_family,
                font_size,
                font_weight,
                line_height,
                max_width,
            )
        })
    }
}

fn text_config(
    font_family: &str,
    font_size: f32,
    font_weight: u16,
    line_height: f32,
    max_width: Option<f32>,
    align: TextAlign,
) -> (Attrs<'_>, Metrics, Option<f32>, Align) {
    let family = primary_family(font_family);
    let attrs = Attrs::new()
        .family(family)
        .style(CosmicStyle::Normal)
        .weight(Weight(font_weight.max(100)));
    let metrics = Metrics::new(
        font_size.max(1.0),
        (font_size * line_height.max(1.0)).max(1.0),
    );
    let width = max_width.filter(|value| *value > 0.0);
    let align = match align {
        TextAlign::Left => Align::Left,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::Right,
    };
    (attrs, metrics, width, align)
}

fn primary_family(font_family: &str) -> Family<'_> {
    let family = font_family
        .split(',')
        .map(|part| part.trim().trim_matches('"').trim_matches('\''))
        .find(|part| !part.is_empty())
        .unwrap_or("sans-serif");

    match family.to_ascii_lowercase().as_str() {
        "serif" => Family::Serif,
        "sans-serif" | "sans" | "system-ui" => Family::SansSerif,
        "monospace" | "mono" => Family::Monospace,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        _ => Family::Name(family),
    }
}

fn wrap_for(max_width: Option<f32>) -> Wrap {
    if max_width.is_some() {
        Wrap::Word
    } else {
        Wrap::None
    }
}

fn cosmic_color(color: Color) -> cosmic_text::Color {
    cosmic_text::Color::rgba(color.r, color.g, color.b, color.a)
}

fn order_cursors(a: Cursor, b: Cursor) -> (Cursor, Cursor) {
    match a.cmp(&b) {
        Ordering::Greater => (b, a),
        _ => (a, b),
    }
}

fn extract_selected_text(text: &str, start: Cursor, end: Cursor) -> String {
    if start == end {
        return String::new();
    }

    let lines: Vec<&str> = text.split('\n').collect();
    let mut output = String::new();

    for line_index in start.line..=end.line {
        let Some(line) = lines.get(line_index).copied() else {
            break;
        };
        let line_start = if line_index == start.line {
            start.index.min(line.len())
        } else {
            0
        };
        let line_end = if line_index == end.line {
            end.index.min(line.len())
        } else {
            line.len()
        };

        if line_start <= line_end {
            output.push_str(&line[line_start..line_end]);
        }

        if line_index != end.line {
            output.push('\n');
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_geometry_spans_wrapped_lines() {
        let geometry = TextRenderer::new()
            .selection_geometry(
                "alpha beta gamma delta epsilon",
                "Inter",
                14.0,
                400,
                1.4,
                TextAlign::Left,
                Some(64.0),
                (0.0, 0.0),
                (1000.0, 1000.0),
            )
            .expect("geometry");

        assert_eq!(geometry.selected_text, "alpha beta gamma delta epsilon");
        assert!(
            geometry.highlights.len() >= 2,
            "wrapped text should produce multiple highlighted line rects"
        );
    }

    #[test]
    fn selection_geometry_preserves_utf8_boundaries() {
        let utf8 = extract_selected_text(
            "cafe\u{301} nai\u{308}ve",
            Cursor::new(0, 0),
            Cursor::new(0, "cafe\u{301} nai\u{308}ve".len()),
        );
        assert_eq!(utf8, "cafe\u{301} nai\u{308}ve");
    }

    #[test]
    fn text_cache_reuses_unchanged_measure_layout() {
        let renderer = TextRenderer::new();
        renderer.reset_cache_metrics();

        let first = renderer.measure_styled("cached text", "Inter", 14.0, 400, 1.2, Some(120.0));
        let second = renderer.measure_styled("cached text", "Inter", 14.0, 400, 1.2, Some(120.0));
        let metrics = renderer.cache_metrics();

        assert_eq!(first, second);
        assert_eq!(metrics.layout_misses, 1);
        assert_eq!(metrics.layout_hits, 1);
        assert_eq!(metrics.shaped_entries, 1);
        assert!(metrics.glyph_cache_active);
    }

    #[test]
    fn text_cache_reuses_unchanged_render_layout() {
        let renderer = TextRenderer::new();
        renderer.reset_cache_metrics();

        let mut buffer = PixelBuffer::new(240, 80);
        renderer.render_clipped(
            "cached render text",
            "Inter",
            14.0,
            400,
            1.2,
            TextAlign::Left,
            Color::BLACK,
            &mut buffer,
            4,
            4,
            (0, 0, 240, 80),
            Some(180.0),
        );
        renderer.render_clipped(
            "cached render text",
            "Inter",
            14.0,
            400,
            1.2,
            TextAlign::Left,
            Color::WHITE,
            &mut buffer,
            12,
            8,
            (0, 0, 240, 80),
            Some(180.0),
        );
        let metrics = renderer.cache_metrics();

        assert_eq!(metrics.layout_misses, 1);
        assert_eq!(metrics.layout_hits, 1);
        assert_eq!(metrics.shaped_entries, 1);
    }

    #[test]
    fn text_cache_reuses_unchanged_selection_layout() {
        let renderer = TextRenderer::new();
        renderer.reset_cache_metrics();

        let first = renderer.selection_geometry(
            "alpha beta gamma delta",
            "Inter",
            14.0,
            400,
            1.2,
            TextAlign::Left,
            Some(120.0),
            (0.0, 0.0),
            (120.0, 40.0),
        );
        let second = renderer.selection_geometry(
            "alpha beta gamma delta",
            "Inter",
            14.0,
            400,
            1.2,
            TextAlign::Left,
            Some(120.0),
            (8.0, 0.0),
            (60.0, 20.0),
        );
        let metrics = renderer.cache_metrics();

        assert!(first.is_some());
        assert!(second.is_some());
        assert_eq!(metrics.layout_misses, 1);
        assert_eq!(metrics.layout_hits, 1);
        assert_eq!(metrics.shaped_entries, 1);
    }

    #[test]
    fn text_cache_misses_when_shaping_inputs_change() {
        let renderer = TextRenderer::new();
        renderer.reset_cache_metrics();

        renderer.measure_styled("cached text", "Inter", 14.0, 400, 1.2, Some(120.0));
        renderer.measure_styled("cached text", "Serif", 14.0, 400, 1.2, Some(120.0));
        renderer.measure_styled("cached text", "Inter", 15.0, 400, 1.2, Some(120.0));
        renderer.measure_styled("changed text", "Inter", 15.0, 400, 1.2, Some(120.0));
        renderer.measure_styled("changed text", "Inter", 15.0, 600, 1.2, Some(120.0));
        renderer.measure_styled("changed text", "Inter", 15.0, 600, 1.4, Some(120.0));
        renderer.measure_styled("changed text", "Inter", 15.0, 600, 1.4, Some(160.0));
        renderer.measure_styled("changed text", "Inter", 15.0, 600, 1.4, Some(160.0));
        let metrics = renderer.cache_metrics();

        assert_eq!(metrics.layout_misses, 7);
        assert_eq!(metrics.layout_hits, 1);
        assert_eq!(metrics.shaped_entries, 7);
    }

    #[test]
    fn text_cache_misses_when_alignment_changes() {
        let renderer = TextRenderer::new();
        renderer.reset_cache_metrics();
        let mut buffer = PixelBuffer::new(240, 80);

        renderer.render_clipped(
            "aligned text",
            "Inter",
            14.0,
            400,
            1.2,
            TextAlign::Left,
            Color::BLACK,
            &mut buffer,
            4,
            4,
            (0, 0, 240, 80),
            Some(180.0),
        );
        renderer.render_clipped(
            "aligned text",
            "Inter",
            14.0,
            400,
            1.2,
            TextAlign::Center,
            Color::BLACK,
            &mut buffer,
            4,
            4,
            (0, 0, 240, 80),
            Some(180.0),
        );
        let metrics = renderer.cache_metrics();

        assert_eq!(metrics.layout_misses, 2);
        assert_eq!(metrics.layout_hits, 0);
        assert_eq!(metrics.shaped_entries, 2);
    }
}
