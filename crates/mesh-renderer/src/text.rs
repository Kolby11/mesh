/// Text measurement and rendering.
use crate::buffer::PixelBuffer;
use font8x8::{BASIC_FONTS, UnicodeFonts};
use fontdb::{Database, Family, Query, Stretch, Style, Weight};
use fontdue::layout::{
    CoordinateSystem, GlyphRasterConfig, HorizontalAlign, Layout, LayoutSettings, TextStyle,
};
use fontdue::{Font, FontSettings};
use mesh_ui::Color;
use mesh_ui::style::TextAlign;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Handles font loading, text measurement, and glyph rendering.
pub struct TextRenderer {
    font_db: Database,
    font_cache: Mutex<HashMap<FontRequest, Option<Arc<Font>>>>,
    glyph_cache: Mutex<HashMap<GlyphRasterConfig, Arc<Vec<u8>>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontRequest {
    family: String,
    weight: u16,
}

impl TextRenderer {
    pub fn new() -> Self {
        let mut font_db = Database::new();
        font_db.load_system_fonts();
        Self {
            font_db,
            font_cache: Mutex::new(HashMap::new()),
            glyph_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Measure text dimensions without rendering.
    pub fn measure(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        self.measure_styled(text, font_family, font_size, 400, 1.0, max_width)
    }

    /// Render text into a pixel buffer using an embedded bitmap font.
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
        let Some((font, layout)) = self.layout_text(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            x,
            y,
            max_width,
            align,
        ) else {
            self.render_bitmap_fallback(text, font_size, color, buffer, x, y, clip);
            return;
        };

        let (clip_x, clip_y, clip_w, clip_h) = clip;
        let clip_right = clip_x.saturating_add(clip_w);
        let clip_bottom = clip_y.saturating_add(clip_h);

        for glyph in layout.glyphs() {
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }

            let bitmap = {
                let mut cache = self.glyph_cache.lock().unwrap();
                cache
                    .entry(glyph.key)
                    .or_insert_with(|| {
                        let (_metrics, bitmap) = font.rasterize_config(glyph.key);
                        Arc::new(bitmap)
                    })
                    .clone()
            };

            let draw_x = glyph.x.round().max(0.0) as u32;
            let draw_y = glyph.y.round().max(0.0) as u32;

            for row in 0..glyph.height {
                for col in 0..glyph.width {
                    let coverage = bitmap[row * glyph.width + col];
                    if coverage == 0 {
                        continue;
                    }

                    let px = draw_x.saturating_add(col as u32);
                    let py = draw_y.saturating_add(row as u32);
                    if px < clip_x || py < clip_y || px >= clip_right || py >= clip_bottom {
                        continue;
                    }

                    buffer.blend_pixel(px, py, color, coverage);
                }
            }
        }
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
        let Some((_font, layout)) = self.layout_text(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            0,
            0,
            max_width,
            TextAlign::Left,
        ) else {
            return bitmap_measure(text, font_size, max_width);
        };

        let mut max_x = 0.0f32;
        let mut min_x = f32::MAX;
        for glyph in layout.glyphs() {
            min_x = min_x.min(glyph.x);
            max_x = max_x.max(glyph.x + glyph.width as f32);
        }
        let width = if layout.glyphs().is_empty() {
            0.0
        } else {
            (max_x - min_x.min(0.0)).max(0.0)
        };
        let height = layout.height().max(font_size.max(1.0));
        (width, height)
    }

    fn layout_text(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        x: u32,
        y: u32,
        max_width: Option<f32>,
        align: TextAlign,
    ) -> Option<(Arc<Font>, Layout)> {
        let font = self.resolve_font(font_family, font_weight)?;
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            x: x as f32,
            y: y as f32,
            max_width,
            max_height: None,
            horizontal_align: match align {
                TextAlign::Center => HorizontalAlign::Center,
                TextAlign::Right => HorizontalAlign::Right,
                TextAlign::Left => HorizontalAlign::Left,
            },
            vertical_align: fontdue::layout::VerticalAlign::Top,
            line_height: line_height.max(1.0),
            ..LayoutSettings::default()
        });
        let fonts = [font.as_ref()];
        layout.append(&fonts, &TextStyle::new(text, font_size.max(1.0), 0));
        Some((font, layout))
    }

    fn resolve_font(&self, font_family: &str, font_weight: u16) -> Option<Arc<Font>> {
        let request = FontRequest {
            family: font_family.to_string(),
            weight: font_weight,
        };
        if let Some(cached) = self.font_cache.lock().unwrap().get(&request).cloned() {
            return cached;
        }

        let family_names = parsed_families(font_family);
        let families = query_families(&family_names);
        let query = Query {
            families: &families,
            weight: Weight(font_weight.max(100)),
            stretch: Stretch::Normal,
            style: Style::Normal,
        };

        let font = self
            .font_db
            .query(&query)
            .and_then(|id| {
                self.font_db.with_face_data(id, |data, index| {
                    Font::from_bytes(
                        data.to_vec(),
                        FontSettings {
                            collection_index: index,
                            ..FontSettings::default()
                        },
                    )
                    .ok()
                    .map(Arc::new)
                })
            })
            .flatten();

        self.font_cache
            .lock()
            .unwrap()
            .insert(request, font.clone());
        font
    }

    fn render_bitmap_fallback(
        &self,
        text: &str,
        font_size: f32,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
        clip: (u32, u32, u32, u32),
    ) {
        let scale = glyph_scale(font_size);
        let glyph_advance = 8 * scale + scale;
        let mut cursor_x = x;
        let mut cursor_y = y;

        for ch in text.chars() {
            if ch == '\n' {
                cursor_x = x;
                cursor_y = cursor_y.saturating_add(8 * scale + scale);
                continue;
            }

            self.render_bitmap_glyph(ch, color, buffer, cursor_x, cursor_y, scale, clip);
            cursor_x = cursor_x.saturating_add(glyph_advance);
        }
    }

    fn render_bitmap_glyph(
        &self,
        ch: char,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
        scale: u32,
        clip: (u32, u32, u32, u32),
    ) {
        let glyph = BASIC_FONTS.get(ch).or_else(|| BASIC_FONTS.get('?'));
        let Some(glyph) = glyph else {
            return;
        };

        let (clip_x, clip_y, clip_w, clip_h) = clip;
        let clip_right = clip_x.saturating_add(clip_w);
        let clip_bottom = clip_y.saturating_add(clip_h);

        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8u32 {
                if (bits >> col) & 1 == 0 {
                    continue;
                }

                let px = x.saturating_add(col * scale);
                let py = y.saturating_add(row as u32 * scale);
                for sy in 0..scale {
                    for sx in 0..scale {
                        let draw_x = px.saturating_add(sx);
                        let draw_y = py.saturating_add(sy);
                        if draw_x < clip_x
                            || draw_y < clip_y
                            || draw_x >= clip_right
                            || draw_y >= clip_bottom
                        {
                            continue;
                        }
                        buffer.set_pixel(draw_x, draw_y, color);
                    }
                }
            }
        }
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn glyph_scale(font_size: f32) -> u32 {
    let scaled = (font_size / 8.0).round().max(1.0);
    scaled as u32
}

fn bitmap_measure(text: &str, font_size: f32, max_width: Option<f32>) -> (f32, f32) {
    let scale = glyph_scale(font_size);
    let glyph_w = 8.0 * scale as f32;
    let glyph_h = 8.0 * scale as f32;
    let raw_width = text.chars().count() as f32 * glyph_w;
    let width = match max_width {
        Some(max) if max > 0.0 => raw_width.min(max),
        _ => raw_width,
    };
    let lines = match max_width {
        Some(max) if max > 0.0 && raw_width > max => (raw_width / max).ceil(),
        _ => 1.0,
    };
    (width, glyph_h * lines)
}

fn parsed_families(font_family: &str) -> Vec<String> {
    let mut families: Vec<String> = font_family
        .split(',')
        .map(|part| part.trim().trim_matches('"').trim_matches('\''))
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();

    if families.is_empty() {
        families.push("sans-serif".into());
    }

    if !families
        .iter()
        .any(|family| family.eq_ignore_ascii_case("sans-serif"))
    {
        families.push("sans-serif".into());
    }

    families
}

fn query_families(families: &[String]) -> Vec<Family<'_>> {
    let mut query = Vec::with_capacity(families.len() + 2);
    for family in families {
        match family.trim().to_ascii_lowercase().as_str() {
            "serif" => query.push(Family::Serif),
            "sans-serif" | "sans" | "system-ui" => query.push(Family::SansSerif),
            "monospace" | "mono" => query.push(Family::Monospace),
            "cursive" => query.push(Family::Cursive),
            "fantasy" => query.push(Family::Fantasy),
            _ => query.push(Family::Name(family.as_str())),
        }
    }
    query
}
