//! Text measurement and rendering for the frontend render engine.

use super::{PixelBuffer, PixelCanvasSession, checked_pixel_bytes};
use cosmic_text::{
    Align, Attrs, AttrsOwned, Buffer, BufferLine, CacheKey, Cursor, Family, FamilyOwned,
    FontSystem, LayoutGlyph, LayoutLine, Metrics, PhysicalGlyph, Renderer, Shaping,
    Style as CosmicStyle, SwashCache, SwashContent, Weight, Wrap,
};
use mesh_core_elements::Color;
use mesh_core_elements::lru::ByteLruCache;
use mesh_core_elements::style::{FontStyle, TextAlign, TextDirection, WhiteSpace};
use mesh_core_elements::{TextMeasureContext, TextMeasureRevisions};
use mesh_core_resources::resource_revision;
use skia_safe::{
    AlphaType, Canvas, ColorType, Data, ImageInfo, Paint, Rect, SamplingOptions, images,
};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem::size_of;

const TEXT_LAYOUT_CACHE_CAPACITY: usize = 512;
const TEXT_LAYOUT_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_TEXT_LAYOUT_TEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_TEXT_LAYOUT_FAMILY_BYTES: usize = 4096;
const TEXT_LAYOUT_LINE_OVERHEAD_BYTES: usize = 512;
const TEXT_LAYOUT_ATTRIBUTE_SPAN_OVERHEAD_BYTES: usize = 512;
const TEXT_LAYOUT_GLYPH_OVERHEAD_BYTES: usize = size_of::<LayoutGlyph>() + 16 * size_of::<usize>();
const GLYPH_ATLAS_CAPACITY: usize = 2048;
const GLYPH_ATLAS_MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_GLYPH_ATLAS_DIMENSION: u32 = 4096;
const NAMED_FONT_AVAILABILITY_CACHE_CAPACITY: usize = 128;

struct GlyphAtlasEntry {
    image: skia_safe::Image,
    placement_left: i32,
    placement_top: i32,
    is_color: bool,
    bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphAtlasKey {
    resource_revision: u64,
    cache_key: CacheKey,
}

thread_local! {
    /// Cached Skia images for rasterized glyph masks. The explicit resource
    /// revision keeps a font-catalog replacement from reusing an image made
    /// by an older FontSystem, even if cosmic-text reuses the same glyph ID.
    /// `Option` lets us cache "this glyph rasterizes to nothing" (e.g.
    /// spaces) so we skip the swash lookup next time.
    static GLYPH_ATLAS: RefCell<ByteLruCache<GlyphAtlasKey, Option<GlyphAtlasEntry>>> =
        RefCell::new(ByteLruCache::new(GLYPH_ATLAS_CAPACITY, GLYPH_ATLAS_MAX_BYTES));
    static NAMED_FONT_AVAILABILITY: RefCell<HashMap<String, bool>> =
        RefCell::new(HashMap::new());
}

pub struct TextRenderer {
    engine: RefCell<TextEngine>,
}

struct TextEngine {
    font_system: FontSystem,
    locale: String,
    font_database: Option<fontdb::Database>,
    font_aliases: HashMap<String, String>,
    swash_cache: SwashCache,
    layout_cache: ByteLruCache<u64, TextLayoutEntry>,
    resource_revision: u64,
    measurer_revision: u64,
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
    pub layout_cache_bytes: u64,
    pub layout_cache_max_bytes: u64,
    pub glyph_cache_active: bool,
    pub shaping_micros: u64,
}

struct TextLayoutEntry {
    resource_revision: u64,
    measurer_revision: u64,
    text: String,
    font_family: String,
    font_size: u32,
    font_weight: u16,
    font_style: FontStyle,
    letter_spacing: u32,
    line_height: u32,
    text_direction: TextDirection,
    white_space: WhiteSpace,
    language: String,
    shaping_features: String,
    max_width: Option<u32>,
    align: TextAlign,
    buffer: Buffer,
}

struct TextLayoutHasher(u64);

impl Default for TextLayoutHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for TextLayoutHasher {
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

#[derive(Debug, Clone, Copy)]
struct TextLayoutParams<'a> {
    text: &'a str,
    font_family: &'a str,
    font_size: u32,
    font_weight: u16,
    font_style: FontStyle,
    letter_spacing: u32,
    line_height: u32,
    text_direction: TextDirection,
    white_space: WhiteSpace,
    language: &'a str,
    shaping_features: &'a str,
    max_width: Option<u32>,
    align: TextAlign,
    cache_key: u64,
    resource_revision: u64,
    measurer_revision: u64,
}

impl<'a> TextLayoutParams<'a> {
    fn new(
        text: &'a str,
        font_family: &'a str,
        font_size: f32,
        font_weight: u16,
        font_style: FontStyle,
        line_height: f32,
        max_width: Option<f32>,
        align: TextAlign,
        measurer_revision: u64,
    ) -> Self {
        let mut context = TextMeasureContext::new(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
        );
        context.font_style = font_style;
        Self::from_context(&context, align, resource_revision(), measurer_revision)
    }

    fn from_context(
        context: &TextMeasureContext<'a>,
        align: TextAlign,
        resource_revision: u64,
        measurer_revision: u64,
    ) -> Self {
        let font_size = context.font_size.to_bits();
        let line_height = context.line_height.to_bits();
        let letter_spacing = context.letter_spacing.to_bits();
        let max_width = context.max_width.map(f32::to_bits);
        let cache_key = text_layout_cache_key(
            context.text,
            context.font_family,
            font_size,
            context.font_weight,
            context.font_style,
            letter_spacing,
            line_height,
            context.text_direction,
            context.white_space,
            context.language,
            context.shaping_features,
            max_width,
            align,
            resource_revision,
            measurer_revision,
        );
        Self {
            text: context.text,
            font_family: context.font_family,
            font_size,
            font_weight: context.font_weight,
            font_style: context.font_style,
            letter_spacing,
            line_height,
            text_direction: context.text_direction,
            white_space: context.white_space,
            language: context.language,
            shaping_features: context.shaping_features,
            max_width,
            align,
            cache_key,
            resource_revision,
            measurer_revision,
        }
    }
}

impl TextLayoutEntry {
    fn matches(&self, params: &TextLayoutParams<'_>) -> bool {
        self.resource_revision == params.resource_revision
            && self.text == params.text
            && self.font_family == params.font_family
            && self.font_size == params.font_size
            && self.font_weight == params.font_weight
            && self.font_style == params.font_style
            && self.letter_spacing == params.letter_spacing
            && self.line_height == params.line_height
            && self.text_direction == params.text_direction
            && self.white_space == params.white_space
            && self.language == params.language
            && self.shaping_features == params.shaping_features
            && self.max_width == params.max_width
            && self.align == params.align
            && self.measurer_revision == params.measurer_revision
    }

    fn estimated_bytes(&self) -> Option<usize> {
        estimated_text_layout_bytes(
            self.text.capacity(),
            self.font_family.capacity(),
            &self.buffer,
        )
    }
}

/// Estimate the resident storage of one shaped layout conservatively. The
/// public cosmic-text API exposes the line and glyph counts but not the
/// private shape/layout vectors, so fixed per-line, per-span, and per-glyph
/// charges cover those internal allocations while the visible strings and
/// vector capacities account for the storage we can inspect directly.
fn estimated_text_layout_bytes(
    text_bytes: usize,
    font_family_bytes: usize,
    buffer: &Buffer,
) -> Option<usize> {
    if text_bytes > MAX_TEXT_LAYOUT_TEXT_BYTES || font_family_bytes > MAX_TEXT_LAYOUT_FAMILY_BYTES {
        return None;
    }

    let mut bytes = size_of::<TextLayoutEntry>()
        .checked_add(text_bytes)?
        .checked_add(font_family_bytes)?
        .checked_add(
            buffer
                .lines
                .capacity()
                .checked_mul(size_of::<BufferLine>())?,
        )?;

    let mut line_text_bytes = 0usize;
    let mut attribute_span_count = 0usize;
    for line in &buffer.lines {
        line_text_bytes = line_text_bytes.checked_add(line.text().len())?;
        attribute_span_count =
            attribute_span_count.checked_add(line.attrs_list().spans_iter().count())?;
    }
    bytes = bytes
        .checked_add(line_text_bytes)?
        .checked_add(
            buffer
                .lines
                .len()
                .checked_mul(TEXT_LAYOUT_LINE_OVERHEAD_BYTES)?,
        )?
        .checked_add(
            attribute_span_count.checked_mul(TEXT_LAYOUT_ATTRIBUTE_SPAN_OVERHEAD_BYTES)?,
        )?;

    let (run_count, glyph_count) =
        buffer
            .layout_runs()
            .fold((0usize, 0usize), |(runs, glyphs), run| {
                (
                    runs.saturating_add(1),
                    glyphs.saturating_add(run.glyphs.len()),
                )
            });
    bytes = bytes
        .checked_add(run_count.checked_mul(size_of::<LayoutLine>())?)?
        .checked_add(glyph_count.checked_mul(TEXT_LAYOUT_GLYPH_OVERHEAD_BYTES)?)?;

    (bytes <= TEXT_LAYOUT_CACHE_MAX_BYTES).then_some(bytes)
}

fn text_layout_cache_key(
    text: &str,
    font_family: &str,
    font_size: u32,
    font_weight: u16,
    font_style: FontStyle,
    letter_spacing: u32,
    line_height: u32,
    text_direction: TextDirection,
    white_space: WhiteSpace,
    language: &str,
    shaping_features: &str,
    max_width: Option<u32>,
    align: TextAlign,
    resource_revision: u64,
    measurer_revision: u64,
) -> u64 {
    let mut state = TextLayoutHasher::default();
    text.hash(&mut state);
    font_family.hash(&mut state);
    font_size.hash(&mut state);
    font_weight.hash(&mut state);
    font_style.hash(&mut state);
    letter_spacing.hash(&mut state);
    line_height.hash(&mut state);
    text_direction.hash(&mut state);
    white_space.hash(&mut state);
    language.hash(&mut state);
    shaping_features.hash(&mut state);
    max_width.hash(&mut state);
    match align {
        TextAlign::Left => 0u8,
        TextAlign::Center => 1u8,
        TextAlign::Right => 2u8,
    }
    .hash(&mut state);
    resource_revision.hash(&mut state);
    measurer_revision.hash(&mut state);
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
        let font_system = FontSystem::new();
        let locale = font_system.locale().to_owned();
        Self {
            engine: RefCell::new(TextEngine {
                font_system,
                locale,
                font_database: None,
                font_aliases: HashMap::new(),
                swash_cache: SwashCache::new(),
                layout_cache: ByteLruCache::new(
                    TEXT_LAYOUT_CACHE_CAPACITY,
                    TEXT_LAYOUT_CACHE_MAX_BYTES,
                ),
                resource_revision: resource_revision(),
                measurer_revision: 0,
                metrics: TextCacheMetrics {
                    layout_cache_max_bytes: TEXT_LAYOUT_CACHE_MAX_BYTES as u64,
                    glyph_cache_active: true,
                    ..Default::default()
                },
            }),
        }
    }

    pub fn cache_metrics(&self) -> TextCacheMetrics {
        let mut engine = self.engine.borrow_mut();
        engine.ensure_resource_revision();
        engine.metrics.shaped_entries = engine.layout_cache.len() as u64;
        engine.metrics.layout_cache_bytes = engine.layout_cache.bytes() as u64;
        engine.metrics.layout_cache_max_bytes = engine.layout_cache.max_bytes() as u64;
        engine.metrics
    }

    pub fn reset_cache_metrics(&self) {
        let mut engine = self.engine.borrow_mut();
        engine.ensure_resource_revision();
        let shaped_entries = engine.layout_cache.len() as u64;
        let layout_cache_bytes = engine.layout_cache.bytes() as u64;
        let layout_cache_max_bytes = engine.layout_cache.max_bytes() as u64;
        engine.metrics = TextCacheMetrics {
            shaped_entries,
            layout_cache_bytes,
            layout_cache_max_bytes,
            glyph_cache_active: true,
            ..Default::default()
        };
    }

    pub fn set_font_database(&self, database: fontdb::Database) {
        self.engine.borrow_mut().set_font_database(database);
    }

    pub fn set_font_aliases(&self, aliases: HashMap<String, String>) {
        self.engine.borrow_mut().set_font_aliases(aliases);
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
        self.render_clipped_with_font_style(
            text,
            font_family,
            font_size,
            font_weight,
            FontStyle::Normal,
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

    #[allow(clippy::too_many_arguments)]
    pub fn render_clipped_with_font_style(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        font_style: FontStyle,
        line_height: f32,
        align: TextAlign,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
        clip: (u32, u32, u32, u32),
        max_width: Option<f32>,
    ) {
        // Legacy callers pass a raw buffer. Wrap it in a one-shot canvas
        // session and route through the Skia glyph path. Hot-path callers
        // (display-list paint) should use `render_clipped_on_canvas`
        // directly so the surface wrap is shared with surrounding draws.
        let mut session = PixelCanvasSession::new(buffer);
        let _ = session.with_canvas(|canvas| {
            self.render_clipped_on_canvas_with_font_style(
                text,
                font_family,
                font_size,
                font_weight,
                font_style,
                line_height,
                align,
                color,
                canvas,
                x,
                y,
                clip,
                max_width,
            );
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_clipped_on_canvas(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        align: TextAlign,
        color: Color,
        canvas: &Canvas,
        x: u32,
        y: u32,
        clip: (u32, u32, u32, u32),
        max_width: Option<f32>,
    ) {
        self.render_clipped_on_canvas_with_font_style(
            text,
            font_family,
            font_size,
            font_weight,
            FontStyle::Normal,
            line_height,
            align,
            color,
            canvas,
            x,
            y,
            clip,
            max_width,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_clipped_on_canvas_with_font_style(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        font_style: FontStyle,
        line_height: f32,
        align: TextAlign,
        color: Color,
        canvas: &Canvas,
        x: u32,
        y: u32,
        clip: (u32, u32, u32, u32),
        max_width: Option<f32>,
    ) {
        let mut engine = self.engine.borrow_mut();
        engine.ensure_resource_revision();
        let (_, metrics, width, text_align) = text_config(
            &engine.font_system,
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            0.0,
            max_width,
            align,
        );
        let params = TextLayoutParams::new(
            text,
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            max_width,
            align,
            engine.measurer_revision,
        );
        let cosmic = engine.take_layout(&params, metrics, width, text_align);

        let base_x = x as i32;
        let base_y = y as i32;
        let (clip_x, clip_y, clip_w, clip_h) = clip;
        let clip_rect = Rect::from_xywh(clip_x as f32, clip_y as f32, clip_w as f32, clip_h as f32);

        let save_count = canvas.save();
        canvas.clip_rect(clip_rect, None, false);
        let resource_revision = engine.resource_revision;
        {
            let TextEngine {
                font_system,
                swash_cache,
                ..
            } = &mut *engine;
            GLYPH_ATLAS.with(|atlas_cell| {
                let mut atlas = atlas_cell.borrow_mut();
                let mut renderer = SkiaGlyphRenderer {
                    font_system,
                    swash_cache,
                    atlas: &mut atlas,
                    canvas,
                    base_x,
                    base_y,
                    resource_revision,
                };
                cosmic.render(&mut renderer, cosmic_color(color));
            });
        }
        canvas.restore_to_count(save_count);
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
        self.measure_styled_with_font_style(
            text,
            font_family,
            font_size,
            font_weight,
            FontStyle::Normal,
            line_height,
            max_width,
        )
    }

    pub fn measure_styled_with_font_style(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        font_style: FontStyle,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        let mut context = TextMeasureContext::new(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
        );
        context.font_style = font_style;
        self.measure_text_context(&context)
    }

    pub fn measure_text_context(&self, context: &TextMeasureContext<'_>) -> (f32, f32) {
        let mut engine = self.engine.borrow_mut();
        engine.ensure_resource_revision();
        let (_, metrics, width, _) = text_config(
            &engine.font_system,
            context.font_family,
            context.font_size,
            context.font_weight,
            context.font_style,
            context.line_height,
            context.letter_spacing,
            context.max_width,
            TextAlign::Left,
        );
        let params = TextLayoutParams::from_context(
            context,
            TextAlign::Left,
            engine.resource_revision,
            engine.measurer_revision,
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

    pub fn truncate_with_ellipsis_shaped(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: f32,
    ) -> Option<String> {
        self.truncate_with_ellipsis_shaped_with_font_style(
            text,
            font_family,
            font_size,
            font_weight,
            FontStyle::Normal,
            line_height,
            max_width,
        )
    }

    pub fn truncate_with_ellipsis_shaped_with_font_style(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        font_style: FontStyle,
        line_height: f32,
        max_width: f32,
    ) -> Option<String> {
        if text.is_empty() || text.contains('\n') {
            return None;
        }

        const ELLIPSIS: &str = "…";

        let mut engine = self.engine.borrow_mut();
        engine.ensure_resource_revision();
        let (_, metrics, width, align) = text_config(
            &engine.font_system,
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            0.0,
            None,
            TextAlign::Left,
        );

        let ellipsis_params = TextLayoutParams::new(
            ELLIPSIS,
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            None,
            TextAlign::Left,
            engine.measurer_revision,
        );
        let mut ellipsis_layout = engine.take_layout(&ellipsis_params, metrics, width, align);
        let ellipsis_width = {
            let ellipsis_layout = ellipsis_layout.borrow_with(&mut engine.font_system);
            ellipsis_layout
                .layout_runs()
                .map(|run| run.line_w)
                .fold(0.0f32, f32::max)
        };
        engine.store_layout(&ellipsis_params, ellipsis_layout);

        let target = (max_width - ellipsis_width).max(0.0);
        let params = TextLayoutParams::new(
            text,
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            None,
            TextAlign::Left,
            engine.measurer_revision,
        );
        let mut cosmic = engine.take_layout(&params, metrics, width, align);
        let split = {
            let cosmic = cosmic.borrow_with(&mut engine.font_system);
            let mut runs = cosmic.layout_runs();
            match runs.next() {
                Some(run) if !run.rtl && runs.next().is_none() => {
                    let mut best_end = 0usize;
                    for glyph in run.glyphs {
                        if glyph.x + glyph.w <= target {
                            best_end = glyph.end;
                        } else {
                            break;
                        }
                    }
                    Some(best_end)
                }
                _ => None,
            }
        };
        engine.store_layout(&params, cosmic);
        let split = split?;

        let mut output = String::with_capacity(split + ELLIPSIS.len());
        output.push_str(&text[..split]);
        output.push_str(ELLIPSIS);
        Some(output)
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
        self.selection_geometry_with_font_style(
            text,
            font_family,
            font_size,
            font_weight,
            FontStyle::Normal,
            line_height,
            align,
            max_width,
            anchor,
            focus,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn selection_geometry_with_font_style(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        font_style: FontStyle,
        line_height: f32,
        align: TextAlign,
        max_width: Option<f32>,
        anchor: (f32, f32),
        focus: (f32, f32),
    ) -> Option<TextSelectionGeometry> {
        let mut engine = self.engine.borrow_mut();
        engine.ensure_resource_revision();
        let (_, metrics, width, text_align) = text_config(
            &engine.font_system,
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            0.0,
            max_width,
            align,
        );
        let params = TextLayoutParams::new(
            text,
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            max_width,
            align,
            engine.measurer_revision,
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

/// Cosmic-text `Renderer` impl that draws each shaped glyph onto a Skia
/// canvas via the thread-local glyph atlas. Misses pay one swash
/// rasterization plus one `images::raster_from_data` to upload an A8 (or
/// RGBA8888 for color emoji) mask; hits reuse the cached Skia image and
/// only emit a `draw_image_rect` call.
struct SkiaGlyphRenderer<'a> {
    font_system: &'a mut FontSystem,
    swash_cache: &'a mut SwashCache,
    atlas: &'a mut ByteLruCache<GlyphAtlasKey, Option<GlyphAtlasEntry>>,
    canvas: &'a Canvas,
    base_x: i32,
    base_y: i32,
    resource_revision: u64,
}

impl Renderer for SkiaGlyphRenderer<'_> {
    fn rectangle(&mut self, x: i32, y: i32, w: u32, h: u32, color: cosmic_text::Color) {
        let (r, g, b, a) = color.as_rgba_tuple();
        if a == 0 || w == 0 || h == 0 {
            return;
        }
        let mut paint = Paint::default();
        paint.set_anti_alias(false);
        paint.set_argb(a, r, g, b);
        let rect = Rect::from_xywh(
            (self.base_x + x) as f32,
            (self.base_y + y) as f32,
            w as f32,
            h as f32,
        );
        self.canvas.draw_rect(rect, &paint);
    }

    fn glyph(&mut self, physical_glyph: PhysicalGlyph, color: cosmic_text::Color) {
        let (r, g, b, a) = color.as_rgba_tuple();
        if a == 0 {
            return;
        }
        let cache_key = GlyphAtlasKey {
            resource_revision: self.resource_revision,
            cache_key: physical_glyph.cache_key,
        };
        let needs_build = self.atlas.get(&cache_key).is_none();
        if needs_build {
            let entry =
                build_glyph_atlas_entry(self.font_system, self.swash_cache, cache_key.cache_key);
            let weight = entry.as_ref().map_or(1, |entry| entry.bytes);
            self.atlas.insert(cache_key, entry, weight);
        }
        let Some(Some(entry)) = self.atlas.get(&cache_key) else {
            return;
        };
        let dest_x = (self.base_x + physical_glyph.x + entry.placement_left) as f32;
        // SwashImage placement.top is the distance from baseline up to the
        // top of the bitmap; cosmic-text already includes baseline in
        // physical_glyph.y, so subtract the placement.
        let dest_y = (self.base_y + physical_glyph.y - entry.placement_top) as f32;
        let (img_w, img_h) = (entry.image.width() as f32, entry.image.height() as f32);
        let dest = Rect::from_xywh(dest_x, dest_y, img_w, img_h);

        let mut paint = Paint::default();
        paint.set_anti_alias(false);
        if entry.is_color {
            // Color emoji: image carries its own RGB. Modulate alpha only.
            paint.set_argb(a, 0xff, 0xff, 0xff);
        } else {
            // Monochrome mask: paint color becomes the glyph color; the
            // image's A8 alpha modulates it.
            paint.set_argb(a, r, g, b);
        }
        self.canvas.draw_image_rect_with_sampling_options(
            &entry.image,
            None,
            dest,
            SamplingOptions::default(),
            &paint,
        );
    }
}

fn build_glyph_atlas_entry(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    cache_key: CacheKey,
) -> Option<GlyphAtlasEntry> {
    let image = swash_cache.get_image(font_system, cache_key).as_ref()?;
    let width = image.placement.width;
    let height = image.placement.height;
    if width == 0
        || height == 0
        || width > MAX_GLYPH_ATLAS_DIMENSION
        || height > MAX_GLYPH_ATLAS_DIMENSION
        || image.data.is_empty()
    {
        return None;
    }
    let (info, bytes_per_pixel) = match image.content {
        SwashContent::Mask => (
            ImageInfo::new(
                (width as i32, height as i32),
                ColorType::Alpha8,
                AlphaType::Premul,
                None,
            ),
            1,
        ),
        SwashContent::Color => (
            ImageInfo::new(
                (width as i32, height as i32),
                ColorType::RGBA8888,
                AlphaType::Premul,
                None,
            ),
            4,
        ),
        SwashContent::SubpixelMask => return None,
    };
    let bytes = glyph_atlas_storage_bytes(width, height, bytes_per_pixel)?;
    let row_bytes = checked_pixel_bytes(width, 1, bytes_per_pixel)?;
    if image.data.len() != bytes {
        return None;
    }
    let data = Data::new_copy(image.data.as_slice());
    let sk_image = images::raster_from_data(&info, data, row_bytes)?;
    Some(GlyphAtlasEntry {
        image: sk_image,
        placement_left: image.placement.left,
        placement_top: image.placement.top,
        is_color: matches!(image.content, SwashContent::Color),
        bytes,
    })
}

fn glyph_atlas_storage_bytes(width: u32, height: u32, bytes_per_pixel: usize) -> Option<usize> {
    if width == 0
        || height == 0
        || width > MAX_GLYPH_ATLAS_DIMENSION
        || height > MAX_GLYPH_ATLAS_DIMENSION
    {
        return None;
    }
    let bytes = checked_pixel_bytes(width, height, bytes_per_pixel)?;
    (bytes <= GLYPH_ATLAS_MAX_BYTES).then_some(bytes)
}

impl TextEngine {
    fn set_font_database(&mut self, database: fontdb::Database) {
        self.font_database = Some(database.clone());
        self.measurer_revision = self.measurer_revision.saturating_add(1);
        self.font_system = FontSystem::new_with_locale_and_db(self.locale.clone(), database);
        self.swash_cache = SwashCache::new();
        self.layout_cache.clear();
        GLYPH_ATLAS.with(|atlas| atlas.borrow_mut().clear());
        NAMED_FONT_AVAILABILITY.with(|cache| cache.borrow_mut().clear());
        self.metrics.layout_invalidations = self.metrics.layout_invalidations.saturating_add(1);
    }

    fn set_font_aliases(&mut self, aliases: HashMap<String, String>) {
        if self.font_aliases == aliases {
            return;
        }
        self.font_aliases = aliases;
        self.measurer_revision = self.measurer_revision.saturating_add(1);
        self.layout_cache.clear();
        GLYPH_ATLAS.with(|atlas| atlas.borrow_mut().clear());
        self.metrics.layout_invalidations = self.metrics.layout_invalidations.saturating_add(1);
    }

    fn ensure_resource_revision(&mut self) {
        let revision = resource_revision();
        if self.resource_revision == revision {
            return;
        }

        self.resource_revision = revision;
        self.measurer_revision = self.measurer_revision.saturating_add(1);
        self.font_system = self
            .font_database
            .as_ref()
            .map(|database| {
                FontSystem::new_with_locale_and_db(self.locale.clone(), database.clone())
            })
            .unwrap_or_else(FontSystem::new);
        self.swash_cache = SwashCache::new();
        self.layout_cache.clear();
        GLYPH_ATLAS.with(|atlas| atlas.borrow_mut().clear());
        NAMED_FONT_AVAILABILITY.with(|cache| cache.borrow_mut().clear());
        self.metrics.layout_invalidations = self.metrics.layout_invalidations.saturating_add(1);
    }

    fn revisions(&mut self) -> TextMeasureRevisions {
        self.ensure_resource_revision();
        TextMeasureRevisions {
            resource_revision: self.resource_revision,
            measurer_revision: self.measurer_revision,
        }
    }

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
        let attrs = text_attrs(
            &self.font_system,
            &self.font_aliases,
            params.font_family,
            f32::from_bits(params.font_size),
            params.font_weight,
            params.font_style,
            f32::from_bits(params.letter_spacing),
        );
        let mut cosmic = Buffer::new(&mut self.font_system, metrics);
        {
            let mut cosmic_borrow = cosmic.borrow_with(&mut self.font_system);
            cosmic_borrow.set_wrap(wrap_for(
                params.max_width.map(f32::from_bits),
                params.white_space,
            ));
            cosmic_borrow.set_size(width, None);
            cosmic_borrow.set_text(
                params.text,
                &attrs.as_attrs(),
                Shaping::Advanced,
                Some(align),
            );
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
        let entry = TextLayoutEntry {
            resource_revision: params.resource_revision,
            measurer_revision: params.measurer_revision,
            text: params.text.to_string(),
            font_family: params.font_family.to_string(),
            font_size: params.font_size,
            font_weight: params.font_weight,
            font_style: params.font_style,
            letter_spacing: params.letter_spacing,
            line_height: params.line_height,
            text_direction: params.text_direction,
            white_space: params.white_space,
            language: params.language.to_owned(),
            shaping_features: params.shaping_features.to_owned(),
            max_width: params.max_width,
            align: params.align,
            buffer: cosmic,
        };
        let Some(weight) = entry.estimated_bytes() else {
            self.metrics.layout_invalidations = self.metrics.layout_invalidations.saturating_add(1);
            self.metrics.shaped_entries = self.layout_cache.len() as u64;
            self.metrics.layout_cache_bytes = self.layout_cache.bytes() as u64;
            self.metrics.layout_cache_max_bytes = self.layout_cache.max_bytes() as u64;
            return;
        };

        let previous_len = self.layout_cache.len();
        let inserted = self.layout_cache.insert(params.cache_key, entry, weight);
        let evicted = previous_len
            .saturating_add(usize::from(inserted))
            .saturating_sub(self.layout_cache.len());
        if evicted > 0 {
            self.metrics.layout_invalidations = self
                .metrics
                .layout_invalidations
                .saturating_add(evicted as u64);
        }
        self.metrics.shaped_entries = self.layout_cache.len() as u64;
        self.metrics.layout_cache_bytes = self.layout_cache.bytes() as u64;
        self.metrics.layout_cache_max_bytes = self.layout_cache.max_bytes() as u64;
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl mesh_core_elements::TextMeasurer for TextRenderer {
    fn measure_text(&self, context: &TextMeasureContext<'_>) -> (f32, f32) {
        self.measure_text_context(context)
    }

    fn revisions(&self) -> TextMeasureRevisions {
        let mut engine = self.engine.borrow_mut();
        engine.revisions()
    }
}

impl mesh_core_elements::TextMeasurer for SharedTextMeasurer {
    fn measure_text(&self, context: &TextMeasureContext<'_>) -> (f32, f32) {
        RENDERER.with(|renderer| renderer.borrow().measure_text_context(context))
    }

    fn revisions(&self) -> TextMeasureRevisions {
        RENDERER.with(|renderer| renderer.borrow().revisions())
    }
}

impl SharedTextMeasurer {
    pub fn measure_text_context(&self, context: &TextMeasureContext<'_>) -> (f32, f32) {
        RENDERER.with(|renderer| renderer.borrow().measure_text_context(context))
    }

    pub fn set_font_database(&self, database: fontdb::Database) {
        RENDERER.with(|renderer| renderer.borrow().set_font_database(database));
    }

    pub fn set_font_aliases(&self, aliases: HashMap<String, String>) {
        RENDERER.with(|renderer| renderer.borrow().set_font_aliases(aliases));
    }

    pub fn cache_metrics(&self) -> TextCacheMetrics {
        RENDERER.with(|renderer| renderer.borrow().cache_metrics())
    }

    pub fn reset_cache_metrics(&self) {
        RENDERER.with(|renderer| renderer.borrow().reset_cache_metrics());
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

    pub fn measure_styled_with_font_style(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        font_style: FontStyle,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        RENDERER.with(|renderer| {
            renderer.borrow().measure_styled_with_font_style(
                text,
                font_family,
                font_size,
                font_weight,
                font_style,
                line_height,
                max_width,
            )
        })
    }

    pub fn truncate_with_ellipsis_shaped(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: f32,
    ) -> Option<String> {
        RENDERER.with(|renderer| {
            renderer.borrow().truncate_with_ellipsis_shaped(
                text,
                font_family,
                font_size,
                font_weight,
                line_height,
                max_width,
            )
        })
    }

    pub fn truncate_with_ellipsis_shaped_with_font_style(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        font_style: FontStyle,
        line_height: f32,
        max_width: f32,
    ) -> Option<String> {
        RENDERER.with(|renderer| {
            renderer
                .borrow()
                .truncate_with_ellipsis_shaped_with_font_style(
                    text,
                    font_family,
                    font_size,
                    font_weight,
                    font_style,
                    line_height,
                    max_width,
                )
        })
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
        RENDERER.with(|renderer| {
            renderer.borrow().render_clipped(
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
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_clipped_with_font_style(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        font_style: FontStyle,
        line_height: f32,
        align: TextAlign,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
        clip: (u32, u32, u32, u32),
        max_width: Option<f32>,
    ) {
        RENDERER.with(|renderer| {
            renderer.borrow().render_clipped_with_font_style(
                text,
                font_family,
                font_size,
                font_weight,
                font_style,
                line_height,
                align,
                color,
                buffer,
                x,
                y,
                clip,
                max_width,
            );
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_clipped_on_canvas(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        align: TextAlign,
        color: Color,
        canvas: &Canvas,
        x: u32,
        y: u32,
        clip: (u32, u32, u32, u32),
        max_width: Option<f32>,
    ) {
        RENDERER.with(|renderer| {
            renderer.borrow().render_clipped_on_canvas(
                text,
                font_family,
                font_size,
                font_weight,
                line_height,
                align,
                color,
                canvas,
                x,
                y,
                clip,
                max_width,
            );
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_clipped_on_canvas_with_font_style(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        font_style: FontStyle,
        line_height: f32,
        align: TextAlign,
        color: Color,
        canvas: &Canvas,
        x: u32,
        y: u32,
        clip: (u32, u32, u32, u32),
        max_width: Option<f32>,
    ) {
        RENDERER.with(|renderer| {
            renderer.borrow().render_clipped_on_canvas_with_font_style(
                text,
                font_family,
                font_size,
                font_weight,
                font_style,
                line_height,
                align,
                color,
                canvas,
                x,
                y,
                clip,
                max_width,
            );
        });
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
        RENDERER.with(|renderer| {
            renderer.borrow().selection_geometry(
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
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn selection_geometry_with_font_style(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        font_style: FontStyle,
        line_height: f32,
        align: TextAlign,
        max_width: Option<f32>,
        anchor: (f32, f32),
        focus: (f32, f32),
    ) -> Option<TextSelectionGeometry> {
        RENDERER.with(|renderer| {
            renderer.borrow().selection_geometry_with_font_style(
                text,
                font_family,
                font_size,
                font_weight,
                font_style,
                line_height,
                align,
                max_width,
                anchor,
                focus,
            )
        })
    }
}

fn text_config<'a>(
    font_system: &FontSystem,
    font_family: &'a str,
    font_size: f32,
    font_weight: u16,
    font_style: FontStyle,
    line_height: f32,
    letter_spacing: f32,
    max_width: Option<f32>,
    align: TextAlign,
) -> (Attrs<'a>, Metrics, Option<f32>, Align) {
    let family = primary_family(font_system, font_family);
    let attrs = Attrs::new()
        .family(family)
        .style(cosmic_font_style(font_style))
        .weight(Weight(font_weight.max(100)))
        .letter_spacing(letter_spacing_em(letter_spacing, font_size));
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

fn text_attrs(
    font_system: &FontSystem,
    aliases: &HashMap<String, String>,
    font_family: &str,
    font_size: f32,
    font_weight: u16,
    font_style: FontStyle,
    letter_spacing: f32,
) -> AttrsOwned {
    let family = primary_family_owned(font_system, aliases, font_family);
    let attrs = Attrs::new()
        .family(family.as_family())
        .style(cosmic_font_style(font_style))
        .weight(Weight(font_weight.max(100)))
        .letter_spacing(letter_spacing_em(letter_spacing, font_size));
    AttrsOwned::new(&attrs)
}

fn letter_spacing_em(letter_spacing: f32, font_size: f32) -> f32 {
    if letter_spacing.is_finite() && font_size.is_finite() && font_size.abs() > f32::EPSILON {
        letter_spacing / font_size
    } else {
        0.0
    }
}

fn cosmic_font_style(font_style: FontStyle) -> CosmicStyle {
    match font_style {
        FontStyle::Normal => CosmicStyle::Normal,
        FontStyle::Italic => CosmicStyle::Italic,
    }
}

fn primary_family_owned(
    font_system: &FontSystem,
    aliases: &HashMap<String, String>,
    font_family: &str,
) -> FamilyOwned {
    for family in font_family
        .split(',')
        .map(|part| part.trim().trim_matches('"').trim_matches('\''))
        .filter(|part| !part.is_empty())
    {
        let family = aliases.get(family).map(String::as_str).unwrap_or(family);
        match family.to_ascii_lowercase().as_str() {
            "serif" => return FamilyOwned::Serif,
            "sans-serif" | "sans" | "system-ui" => return FamilyOwned::SansSerif,
            "monospace" | "mono" => return FamilyOwned::Monospace,
            "cursive" => return FamilyOwned::Cursive,
            "fantasy" => return FamilyOwned::Fantasy,
            _ if named_family_is_available(font_system, family) => {
                return FamilyOwned::new(Family::Name(family));
            }
            _ => {}
        }
    }

    FamilyOwned::new(fallback_text_family(font_system))
}

fn primary_family<'a>(font_system: &FontSystem, font_family: &'a str) -> Family<'a> {
    for family in font_family
        .split(',')
        .map(|part| part.trim().trim_matches('"').trim_matches('\''))
        .filter(|part| !part.is_empty())
    {
        match family.to_ascii_lowercase().as_str() {
            "serif" => return Family::Serif,
            "sans-serif" | "sans" | "system-ui" => return Family::SansSerif,
            "monospace" | "mono" => return Family::Monospace,
            "cursive" => return Family::Cursive,
            "fantasy" => return Family::Fantasy,
            _ if named_family_is_available(font_system, family) => return Family::Name(family),
            _ => {}
        }
    }

    fallback_text_family(font_system)
}

fn fallback_text_family(font_system: &FontSystem) -> Family<'static> {
    // cosmic-text maps generic sans-serif to Open Sans. When Open Sans is
    // absent, fontdb can pick an arbitrary installed face, including an icon
    // font. Prefer a verified ordinary text family first.
    const TEXT_FALLBACKS: &[&str] = &[
        "Inter",
        "Noto Sans",
        "DejaVu Sans",
        "Liberation Sans",
        "Ubuntu",
        "Arial",
    ];

    TEXT_FALLBACKS
        .iter()
        .copied()
        .find(|family| named_family_is_available(font_system, family))
        .map(Family::Name)
        .unwrap_or(Family::SansSerif)
}

fn named_family_is_available(font_system: &FontSystem, family: &str) -> bool {
    NAMED_FONT_AVAILABILITY.with(|cache| {
        if let Some(available) = cache.borrow().get(family).copied() {
            return available;
        }
        let available = font_system.db().faces().any(|face| {
            face.families
                .iter()
                .any(|(candidate, _)| candidate.eq_ignore_ascii_case(family))
        });
        let mut cache = cache.borrow_mut();
        if cache.len() == NAMED_FONT_AVAILABILITY_CACHE_CAPACITY {
            cache.clear();
        }
        cache.insert(family.to_owned(), available);
        available
    })
}

#[cfg(test)]
mod font_family_tests {
    use super::{Family, FontSystem, primary_family};

    #[test]
    fn unavailable_named_family_does_not_remain_the_selected_face() {
        let font_system = FontSystem::new();
        match primary_family(&font_system, "__mesh_missing_font__") {
            Family::Name(family) => assert_ne!(family, "__mesh_missing_font__"),
            Family::SansSerif => {}
            other => panic!("expected a sans-serif fallback, got {other:?}"),
        }
    }
}

fn wrap_for(max_width: Option<f32>, white_space: WhiteSpace) -> Wrap {
    if max_width.is_some() && white_space != WhiteSpace::Nowrap {
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
    fn text_cache_misses_when_font_style_changes() {
        let renderer = TextRenderer::new();
        renderer.reset_cache_metrics();

        renderer.measure_styled_with_font_style(
            "styled text",
            "Inter",
            14.0,
            400,
            FontStyle::Normal,
            1.2,
            Some(120.0),
        );
        renderer.measure_styled_with_font_style(
            "styled text",
            "Inter",
            14.0,
            400,
            FontStyle::Italic,
            1.2,
            Some(120.0),
        );
        let metrics = renderer.cache_metrics();

        assert_eq!(metrics.layout_misses, 2);
        assert_eq!(metrics.layout_hits, 0);
        assert_eq!(metrics.shaped_entries, 2);
    }

    #[test]
    fn text_layout_cache_reports_bounded_resident_bytes() {
        let renderer = TextRenderer::new();
        renderer.reset_cache_metrics();

        renderer.measure_styled("cached text", "Inter", 14.0, 400, 1.2, Some(120.0));
        let metrics = renderer.cache_metrics();

        assert_eq!(metrics.shaped_entries, 1);
        assert!(metrics.layout_cache_bytes > 0);
        assert_eq!(
            metrics.layout_cache_max_bytes,
            TEXT_LAYOUT_CACHE_MAX_BYTES as u64
        );
        assert!(metrics.layout_cache_bytes <= metrics.layout_cache_max_bytes);
    }

    #[test]
    fn oversized_text_layouts_are_not_admitted_to_the_cache() {
        let buffer = Buffer::new_empty(Metrics::new(14.0, 18.0));

        assert!(estimated_text_layout_bytes(32, 8, &buffer).is_some());
        assert!(estimated_text_layout_bytes(MAX_TEXT_LAYOUT_TEXT_BYTES + 1, 8, &buffer).is_none());
        assert!(
            estimated_text_layout_bytes(32, MAX_TEXT_LAYOUT_FAMILY_BYTES + 1, &buffer).is_none()
        );
    }

    #[test]
    fn text_layout_cache_key_includes_resource_revision() {
        let base = text_layout_cache_key(
            "revisioned text",
            "Inter",
            14.0f32.to_bits(),
            400,
            FontStyle::Normal,
            0.0f32.to_bits(),
            1.2f32.to_bits(),
            TextDirection::Ltr,
            WhiteSpace::Normal,
            "",
            "",
            Some(120.0f32.to_bits()),
            TextAlign::Left,
            7,
            0,
        );
        let changed = text_layout_cache_key(
            "revisioned text",
            "Inter",
            14.0f32.to_bits(),
            400,
            FontStyle::Normal,
            0.0f32.to_bits(),
            1.2f32.to_bits(),
            TextDirection::Ltr,
            WhiteSpace::Normal,
            "",
            "",
            Some(120.0f32.to_bits()),
            TextAlign::Left,
            8,
            0,
        );
        assert_ne!(base, changed);
    }

    #[test]
    fn text_layout_cache_key_includes_font_style() {
        let normal = text_layout_cache_key(
            "styled text",
            "Inter",
            14.0f32.to_bits(),
            400,
            FontStyle::Normal,
            0.0f32.to_bits(),
            1.2f32.to_bits(),
            TextDirection::Ltr,
            WhiteSpace::Normal,
            "",
            "",
            Some(120.0f32.to_bits()),
            TextAlign::Left,
            7,
            0,
        );
        let italic = text_layout_cache_key(
            "styled text",
            "Inter",
            14.0f32.to_bits(),
            400,
            FontStyle::Italic,
            0.0f32.to_bits(),
            1.2f32.to_bits(),
            TextDirection::Ltr,
            WhiteSpace::Normal,
            "",
            "",
            Some(120.0f32.to_bits()),
            TextAlign::Left,
            7,
            0,
        );
        assert_ne!(normal, italic);
    }

    #[test]
    fn text_layout_cache_key_includes_complete_measure_context() {
        let mut context =
            TextMeasureContext::new("context text", "Inter", 14.0, 400, 1.2, Some(120.0));
        let base = TextLayoutParams::from_context(&context, TextAlign::Left, 7, 3).cache_key;

        context.letter_spacing = 0.5;
        assert_ne!(
            base,
            TextLayoutParams::from_context(&context, TextAlign::Left, 7, 3).cache_key
        );
        context.letter_spacing = 0.0;
        context.text_direction = TextDirection::Rtl;
        assert_ne!(
            base,
            TextLayoutParams::from_context(&context, TextAlign::Left, 7, 3).cache_key
        );
        context.text_direction = TextDirection::Ltr;
        context.white_space = WhiteSpace::Nowrap;
        assert_ne!(
            base,
            TextLayoutParams::from_context(&context, TextAlign::Left, 7, 3).cache_key
        );
        context.white_space = WhiteSpace::Normal;
        context.language = "ar";
        assert_ne!(
            base,
            TextLayoutParams::from_context(&context, TextAlign::Left, 7, 3).cache_key
        );
        context.language = "";
        context.shaping_features = "liga=0";
        assert_ne!(
            base,
            TextLayoutParams::from_context(&context, TextAlign::Left, 7, 3).cache_key
        );
        assert_ne!(
            base,
            TextLayoutParams::from_context(&context, TextAlign::Left, 7, 4).cache_key
        );
    }

    #[test]
    fn glyph_atlas_key_includes_resource_revision() {
        let (cache_key, _, _) = CacheKey::new(
            fontdb::ID::dummy(),
            1,
            14.0,
            (0.0, 0.0),
            Weight::NORMAL,
            cosmic_text::CacheKeyFlags::empty(),
        );
        let base = GlyphAtlasKey {
            resource_revision: 7,
            cache_key,
        };
        let changed = GlyphAtlasKey {
            resource_revision: 8,
            cache_key,
        };

        assert_ne!(base, changed);

        let mut atlas = ByteLruCache::new(GLYPH_ATLAS_CAPACITY, GLYPH_ATLAS_MAX_BYTES);
        assert!(atlas.insert(base, None::<GlyphAtlasEntry>, 1));
        assert!(atlas.get(&base).is_some());
        assert!(atlas.get(&changed).is_none());
    }

    #[test]
    fn glyph_atlas_storage_rejects_oversized_or_overflowing_images() {
        assert_eq!(glyph_atlas_storage_bytes(16, 16, 1), Some(256));
        assert_eq!(
            glyph_atlas_storage_bytes(MAX_GLYPH_ATLAS_DIMENSION, MAX_GLYPH_ATLAS_DIMENSION, 4),
            None
        );
        assert_eq!(
            glyph_atlas_storage_bytes(MAX_GLYPH_ATLAS_DIMENSION + 1, 1, 1),
            None
        );
        assert_eq!(
            glyph_atlas_storage_bytes(
                MAX_GLYPH_ATLAS_DIMENSION,
                MAX_GLYPH_ATLAS_DIMENSION,
                usize::MAX
            ),
            None
        );
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
