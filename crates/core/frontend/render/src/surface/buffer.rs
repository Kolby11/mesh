//! Pixel buffer for software rendering.
use mesh_core_elements::style::Color;
use mesh_core_resources::{ResourceByteBudget, ResourceByteReservation};
use skia_safe::{
    AlphaType, BlendMode, Canvas, ColorType, ImageInfo, Paint, PaintStyle, PictureRecorder, RRect,
    Rect, Surface, surfaces,
};
use std::sync::OnceLock;

const PIXEL_BUFFER_GLOBAL_MAX_BYTES: usize = 512 * 1024 * 1024;
static PIXEL_BUFFER_GLOBAL_BUDGET: OnceLock<ResourceByteBudget> = OnceLock::new();

fn pixel_buffer_global_budget() -> &'static ResourceByteBudget {
    PIXEL_BUFFER_GLOBAL_BUDGET
        .get_or_init(|| ResourceByteBudget::new(PIXEL_BUFFER_GLOBAL_MAX_BYTES))
}

/// A premultiplied-alpha BGRA8888 pixel buffer, matching Wayland
/// `wl_shm::Format::Argb8888` on little-endian hosts.
#[derive(Debug)]
pub struct PixelBuffer {
    data: Vec<u8>,
    width: u32,
    height: u32,
    stride: u32,
    _budget: ResourceByteReservation,
}

impl Clone for PixelBuffer {
    fn clone(&self) -> Self {
        let budget = pixel_buffer_global_budget()
            .try_reserve(self.data.len())
            .expect("PixelBuffer global allocation budget exhausted while cloning");
        Self {
            data: self.data.clone(),
            width: self.width,
            height: self.height,
            stride: self.stride,
            _budget: budget,
        }
    }
}

impl PixelBuffer {
    /// Keep one software surface within the same practical dimension envelope
    /// as a Wayland SHM surface. The byte limit remains authoritative for the
    /// combined allocation, while this cap rejects pathological thin or tall
    /// buffers before Skia sees them.
    pub const MAX_DIMENSION: u32 = 16_384;

    /// Keep one software surface from consuming unbounded process memory.
    /// The shell applies the same limit before requesting a surface-sized
    /// buffer, while this type remains safe for all direct callers.
    pub const MAX_BYTES: usize = 512 * 1024 * 1024;

    /// Allocate a pixel buffer without panicking on overflow, an oversized
    /// request, or allocator failure.
    pub fn try_new(width: u32, height: u32) -> Option<Self> {
        if width > Self::MAX_DIMENSION || height > Self::MAX_DIMENSION {
            return None;
        }
        let stride = width.checked_mul(4)?;
        let byte_len = usize::try_from(stride)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?;
        if byte_len > Self::MAX_BYTES {
            return None;
        }
        let budget = pixel_buffer_global_budget().try_reserve(byte_len)?;

        let mut data = Vec::new();
        if data.try_reserve_exact(byte_len).is_err() {
            return None;
        }
        data.resize(byte_len, 0);
        Some(Self {
            data,
            width,
            height,
            stride,
            _budget: budget,
        })
    }

    /// Allocate a normal render buffer. Callers handling untrusted or
    /// compositor-provided dimensions should use [`Self::try_new`] so a
    /// rejected allocation can be reported without unwinding.
    pub fn new(width: u32, height: u32) -> Self {
        Self::try_new(width, height)
            .expect("PixelBuffer dimensions exceed the bounded render allocation")
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub(crate) fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Shift a standalone content raster by integral device pixels, preserving
    /// overlapping pixels exactly. Newly exposed pixels become transparent;
    /// returned strips are disjoint and must be repainted before cache reuse.
    #[cfg(test)]
    pub(crate) fn shift_raster_pixels(&mut self, dx: i32, dy: i32) -> Vec<crate::DamageRect> {
        let exposed = self.shift_region_pixels(
            crate::DamageRect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            },
            dx,
            dy,
        );
        for strip in &exposed {
            self.clear_rect(
                strip.x,
                strip.y,
                strip.width,
                strip.height,
                Color::TRANSPARENT,
            );
        }
        exposed.into_vec()
    }

    /// Move the pixels inside `region` by integral device pixels, preserving
    /// overlapping pixels exactly; pixels outside `region` are untouched.
    /// Returns the disjoint strips of `region` that received no moved pixel;
    /// they keep stale contents and must be repainted. A displacement at least
    /// as large as `region` returns the whole region.
    pub fn shift_region_pixels(
        &mut self,
        region: crate::DamageRect,
        dx: i32,
        dy: i32,
    ) -> smallvec::SmallVec<[crate::DamageRect; 2]> {
        let shift = RegionShift::plan(self.width, self.height, region, dx, dy);
        let Some(copy) = shift.copy else {
            return shift.exposed;
        };
        let row_bytes = copy.width as usize * 4;
        for step in 0..copy.height {
            // Moving down must copy bottom-up; copy_within handles horizontal
            // overlap in either direction within each row.
            let row = if dy > 0 { copy.height - 1 - step } else { step };
            let source = ((copy.source_y + row) * self.stride + copy.source_x * 4) as usize;
            let target = ((copy.target_y + row) * self.stride + copy.target_x * 4) as usize;
            self.data.copy_within(source..source + row_bytes, target);
        }
        shift.exposed
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn stride(&self) -> u32 {
        self.stride
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

/// The paint target of one pass: a CPU [`PixelBuffer`] wrapped by a Skia
/// raster surface, or any other Skia surface (a GPU render target). Every
/// paint path draws through [`Self::with_canvas`], so one frame plan paints
/// into either target; the remaining pixel operations — damage clears and
/// in-place scroll shifts — are session methods with a byte fast path on
/// raster targets and a canvas path elsewhere.
pub struct PixelCanvasSession<'a> {
    target: SessionTarget<'a>,
    pub(crate) scene_cache:
        Option<std::rc::Rc<std::cell::RefCell<crate::prepared_scene::SceneNodeCache>>>,
    pub(crate) scroll_replay: Option<(Option<usize>, f32, f32)>,
}

enum SessionTarget<'a> {
    Raster {
        buffer: &'a mut PixelBuffer,
        /// Wraps `buffer`'s pixels; created on first canvas use. The buffer's
        /// `data` must not be reallocated while it is live.
        surface: Option<Surface>,
    },
    // Constructed by the GPU target and by tests.
    #[cfg_attr(not(feature = "renderer-gpu"), allow(dead_code))]
    Surface(&'a mut Surface),
    Recording {
        canvas: &'a Canvas,
        images: crate::prepared_scene::SceneImages,
        width: u32,
        height: u32,
    },
    /// Records into a picture that the canvas replays later; see
    /// [`super::canvas::RasterCanvas`].
    Deferred {
        raster: &'a mut super::canvas::RasterCanvas,
        recorder: PictureRecorder,
    },
}

impl Drop for PixelCanvasSession<'_> {
    fn drop(&mut self) {
        if let SessionTarget::Deferred { raster, recorder } = &mut self.target {
            finish_deferred_recording(raster, recorder);
        }
    }
}

fn begin_deferred_recording(width: u32, height: u32) -> PictureRecorder {
    let mut recorder = PictureRecorder::new();
    recorder.begin_recording(Rect::from_wh(width as f32, height as f32), false);
    recorder
}

fn finish_deferred_recording(
    raster: &mut super::canvas::RasterCanvas,
    recorder: &mut PictureRecorder,
) {
    if let Some(picture) = recorder.finish_recording_as_picture(None)
        && picture.approximate_op_count() > 0
    {
        raster.push_recording(picture);
    }
}

impl<'a> PixelCanvasSession<'a> {
    pub fn new(buffer: &'a mut PixelBuffer) -> Self {
        Self {
            target: SessionTarget::Raster {
                buffer,
                surface: None,
            },
            scene_cache: None,
            scroll_replay: None,
        }
    }

    /// Paint into an existing Skia surface, such as a GPU render target. The
    /// surface must use the premultiplied BGRA layout `PixelBuffer` uses.
    #[cfg_attr(not(feature = "renderer-gpu"), allow(dead_code))]
    pub(crate) fn on_surface(surface: &'a mut Surface) -> Self {
        Self {
            target: SessionTarget::Surface(surface),
            scene_cache: None,
            scroll_replay: None,
        }
    }

    pub(crate) fn deferred(raster: &'a mut super::canvas::RasterCanvas) -> Self {
        let recorder = begin_deferred_recording(raster.width(), raster.height());
        Self {
            target: SessionTarget::Deferred { raster, recorder },
            scene_cache: None,
            scroll_replay: None,
        }
    }

    pub(crate) fn on_recording(
        canvas: &'a Canvas,
        width: u32,
        height: u32,
        images: crate::prepared_scene::SceneImages,
    ) -> Self {
        Self {
            target: SessionTarget::Recording {
                canvas,
                images,
                width,
                height,
            },
            scene_cache: None,
            scroll_replay: None,
        }
    }

    pub(crate) fn with_scene_cache(
        mut self,
        cache: std::rc::Rc<std::cell::RefCell<crate::prepared_scene::SceneNodeCache>>,
    ) -> Self {
        #[cfg(test)]
        if cache.borrow().disabled {
            return self;
        }
        self.scene_cache = Some(cache);
        self
    }

    pub fn width(&self) -> u32 {
        match &self.target {
            SessionTarget::Recording { width, .. } => *width,
            SessionTarget::Deferred { raster, .. } => raster.width(),
            SessionTarget::Raster { buffer, .. } => buffer.width,
            SessionTarget::Surface(surface) => surface.width().max(0) as u32,
        }
    }

    pub fn height(&self) -> u32 {
        match &self.target {
            SessionTarget::Recording { height, .. } => *height,
            SessionTarget::Deferred { raster, .. } => raster.height(),
            SessionTarget::Raster { buffer, .. } => buffer.height,
            SessionTarget::Surface(surface) => surface.height().max(0) as u32,
        }
    }

    /// Run `f` against the target's canvas. A raster target's wrapping
    /// surface is created on the first call and reused until the session is
    /// dropped. Returns `None` if the target has zero extent or Skia could
    /// not wrap it.
    pub fn with_canvas<R>(&mut self, f: impl FnOnce(&Canvas) -> R) -> Option<R> {
        match &mut self.target {
            SessionTarget::Raster { buffer, surface } => {
                if surface.is_none() {
                    *surface = wrap_buffer(buffer);
                }
                surface.as_mut().map(|surface| f(surface.canvas()))
            }
            SessionTarget::Surface(surface) => Some(f(surface.canvas())),
            SessionTarget::Recording { canvas, images, .. } => {
                Some(crate::prepared_scene::with_images(images, || f(canvas)))
            }
            SessionTarget::Deferred { recorder, .. } => recorder.recording_canvas().map(f),
        }
    }

    /// Replace the pixels of `rect` (device pixels, clamped to the target)
    /// with `color`, ignoring the canvas matrix and clip. Call between paint
    /// passes, with no layer open.
    pub fn clear_rect(&mut self, rect: crate::DamageRect, color: Color) {
        match &mut self.target {
            SessionTarget::Recording { canvas, .. } => {
                canvas.draw_irect(device_irect(rect), &src_paint(color));
            }
            SessionTarget::Deferred { recorder, .. } => {
                if let Some(canvas) = recorder.recording_canvas() {
                    debug_assert_eq!(canvas.save_count(), 1, "clear inside an open layer");
                    canvas.save();
                    canvas.reset_matrix();
                    canvas.draw_irect(device_irect(rect), &src_paint(color));
                    canvas.restore();
                }
            }
            SessionTarget::Raster { buffer, surface } => {
                notify_external_write(surface.as_mut());
                buffer.clear_rect(rect.x, rect.y, rect.width, rect.height, color);
            }
            SessionTarget::Surface(surface) => {
                let canvas = surface.canvas();
                debug_assert_eq!(canvas.save_count(), 1, "clear inside an open layer");
                canvas.save();
                canvas.reset_matrix();
                canvas.draw_irect(device_irect(rect), &src_paint(color));
                canvas.restore();
            }
        }
    }

    /// Move the pixels of `region` by whole device pixels with the semantics
    /// of [`PixelBuffer::shift_region_pixels`]: retained pixels are copied
    /// exactly, and the returned strips keep stale content and must be
    /// repainted. Call between paint passes, with no layer open.
    pub fn shift_region(
        &mut self,
        region: crate::DamageRect,
        dx: i32,
        dy: i32,
    ) -> smallvec::SmallVec<[crate::DamageRect; 2]> {
        match &mut self.target {
            SessionTarget::Recording { .. } => {
                panic!("complete scenes cannot shift retained pixels")
            }
            // Shifting reads retained pixels, so everything recorded so far
            // is rasterized here, on this thread, before recording resumes.
            SessionTarget::Deferred { raster, recorder } => {
                finish_deferred_recording(raster, recorder);
                raster.flush();
                let shifted = raster.pixels_mut().shift_region_pixels(region, dx, dy);
                *recorder = begin_deferred_recording(raster.width(), raster.height());
                shifted
            }
            SessionTarget::Raster { buffer, surface } => {
                notify_external_write(surface.as_mut());
                buffer.shift_region_pixels(region, dx, dy)
            }
            SessionTarget::Surface(surface) => {
                let shift = RegionShift::plan(
                    surface.width().max(0) as u32,
                    surface.height().max(0) as u32,
                    region,
                    dx,
                    dy,
                );
                if let Some(copy) = shift.copy {
                    // A snapshot is the only way to read a GPU target while
                    // drawing into it; Skia copies the block on write.
                    let source = skia_safe::IRect::from_xywh(
                        copy.source_x as i32,
                        copy.source_y as i32,
                        copy.width as i32,
                        copy.height as i32,
                    );
                    if let Some(block) = surface.image_snapshot_with_bounds(source) {
                        let canvas = surface.canvas();
                        debug_assert_eq!(canvas.save_count(), 1, "shift inside an open layer");
                        canvas.save();
                        canvas.reset_matrix();
                        let mut paint = Paint::default();
                        paint.set_blend_mode(BlendMode::Src);
                        canvas.draw_image(
                            &block,
                            (copy.target_x as f32, copy.target_y as f32),
                            Some(&paint),
                        );
                        canvas.restore();
                    }
                }
                shift.exposed
            }
        }
    }
}

fn wrap_buffer(buffer: &mut PixelBuffer) -> Option<Surface> {
    if buffer.width == 0 || buffer.height == 0 || buffer.stride == 0 {
        return None;
    }
    let info = ImageInfo::new(
        (buffer.width as i32, buffer.height as i32),
        ColorType::BGRA8888,
        AlphaType::Premul,
        None,
    );
    let borrows = surfaces::wrap_pixels(
        &info,
        buffer.data.as_mut_slice(),
        Some(buffer.stride as usize),
        None,
    );
    // SAFETY: skia-safe's `wrap_pixels` returns `Borrows<'_, Surface>` so the
    // surface cannot outlive the slice borrow at the type level. The session
    // sheds that borrow with `release()` so its own pixel operations can write
    // the buffer without dropping the surface. The surface stays valid
    // because the session's `&'a mut PixelBuffer` outlives it, keeping the
    // `Vec<u8>` allocation alive, and nothing resizes `buffer.data`, so Skia's
    // pixel pointer remains stable.
    borrows.map(|borrows| unsafe { borrows.release() })
}

/// Tell Skia that a raster target's pixels change behind the canvas, so it
/// invalidates any snapshot of them.
fn notify_external_write(surface: Option<&mut Surface>) {
    if let Some(surface) = surface {
        surface.notify_content_will_change(skia_safe::surface::ContentChangeMode::Retain);
    }
}

fn device_irect(rect: crate::DamageRect) -> skia_safe::IRect {
    skia_safe::IRect::from_xywh(
        rect.x.min(i32::MAX as u32) as i32,
        rect.y.min(i32::MAX as u32) as i32,
        rect.width.min(i32::MAX as u32) as i32,
        rect.height.min(i32::MAX as u32) as i32,
    )
}

/// The pixel moves of one integral region shift: the retained block, if any,
/// and the strips of the region that it leaves uncovered.
struct RegionShift {
    copy: Option<RegionCopy>,
    exposed: smallvec::SmallVec<[crate::DamageRect; 2]>,
}

/// A `width`×`height` block moved from `source` to `target` on one target.
#[derive(Clone, Copy)]
struct RegionCopy {
    source_x: u32,
    source_y: u32,
    target_x: u32,
    target_y: u32,
    width: u32,
    height: u32,
}

impl RegionShift {
    fn plan(
        surface_width: u32,
        surface_height: u32,
        region: crate::DamageRect,
        dx: i32,
        dy: i32,
    ) -> Self {
        let mut exposed = smallvec::SmallVec::new();
        let region = crate::DamageRect {
            x: region.x.min(surface_width),
            y: region.y.min(surface_height),
            width: region.width.min(surface_width.saturating_sub(region.x)),
            height: region.height.min(surface_height.saturating_sub(region.y)),
        };
        if region.width == 0 || region.height == 0 || (dx == 0 && dy == 0) {
            return Self {
                copy: None,
                exposed,
            };
        }
        let ax = dx.unsigned_abs();
        let ay = dy.unsigned_abs();
        if ax >= region.width || ay >= region.height {
            exposed.push(region);
            return Self {
                copy: None,
                exposed,
            };
        }
        let copy = RegionCopy {
            source_x: region.x + if dx < 0 { ax } else { 0 },
            source_y: region.y + if dy < 0 { ay } else { 0 },
            target_x: region.x + if dx > 0 { ax } else { 0 },
            target_y: region.y + if dy > 0 { ay } else { 0 },
            width: region.width - ax,
            height: region.height - ay,
        };
        if ay > 0 {
            exposed.push(crate::DamageRect {
                x: region.x,
                y: if dy > 0 {
                    region.y
                } else {
                    region.y + copy.height
                },
                width: region.width,
                height: ay,
            });
        }
        if ax > 0 {
            exposed.push(crate::DamageRect {
                x: if dx > 0 {
                    region.x
                } else {
                    region.x + region.width - ax
                },
                y: copy.target_y,
                width: ax,
                height: copy.height,
            });
        }
        Self {
            copy: Some(copy),
            exposed,
        }
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
    #[test]
    fn raster_shift_matches_independent_byte_oracle_and_disjoint_coverage() {
        for width in 0..=7 {
            for height in 0..=5 {
                for dx in (-9..=9).chain([i32::MIN, i32::MAX]) {
                    for dy in (-7..=7).chain([i32::MIN, i32::MAX]) {
                        let mut buffer = super::PixelBuffer::new(width, height);
                        for (index, pixel) in buffer.data_mut().chunks_exact_mut(4).enumerate() {
                            if index % 3 != 0 {
                                pixel.copy_from_slice(&[index as u8, (index * 3) as u8, 17, 200]);
                            }
                        }
                        let original = buffer.data().to_vec();
                        let allocation = buffer.data().as_ptr();
                        let strips = buffer.shift_raster_pixels(dx, dy);
                        assert!(strips.len() <= 2);
                        assert_eq!(buffer.data().as_ptr(), allocation);
                        for y in 0..height {
                            for x in 0..width {
                                let sx = i64::from(x) - i64::from(dx);
                                let sy = i64::from(y) - i64::from(dy);
                                let retained = sx >= 0
                                    && sx < i64::from(width)
                                    && sy >= 0
                                    && sy < i64::from(height);
                                let target = ((y * width + x) * 4) as usize;
                                let expected = if retained {
                                    let source = ((sy as u32 * width + sx as u32) * 4) as usize;
                                    &original[source..source + 4]
                                } else {
                                    &[0; 4]
                                };
                                assert_eq!(
                                    &buffer.data()[target..target + 4],
                                    expected,
                                    "{width}x{height}, shift ({dx},{dy}), pixel ({x},{y})"
                                );
                                let coverage = strips
                                    .iter()
                                    .filter(|strip| {
                                        x >= strip.x
                                            && x < strip.x + strip.width
                                            && y >= strip.y
                                            && y < strip.y + strip.height
                                    })
                                    .count();
                                assert_eq!(coverage, usize::from(!retained));
                            }
                        }
                    }
                }
            }
        }
    }

    use super::*;

    #[test]
    fn buffer_creation_and_clear() {
        let mut buf = PixelBuffer::new(10, 10);
        assert_eq!(buf.data().len(), 400);

        buf.clear(Color::WHITE);
        // Check first pixel is white (BGRA order).
        assert_eq!(&buf.data()[0..4], &[255, 255, 255, 255]);
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

        assert_eq!(&buf.data()[0..4], &[255, 255, 255, 255]);
        let offset = ((buf.stride() + 4) as usize)..((buf.stride() + 8) as usize);
        assert_eq!(&buf.data()[offset], &[0, 0, 0, 0]);
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

        assert_eq!(&buf.data()[0..4], &[7, 20, 90, 102]);
        let round_trip = buf.get_pixel(0, 0);
        assert_eq!(round_trip.a, color.a);
        assert!((i16::from(round_trip.r) - i16::from(color.r)).abs() <= 1);
        assert!((i16::from(round_trip.g) - i16::from(color.g)).abs() <= 1);
        assert!((i16::from(round_trip.b) - i16::from(color.b)).abs() <= 1);
    }

    #[test]
    fn try_new_rejects_overflow_and_oversized_allocations() {
        assert!(PixelBuffer::try_new(u32::MAX, 2).is_none());
        assert!(PixelBuffer::try_new(PixelBuffer::MAX_DIMENSION + 1, 1).is_none());
        assert!(PixelBuffer::try_new(16_385, 8_192).is_none());
        assert!(PixelBuffer::try_new(16, 16).is_some());
    }

    #[test]
    fn canvas_session_keeps_backing_storage_stable() {
        let mut buffer = PixelBuffer::try_new(4, 4).expect("small buffer must allocate");
        let allocation = buffer.data().as_ptr();

        {
            let mut session = PixelCanvasSession::new(&mut buffer);
            session.clear_rect(
                crate::DamageRect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                Color::WHITE,
            );
            assert!(
                session
                    .with_canvas(|canvas| {
                        canvas.clear(skia_color(Color::BLACK));
                    })
                    .is_some()
            );
            session.shift_region(
                crate::DamageRect {
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 4,
                },
                1,
                1,
            );
        }

        assert_eq!(buffer.data().as_ptr(), allocation);
        assert_eq!(buffer.get_pixel(3, 3), Color::BLACK);
    }
}

#[cfg(test)]
mod shift_region_tests {
    use super::*;

    #[test]
    fn region_shift_moves_only_pixels_inside_the_region() {
        let mut buffer = PixelBuffer::new(8, 8);
        for (index, pixel) in buffer.data_mut().chunks_mut(4).enumerate() {
            pixel.copy_from_slice(&[index as u8, 0, 0, 255]);
        }
        let before = buffer.data().to_vec();
        let region = crate::DamageRect {
            x: 2,
            y: 2,
            width: 4,
            height: 4,
        };
        let exposed = buffer.shift_region_pixels(region, 0, -1);
        assert_eq!(
            exposed.as_slice(),
            &[crate::DamageRect {
                x: 2,
                y: 5,
                width: 4,
                height: 1
            }]
        );
        let at = |data: &[u8], x: usize, y: usize| data[(y * 8 + x) * 4];
        for y in 0..8 {
            for x in 0..8 {
                let inside = (2..6).contains(&x) && (2..5).contains(&y);
                let expected = if inside {
                    at(&before, x, y + 1)
                } else {
                    at(&before, x, y)
                };
                if !(inside || !(2..6).contains(&x) || !(2..6).contains(&y)) {
                    continue;
                }
                assert_eq!(at(buffer.data(), x, y), expected, "pixel ({x}, {y})");
            }
        }
        assert_eq!(
            buffer.shift_region_pixels(region, 0, 9).as_slice(),
            &[region],
            "a move past the region exposes all of it"
        );
    }

    /// Clears and shifts on a non-raster session (the GPU path) must match
    /// the raster byte fast paths exactly; a raster Skia surface stands in
    /// for the GPU target so this runs on every host.
    #[test]
    fn surface_session_clear_and_shift_match_the_raster_byte_paths() {
        let fill = |buffer: &mut PixelBuffer| {
            for (index, pixel) in buffer.data_mut().chunks_exact_mut(4).enumerate() {
                let alpha = (index * 37 % 256) as u8;
                pixel.copy_from_slice(&[alpha / 2, alpha / 3, alpha, alpha]);
            }
        };
        let info = ImageInfo::new((9, 7), ColorType::BGRA8888, AlphaType::Premul, None);
        let region = crate::DamageRect {
            x: 1,
            y: 2,
            width: 7,
            height: 4,
        };
        let cleared = crate::DamageRect {
            x: 3,
            y: 0,
            width: 20,
            height: 2,
        };
        let translucent = Color {
            r: 200,
            g: 10,
            b: 90,
            a: 120,
        };
        for (dx, dy) in [(0, 1), (0, -2), (2, 0), (-3, 1), (1, -1), (8, 0)] {
            let mut raster = PixelBuffer::new(9, 7);
            fill(&mut raster);
            let mut source = PixelBuffer::new(9, 7);
            fill(&mut source);
            let mut surface = surfaces::raster(&info, None, None).unwrap();
            assert!(surface.canvas().write_pixels(
                &info,
                source.data(),
                source.stride() as usize,
                (0, 0)
            ));

            let raster_exposed = {
                let mut session = PixelCanvasSession::new(&mut raster);
                session.clear_rect(cleared, translucent);
                session.shift_region(region, dx, dy)
            };
            let surface_exposed = {
                let mut session = PixelCanvasSession::on_surface(&mut surface);
                session.clear_rect(cleared, translucent);
                session.shift_region(region, dx, dy)
            };
            assert_eq!(raster_exposed, surface_exposed, "shift ({dx},{dy})");
            let mut read = PixelBuffer::new(9, 7);
            let stride = read.stride() as usize;
            assert!(surface.read_pixels(&info, read.data_mut(), stride, (0, 0)));
            assert_eq!(read.data(), raster.data(), "shift ({dx},{dy})");
        }
    }
}
