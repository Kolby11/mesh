/// Pixel buffer for software rendering.
use mesh_ui::style::Color;

/// An ARGB8888 pixel buffer.
#[derive(Debug, Clone)]
pub struct PixelBuffer {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

impl PixelBuffer {
    /// Create a new buffer filled with transparent black.
    pub fn new(width: u32, height: u32) -> Self {
        let stride = width * 4;
        Self {
            data: vec![0u8; (stride * height) as usize],
            width,
            height,
            stride,
        }
    }

    /// Clear the buffer to a solid color.
    pub fn clear(&mut self, color: Color) {
        for pixel in self.data.chunks_exact_mut(4) {
            pixel[0] = color.b;
            pixel[1] = color.g;
            pixel[2] = color.r;
            pixel[3] = color.a;
        }
    }

    /// Set a single pixel. Coordinates are bounds-checked.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let offset = (y * self.stride + x * 4) as usize;
        if offset + 3 < self.data.len() {
            self.data[offset] = color.b;
            self.data[offset + 1] = color.g;
            self.data[offset + 2] = color.r;
            self.data[offset + 3] = color.a;
        }
    }

    /// Blend a single pixel using source alpha and an extra coverage value.
    pub fn blend_pixel(&mut self, x: u32, y: u32, color: Color, coverage: u8) {
        if x >= self.width || y >= self.height || coverage == 0 {
            return;
        }

        let offset = (y * self.stride + x * 4) as usize;
        if offset + 3 >= self.data.len() {
            return;
        }

        let src_alpha = (u16::from(color.a) * u16::from(coverage)) / 255;
        if src_alpha == 0 {
            return;
        }

        let inv_alpha = 255u16.saturating_sub(src_alpha);
        let dst_b = u16::from(self.data[offset]);
        let dst_g = u16::from(self.data[offset + 1]);
        let dst_r = u16::from(self.data[offset + 2]);
        let dst_a = u16::from(self.data[offset + 3]);

        self.data[offset] = ((u16::from(color.b) * src_alpha + dst_b * inv_alpha) / 255) as u8;
        self.data[offset + 1] = ((u16::from(color.g) * src_alpha + dst_g * inv_alpha) / 255) as u8;
        self.data[offset + 2] = ((u16::from(color.r) * src_alpha + dst_r * inv_alpha) / 255) as u8;
        self.data[offset + 3] = (src_alpha + ((dst_a * inv_alpha) / 255)).min(255) as u8;
    }

    /// Blend a pixel using a normalized floating-point coverage mask.
    pub fn blend_pixel_f32(&mut self, x: u32, y: u32, color: Color, coverage: f32) {
        let coverage = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
        self.blend_pixel(x, y, color, coverage);
    }

    /// Fill a rectangle with a solid color.
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        for py in y..y.saturating_add(h).min(self.height) {
            for px in x..x.saturating_add(w).min(self.width) {
                self.set_pixel(px, py, color);
            }
        }
    }

    /// Fill a rounded rectangle. Uses a simple distance check for corners.
    pub fn fill_rounded_rect(&mut self, x: u32, y: u32, w: u32, h: u32, radius: f32, color: Color) {
        let r = radius.min(w as f32 / 2.0).min(h as f32 / 2.0);
        let ri = r as u32;

        for py in y..y.saturating_add(h).min(self.height) {
            for px in x..x.saturating_add(w).min(self.width) {
                let lx = px - x;
                let ly = py - y;

                // Check if we're in a corner region.
                let in_corner = (lx < ri && ly < ri)
                    || (lx >= w - ri && ly < ri)
                    || (lx < ri && ly >= h - ri)
                    || (lx >= w - ri && ly >= h - ri);

                if in_corner {
                    // Find the center of the relevant corner circle.
                    let cx = if lx < ri { x + ri } else { x + w - ri - 1 } as f32;
                    let cy = if ly < ri { y + ri } else { y + h - ri - 1 } as f32;

                    let dx = px as f32 - cx;
                    let dy = py as f32 - cy;
                    if dx * dx + dy * dy <= r * r {
                        self.set_pixel(px, py, color);
                    }
                } else {
                    self.set_pixel(px, py, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_creation_and_clear() {
        let mut buf = PixelBuffer::new(10, 10);
        assert_eq!(buf.data.len(), 400);

        buf.clear(Color::WHITE);
        // Check first pixel is white (BGRA order).
        assert_eq!(&buf.data[0..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn fill_rect_bounds_checked() {
        let mut buf = PixelBuffer::new(10, 10);
        // Should not panic even with out-of-bounds rect.
        buf.fill_rect(8, 8, 20, 20, Color::WHITE);
    }

    #[test]
    fn blend_pixel_applies_coverage() {
        let mut buf = PixelBuffer::new(1, 1);
        buf.blend_pixel(0, 0, Color::WHITE, 128);
        assert_eq!(&buf.data[0..4], &[128, 128, 128, 128]);
    }
}
