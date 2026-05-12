/// Pixel buffer for software rendering.
use mesh_core_elements::style::Color;
use skia_safe::{
    AlphaType, BlendMode, Canvas, ColorType, ImageInfo, Paint, PaintStyle, RRect, Rect, surfaces,
};

/// A BGRA8888 pixel buffer.
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
        if color.a == 0 && color.r == 0 && color.g == 0 && color.b == 0 {
            self.data.fill(0);
            return;
        }

        if !self.with_skia_canvas(|canvas| {
            canvas.clear(skia_color(color));
        }) {
            return;
        }
    }

    /// Clear a rectangle to a solid color.
    pub fn clear_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        let end_x = x.saturating_add(w).min(self.width);
        let end_y = y.saturating_add(h).min(self.height);
        if x >= end_x || y >= end_y {
            return;
        }

        let rect = Rect::from_xywh(x as f32, y as f32, (end_x - x) as f32, (end_y - y) as f32);
        self.with_skia_canvas(|canvas| {
            let mut paint = src_paint(color);
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rect(rect, &paint);
        });
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
        self.clear_rect(x, y, w, h, color);
    }

    /// Fill a rounded rectangle through the Skia raster backend.
    pub fn fill_rounded_rect(&mut self, x: u32, y: u32, w: u32, h: u32, radius: f32, color: Color) {
        if w == 0 || h == 0 {
            return;
        }
        let radius = radius.max(0.0).min(w as f32 * 0.5).min(h as f32 * 0.5);
        let rect = Rect::from_xywh(x as f32, y as f32, w as f32, h as f32);
        self.with_skia_canvas(|canvas| {
            let mut paint = src_over_paint(color);
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), &paint);
        });
    }

    pub(crate) fn fill_rounded_rect_clipped(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        radius: f32,
        color: Color,
        clip: (i32, i32, i32, i32),
    ) -> bool {
        if w <= 0 || h <= 0 || clip.2 <= 0 || clip.3 <= 0 {
            return false;
        }

        self.with_skia_canvas(|canvas| {
            let save_count = canvas.save();
            canvas.clip_rect(
                Rect::from_xywh(clip.0 as f32, clip.1 as f32, clip.2 as f32, clip.3 as f32),
                None,
                false,
            );
            let rect = Rect::from_xywh(x as f32, y as f32, w as f32, h as f32);
            let radius = radius.max(0.0).min(w as f32 * 0.5).min(h as f32 * 0.5);
            let mut paint = src_over_paint(color);
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), &paint);
            canvas.restore_to_count(save_count);
        })
    }

    pub(crate) fn stroke_rounded_rect_clipped(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        radius: f32,
        stroke_width: f32,
        color: Color,
        clip: (i32, i32, i32, i32),
    ) -> bool {
        if w <= 0 || h <= 0 || stroke_width <= 0.0 || clip.2 <= 0 || clip.3 <= 0 {
            return false;
        }

        self.with_skia_canvas(|canvas| {
            let save_count = canvas.save();
            canvas.clip_rect(
                Rect::from_xywh(clip.0 as f32, clip.1 as f32, clip.2 as f32, clip.3 as f32),
                None,
                false,
            );

            let inset = stroke_width * 0.5;
            let stroke_w = (w as f32 - stroke_width).max(0.0);
            let stroke_h = (h as f32 - stroke_width).max(0.0);
            if stroke_w > 0.0 && stroke_h > 0.0 {
                let rect = Rect::from_xywh(x as f32 + inset, y as f32 + inset, stroke_w, stroke_h);
                let radius = (radius - inset)
                    .max(0.0)
                    .min(stroke_w * 0.5)
                    .min(stroke_h * 0.5);
                let mut paint = src_over_paint(color);
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(stroke_width);
                canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), &paint);
            }

            canvas.restore_to_count(save_count);
        })
    }

    pub(crate) fn with_skia_canvas(&mut self, draw: impl FnOnce(&Canvas)) -> bool {
        if self.width == 0 || self.height == 0 || self.stride == 0 {
            return false;
        }

        let info = ImageInfo::new(
            (self.width as i32, self.height as i32),
            ColorType::BGRA8888,
            AlphaType::Unpremul,
            None,
        );
        let Some(mut surface) = surfaces::wrap_pixels(
            &info,
            self.data.as_mut_slice(),
            Some(self.stride as usize),
            None,
        ) else {
            return false;
        };
        draw(surface.canvas());
        true
    }
}

pub(crate) fn skia_color(color: Color) -> skia_safe::Color {
    skia_safe::Color::from_argb(color.a, color.r, color.g, color.b)
}

fn src_paint(color: Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(false);
    paint.set_color(skia_color(color));
    paint.set_blend_mode(BlendMode::Src);
    paint
}

fn src_over_paint(color: Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(skia_color(color));
    paint.set_blend_mode(BlendMode::SrcOver);
    paint
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
    fn clear_rect_only_touches_clipped_area() {
        let mut buf = PixelBuffer::new(4, 4);
        buf.clear(Color::WHITE);
        buf.clear_rect(1, 1, 2, 2, Color::TRANSPARENT);

        assert_eq!(&buf.data[0..4], &[255, 255, 255, 255]);
        let offset = ((buf.stride + 4) as usize)..((buf.stride + 8) as usize);
        assert_eq!(&buf.data[offset], &[0, 0, 0, 0]);
    }

    #[test]
    fn blend_pixel_applies_coverage() {
        let mut buf = PixelBuffer::new(1, 1);
        buf.blend_pixel(0, 0, Color::WHITE, 128);
        assert_eq!(&buf.data[0..4], &[128, 128, 128, 128]);
    }
}
