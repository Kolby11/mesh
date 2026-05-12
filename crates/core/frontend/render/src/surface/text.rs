//! Text measurement and rendering for the frontend render engine.

use super::PixelBuffer;
use cosmic_text::{
    Align, Attrs, Buffer, Cursor, Family, FontSystem, Metrics, Shaping, Style as CosmicStyle,
    SwashCache, Weight, Wrap,
};
use mesh_core_elements::Color;
use mesh_core_elements::style::TextAlign;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

const TEXT_LAYOUT_CACHE_CAPACITY: usize = 128;

pub struct TextRenderer {
    engine: Mutex<TextEngine>,
}

struct TextEngine {
    font_system: FontSystem,
    swash_cache: SwashCache,
    layout_cache: HashMap<TextLayoutKey, Buffer>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextLayoutKey {
    text: String,
    font_family: String,
    font_size: u32,
    font_weight: u16,
    line_height: u32,
    max_width: Option<u32>,
    align: TextAlign,
}

impl TextLayoutKey {
    fn new(
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
        align: TextAlign,
    ) -> Self {
        Self {
            text: text.to_string(),
            font_family: font_family.to_string(),
            font_size: font_size.to_bits(),
            font_weight,
            line_height: line_height.to_bits(),
            max_width: max_width.map(f32::to_bits),
            align,
        }
    }
}

impl Hash for TextLayoutKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text.hash(state);
        self.font_family.hash(state);
        self.font_size.hash(state);
        self.font_weight.hash(state);
        self.line_height.hash(state);
        self.max_width.hash(state);
        match self.align {
            TextAlign::Left => 0u8,
            TextAlign::Center => 1u8,
            TextAlign::Right => 2u8,
        }
        .hash(state);
    }
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
            engine: Mutex::new(TextEngine {
                font_system: FontSystem::new(),
                swash_cache: SwashCache::new(),
                layout_cache: HashMap::new(),
                metrics: TextCacheMetrics {
                    glyph_cache_active: true,
                    ..Default::default()
                },
            }),
        }
    }

    pub fn cache_metrics(&self) -> TextCacheMetrics {
        let mut engine = self.engine.lock().unwrap();
        engine.metrics.shaped_entries = engine.layout_cache.len() as u64;
        engine.metrics
    }

    pub fn reset_cache_metrics(&self) {
        let mut engine = self.engine.lock().unwrap();
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
        let mut engine = self.engine.lock().unwrap();
        let (_, metrics, width, text_align) = text_config(
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            align,
        );
        let key = TextLayoutKey::new(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            align,
        );
        let mut cosmic = engine.take_layout(&key, metrics, width, text_align);

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
                    let draw_color = Color { r, g, b, a };

                    for off_y in 0..glyph_h {
                        for off_x in 0..glyph_w {
                            let px = draw_x + off_x as i32;
                            let py = draw_y + off_y as i32;
                            if px < clip_x as i32
                                || py < clip_y as i32
                                || px >= clip_right as i32
                                || py >= clip_bottom as i32
                            {
                                continue;
                            }
                            buffer.blend_pixel(px as u32, py as u32, draw_color, 255);
                        }
                    }
                },
            );
        }
        engine.store_layout(key, cosmic);
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
        let mut engine = self.engine.lock().unwrap();
        let (_, metrics, width, _) = text_config(
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            TextAlign::Left,
        );
        let key = TextLayoutKey::new(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            TextAlign::Left,
        );
        let mut cosmic = engine.take_layout(&key, metrics, width, Align::Left);

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

        engine.store_layout(key, cosmic);
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
        let mut engine = self.engine.lock().unwrap();
        let (_, metrics, width, text_align) = text_config(
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            align,
        );
        let key = TextLayoutKey::new(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            align,
        );
        let mut cosmic = engine.take_layout(&key, metrics, width, text_align);

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

        engine.store_layout(key, cosmic);
        result
    }
}

impl TextEngine {
    fn take_layout(
        &mut self,
        key: &TextLayoutKey,
        metrics: Metrics,
        width: Option<f32>,
        align: Align,
    ) -> Buffer {
        if let Some(cosmic) = self.layout_cache.remove(key) {
            self.metrics.layout_hits = self.metrics.layout_hits.saturating_add(1);
            return cosmic;
        }

        self.metrics.layout_misses = self.metrics.layout_misses.saturating_add(1);
        let shaping_started = std::time::Instant::now();
        let (attrs, _, _, _) = text_config(
            &key.font_family,
            f32::from_bits(key.font_size),
            key.font_weight,
            f32::from_bits(key.line_height),
            key.max_width.map(f32::from_bits),
            key.align,
        );
        let mut cosmic = Buffer::new(&mut self.font_system, metrics);
        {
            let mut cosmic_borrow = cosmic.borrow_with(&mut self.font_system);
            cosmic_borrow.set_wrap(wrap_for(key.max_width.map(f32::from_bits)));
            cosmic_borrow.set_size(width, None);
            cosmic_borrow.set_text(&key.text, &attrs, Shaping::Advanced, Some(align));
        }
        self.metrics.shaping_micros = self.metrics.shaping_micros.saturating_add(
            shaping_started
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64,
        );
        cosmic
    }

    fn store_layout(&mut self, key: TextLayoutKey, cosmic: Buffer) {
        if self.layout_cache.len() >= TEXT_LAYOUT_CACHE_CAPACITY
            && !self.layout_cache.contains_key(&key)
            && let Some(evicted) = self.layout_cache.keys().next().cloned()
        {
            self.layout_cache.remove(&evicted);
            self.metrics.layout_invalidations = self.metrics.layout_invalidations.saturating_add(1);
        }
        self.layout_cache.insert(key, cosmic);
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
