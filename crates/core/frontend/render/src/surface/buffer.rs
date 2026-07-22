/// Pixel buffer for software rendering.
use mesh_core_elements::style::Color;
use skia_safe::{
    AlphaType, BlendMode, Canvas, ColorType, ImageInfo, Paint, PaintStyle, RRect, Rect, Surface,
    surfaces,
};

/// A premultiplied-alpha BGRA8888 pixel buffer, matching Wayland
/// `wl_shm::Format::Argb8888` on little-endian hosts.
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
        }) {}
    }

    /// Clear a rectangle to a solid color.
    pub fn clear_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        let end_x = x.saturating_add(w).min(self.width);
        let end_y = y.saturating_add(h).min(self.height);
        if x >= end_x || y >= end_y {
            return;
        }

        if self.clear_rect_direct(x, y, end_x, end_y, color) {
            return;
        }

        let rect = Rect::from_xywh(x as f32, y as f32, (end_x - x) as f32, (end_y - y) as f32);
        self.with_skia_canvas(|canvas| {
            let mut paint = src_paint(color);
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rect(rect, &paint);
        });
    }

    fn clear_rect_direct(&mut self, x: u32, y: u32, end_x: u32, end_y: u32, color: Color) -> bool {
        if self.stride != self.width.saturating_mul(4) {
            return false;
        }
        let row_bytes = (end_x - x) as usize * 4;
        if row_bytes == 0 {
            return true;
        }
        if color.a == 0 && color.r == 0 && color.g == 0 && color.b == 0 {
            for py in y..end_y {
                let start = (py * self.stride + x * 4) as usize;
                let end = start + row_bytes;
                if end > self.data.len() {
                    return false;
                }
                self.data[start..end].fill(0);
            }
            return true;
        }
        let pixel = premultiplied_bgra(color);
        for py in y..end_y {
            let start = (py * self.stride + x * 4) as usize;
            let end = start + row_bytes;
            if end > self.data.len() {
                return false;
            }
            fill_bgra_row(&mut self.data[start..end], &pixel);
        }
        true
    }

    /// Get a single pixel as straight-alpha [`Color`]. Returns transparent
    /// black if out of bounds.
    pub fn get_pixel(&self, x: u32, y: u32) -> Color {
        if x >= self.width || y >= self.height {
            return Color::TRANSPARENT;
        }
        let offset = (y * self.stride + x * 4) as usize;
        if offset + 3 >= self.data.len() {
            return Color::TRANSPARENT;
        }
        unpremultiplied_color(
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        )
    }

    /// Set a single pixel. Coordinates are bounds-checked.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let offset = (y * self.stride + x * 4) as usize;
        if offset + 3 < self.data.len() {
            self.data[offset..offset + 4].copy_from_slice(&premultiplied_bgra(color));
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

    pub(crate) fn with_skia_canvas(&mut self, draw: impl FnOnce(&Canvas)) -> bool {
        if self.width == 0 || self.height == 0 || self.stride == 0 {
            return false;
        }

        let info = ImageInfo::new(
            (self.width as i32, self.height as i32),
            ColorType::BGRA8888,
            AlphaType::Premul,
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

/// A Skia surface kept alive across multiple draws so a paint pass shares
/// one `surfaces::wrap_pixels`. Code paths that mutate `PixelBuffer::data`
/// directly (text glyph blending, icon blits) access raw bytes through
/// `with_buffer`. The active Skia surface internally holds a pointer into
/// the same backing memory, so writes through either path are visible to
/// subsequent Skia draws.
///
/// The buffer's `data` Vec must not be reallocated while a session is live.
pub struct PixelCanvasSession<'a> {
    buffer: &'a mut PixelBuffer,
    surface: Option<Surface>,
}

impl<'a> PixelCanvasSession<'a> {
    pub fn new(buffer: &'a mut PixelBuffer) -> Self {
        Self {
            buffer,
            surface: None,
        }
    }

    /// Run `f` against a Skia canvas wrapping the buffer. The wrapping
    /// surface is created on the first call and reused for subsequent
    /// calls until the session is dropped. Returns `None` if the buffer
    /// has zero extent or Skia could not wrap it.
    pub fn with_canvas<R>(&mut self, f: impl FnOnce(&Canvas) -> R) -> Option<R> {
        if self.surface.is_none() {
            if self.buffer.width == 0 || self.buffer.height == 0 || self.buffer.stride == 0 {
                return None;
            }
            let info = ImageInfo::new(
                (self.buffer.width as i32, self.buffer.height as i32),
                ColorType::BGRA8888,
                AlphaType::Premul,
                None,
            );
            let borrows = surfaces::wrap_pixels(
                &info,
                self.buffer.data.as_mut_slice(),
                Some(self.buffer.stride as usize),
                None,
            );
            // SAFETY: skia-safe's `wrap_pixels` returns `Borrows<'_, Surface>`
            // so the surface cannot outlive the slice borrow at the type
            // level. We shed that borrow with `release()` so the same
            // buffer can also be mutated through `with_buffer` without
            // dropping the surface. The surface stays valid because:
            //   - `self.buffer` (an `&'a mut PixelBuffer`) outlives the
            //     session, keeping the `Vec<u8>` allocation alive.
            //   - The session does not resize `buffer.data`, so Skia's
            //     internal pixel pointer remains stable.
            self.surface = borrows.map(|b| unsafe { b.release() });
        }
        self.surface.as_mut().map(|surface| f(surface.canvas()))
    }

    /// Run `f` with raw access to the underlying `PixelBuffer`. Safe to
    /// interleave with `with_canvas`; the active Skia surface (if any)
    /// reads from and writes to the same backing memory.
    pub fn with_buffer<R>(&mut self, f: impl FnOnce(&mut PixelBuffer) -> R) -> R {
        // Tell Skia that pixel content may be modified externally so it
        // invalidates any cached snapshots. For raw raster surfaces this
        // is effectively a no-op, but it is the documented contract.
        if let Some(surface) = self.surface.as_mut() {
            surface.notify_content_will_change(skia_safe::surface::ContentChangeMode::Retain);
        }
        f(self.buffer)
    }
}

fn fill_bgra_row(dst: &mut [u8], pixel: &[u8; 4]) {
    if dst.is_empty() {
        return;
    }

    dst[..4].copy_from_slice(pixel);
    let mut width = 4usize;
    while width <= dst.len() / 2 {
        let (head, tail) = dst.split_at_mut(width);
        let (tail_half, _) = tail.split_at_mut(width);
        tail_half.copy_from_slice(head);
        width *= 2;
    }
    let remainder = dst.len() - width;
    if remainder > 0 {
        let (head, tail) = dst.split_at_mut(width);
        tail[..remainder].copy_from_slice(&head[..remainder]);
    }
}

fn premultiplied_bgra(color: Color) -> [u8; 4] {
    let alpha = u16::from(color.a);
    let premultiply = |channel: u8| ((u16::from(channel) * alpha + 127) / 255) as u8;
    [
        premultiply(color.b),
        premultiply(color.g),
        premultiply(color.r),
        color.a,
    ]
}

fn unpremultiplied_color(b: u8, g: u8, r: u8, a: u8) -> Color {
    if a == 0 {
        return Color::TRANSPARENT;
    }
    let alpha = u32::from(a);
    let unpremultiply =
        |channel: u8| ((u32::from(channel) * 255 + alpha / 2) / alpha).min(255) as u8;
    Color {
        b: unpremultiply(b),
        g: unpremultiply(g),
        r: unpremultiply(r),
        a,
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

    #[test]
    fn translucent_pixels_are_stored_premultiplied_for_wayland() {
        let mut buf = PixelBuffer::new(1, 1);
        let color = Color {
            r: 224,
            g: 49,
            b: 17,
            a: 102,
        };

        buf.clear(color);

        assert_eq!(&buf.data[0..4], &[7, 20, 90, 102]);
        let round_trip = buf.get_pixel(0, 0);
        assert_eq!(round_trip.a, color.a);
        assert!((i16::from(round_trip.r) - i16::from(color.r)).abs() <= 1);
        assert!((i16::from(round_trip.g) - i16::from(color.g)).abs() <= 1);
        assert!((i16::from(round_trip.b) - i16::from(color.b)).abs() <= 1);
    }
}
