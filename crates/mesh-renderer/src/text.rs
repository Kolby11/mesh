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
        self.render_clipped(
            text,
            font_size,
            color,
            buffer,
            x,
            y,
            (0, 0, buffer.width, buffer.height),
        );
    }

    pub fn render_clipped(
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

            self.render_glyph(ch, color, buffer, cursor_x, cursor_y, scale, clip);
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
