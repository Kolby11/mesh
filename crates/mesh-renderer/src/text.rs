/// Text measurement and rendering.
///
/// This is a stub that will be replaced with a real implementation
/// using `cosmic-text` or `fontdue` for glyph rasterization.
use crate::buffer::PixelBuffer;
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
        // Stub: estimate width as ~0.6 * font_size per character.
        let char_width = font_size * 0.6;
        let raw_width = text.len() as f32 * char_width;
        let width = match max_width {
            Some(max) => raw_width.min(max),
            None => raw_width,
        };
        let lines = if max_width.is_some() && raw_width > width {
            (raw_width / width).ceil()
        } else {
            1.0
        };
        let height = lines * font_size * 1.4;
        (width, height)
    }

    /// Render text into a pixel buffer.
    ///
    /// Stub implementation: draws a colored rectangle where text would go.
    /// Real implementation will use `cosmic-text` for proper glyph rasterization.
    pub fn render(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
    ) {
        let (w, h) = self.measure(text, font_family, font_size, None);
        // Stub: fill a rectangle as a placeholder for text.
        // In a real implementation, this would rasterize glyphs.
        let mut faded = color;
        faded.a = faded.a / 3; // Make it lighter to indicate it's a stub.
        buffer.fill_rect(x, y, w as u32, h as u32, faded);
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}
