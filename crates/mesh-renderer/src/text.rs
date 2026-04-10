/// Text measurement and rendering.
use crate::buffer::PixelBuffer;
use font8x8::{BASIC_FONTS, UnicodeFonts};
use mesh_ui::Color;

/// Handles font loading, text measurement, and glyph rendering.
pub struct TextRenderer;

impl TextRenderer {
    pub fn new() -> Self {
        Self
    }

    /// Measure text dimensions without rendering.
    pub fn measure(
        &self,
        text: &str,
        _font_family: &str,
        font_size: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
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

    /// Render text into a pixel buffer using an embedded bitmap font.
    pub fn render(
        &self,
        text: &str,
        _font_family: &str,
        font_size: f32,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
    ) {
        let scale = glyph_scale(font_size);
        let glyph_advance = 8 * scale + scale;
        let mut cursor_x = x;

        for ch in text.chars() {
            if ch == '\n' {
                cursor_x = x;
                continue;
            }

            self.render_glyph(ch, color, buffer, cursor_x, y, scale);
            cursor_x = cursor_x.saturating_add(glyph_advance);
        }
    }

    fn render_glyph(
        &self,
        ch: char,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
        scale: u32,
    ) {
        let glyph = BASIC_FONTS.get(ch).or_else(|| BASIC_FONTS.get('?'));
        let Some(glyph) = glyph else {
            return;
        };

        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8u32 {
                if (bits >> col) & 1 == 0 {
                    continue;
                }

                let px = x.saturating_add(col * scale);
                let py = y.saturating_add(row as u32 * scale);
                for sy in 0..scale {
                    for sx in 0..scale {
                        buffer.set_pixel(px.saturating_add(sx), py.saturating_add(sy), color);
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
