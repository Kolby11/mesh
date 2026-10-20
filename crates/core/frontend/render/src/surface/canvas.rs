use super::{PixelBuffer, PixelCanvasSession};
use skia_safe::Picture;

/// UI-owned surface state: retained CPU pixels or a complete immutable scene
/// whose GPU targets remain exclusively on the render thread.
pub enum SurfaceCanvas {
    Raster(RasterCanvas),
    Scene(crate::prepared_scene::SceneCanvas),
}

impl SurfaceCanvas {
    pub fn width(&self) -> u32 {
        match self {
            Self::Raster(raster) => raster.buffer.width(),
            Self::Scene(scene) => scene.width(),
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Self::Raster(raster) => raster.buffer.height(),
            Self::Scene(scene) => scene.height(),
        }
    }

    pub fn is_gpu(&self) -> bool {
        !matches!(self, Self::Raster(_))
    }

    pub fn is_scene(&self) -> bool {
        matches!(self, Self::Scene(_))
    }

    /// Open a raster session or the current complete scene recording. Scene
    /// recordings must be begun first and cannot read or shift retained pixels.
    pub fn session(&mut self) -> PixelCanvasSession<'_> {
        match self {
            Self::Raster(raster) => raster.session(),
            Self::Scene(scene) => scene.session(),
        }
    }

    /// See [`RasterCanvas::set_deferred`]. Scenes are always recorded.
    pub fn set_deferred_raster(&mut self, deferred: bool) {
        if let Self::Raster(raster) = self {
            raster.set_deferred(deferred);
        }
    }

    /// The retained CPU pixels. Panics in debug builds while recorded paint
    /// is still pending; use [`Self::as_raster_mut`], which replays it.
    pub fn as_raster(&self) -> Option<&PixelBuffer> {
        match self {
            Self::Raster(raster) => {
                debug_assert!(
                    raster.pending.is_empty(),
                    "read a raster canvas with unreplayed paint"
                );
                Some(&raster.buffer)
            }
            Self::Scene(_) => None,
        }
    }

    /// The retained CPU pixels, with any pending recorded paint replayed.
    pub fn as_raster_mut(&mut self) -> Option<&mut PixelBuffer> {
        match self {
            Self::Raster(raster) => Some(raster.buffer_mut()),
            Self::Scene(_) => None,
        }
    }
}

impl From<PixelBuffer> for SurfaceCanvas {
    fn from(buffer: PixelBuffer) -> Self {
        Self::Raster(RasterCanvas::from(buffer))
    }
}

/// Retained CPU pixels that can defer rasterization.
///
/// While deferred, a paint session records into a Skia picture instead of the
/// pixels. The pictures pin every image and glyph page they draw, so they can
/// be replayed on any thread without the painter's thread-local caches; this is
/// the boundary that lets the shell rasterize several surfaces in parallel.
/// Every pixel read goes through [`Self::buffer_mut`] or [`Self::replay`],
/// which apply pending pictures in recording order first.
pub struct RasterCanvas {
    buffer: PixelBuffer,
    deferred: bool,
    pending: Vec<Picture>,
}

impl From<PixelBuffer> for RasterCanvas {
    fn from(buffer: PixelBuffer) -> Self {
        Self {
            buffer,
            deferred: false,
            pending: Vec::new(),
        }
    }
}

impl RasterCanvas {
    pub fn width(&self) -> u32 {
        self.buffer.width()
    }

    pub fn height(&self) -> u32 {
        self.buffer.height()
    }

    /// Record later sessions for [`Self::replay`] instead of rasterizing them
    /// immediately. Turning deferral off replays anything pending.
    pub fn set_deferred(&mut self, deferred: bool) {
        if !deferred {
            self.flush();
        }
        self.deferred = deferred;
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// The pixels after every pending picture has been replayed.
    pub fn buffer_mut(&mut self) -> &mut PixelBuffer {
        self.flush();
        &mut self.buffer
    }

    /// Take the pending pictures together with the pixels they draw into. The
    /// returned job is `Send` and leaves the canvas with nothing pending.
    pub fn replay(&mut self) -> Option<RasterReplay<'_>> {
        if self.pending.is_empty() {
            return None;
        }
        Some(RasterReplay {
            buffer: &mut self.buffer,
            pictures: std::mem::take(&mut self.pending),
        })
    }

    pub fn session(&mut self) -> PixelCanvasSession<'_> {
        if self.deferred {
            PixelCanvasSession::deferred(self)
        } else {
            PixelCanvasSession::new(self.buffer_mut())
        }
    }

    pub(crate) fn push_recording(&mut self, picture: Picture) {
        self.pending.push(picture);
    }

    pub(crate) fn flush(&mut self) {
        if let Some(replay) = self.replay() {
            replay.run();
        }
    }

    pub(crate) fn pixels_mut(&mut self) -> &mut PixelBuffer {
        debug_assert!(self.pending.is_empty());
        &mut self.buffer
    }
}

/// Recorded paint for one raster canvas, ready to replay off the UI thread.
pub struct RasterReplay<'a> {
    buffer: &'a mut PixelBuffer,
    pictures: Vec<Picture>,
}

impl RasterReplay<'_> {
    /// Rasterize the recorded pictures in order and return how long it took.
    pub fn run(self) -> std::time::Duration {
        let started = std::time::Instant::now();
        let pictures = self.pictures;
        PixelCanvasSession::new(self.buffer).with_canvas(|canvas| {
            for picture in &pictures {
                canvas.draw_picture(picture, None, None);
            }
        });
        started.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DamageRect;
    use mesh_core_elements::style::Color;
    use skia_safe::{Paint, Rect};

    fn red() -> Color {
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    fn paint_scene(canvas: &mut SurfaceCanvas) {
        let mut session = canvas.session();
        session.clear_rect(
            DamageRect {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
            red(),
        );
        session.with_canvas(|canvas| {
            canvas.save();
            canvas.clip_rect(Rect::from_xywh(2., 2., 8., 8.), None, true);
            canvas.draw_rect(
                Rect::from_wh(16., 16.),
                Paint::default().set_color(skia_safe::Color::from_argb(128, 0, 0, 255)),
            );
            canvas.restore();
        });
    }

    #[test]
    fn deferred_replay_matches_immediate_raster() {
        let mut immediate = SurfaceCanvas::from(PixelBuffer::new(16, 16));
        paint_scene(&mut immediate);

        let mut deferred = SurfaceCanvas::from(PixelBuffer::new(16, 16));
        deferred.set_deferred_raster(true);
        paint_scene(&mut deferred);
        let SurfaceCanvas::Raster(raster) = &mut deferred else {
            unreachable!()
        };
        assert!(raster.has_pending());
        assert_eq!(raster.buffer.get_pixel(4, 4).a, 0, "nothing rasterized yet");
        std::thread::scope(|scope| {
            let replay = raster.replay().expect("pending paint");
            scope.spawn(move || replay.run());
        });
        assert!(!raster.has_pending());
        assert_eq!(
            immediate.as_raster().unwrap().data(),
            deferred.as_raster().unwrap().data()
        );
    }

    #[test]
    fn deferred_sessions_replay_in_order_over_retained_pixels() {
        let mut canvas = SurfaceCanvas::from(PixelBuffer::new(8, 8));
        canvas.set_deferred_raster(true);
        canvas.session().clear_rect(
            DamageRect {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            },
            red(),
        );
        canvas.session().clear_rect(
            DamageRect {
                x: 0,
                y: 0,
                width: 4,
                height: 8,
            },
            Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            },
        );
        let buffer = canvas.as_raster_mut().unwrap();
        assert_eq!(buffer.get_pixel(1, 1).g, 255);
        assert_eq!(buffer.get_pixel(6, 1).r, 255);
    }

    #[test]
    fn deferred_shift_reads_replayed_pixels() {
        let mut canvas = SurfaceCanvas::from(PixelBuffer::new(8, 8));
        canvas.set_deferred_raster(true);
        let mut session = canvas.session();
        session.clear_rect(
            DamageRect {
                x: 0,
                y: 0,
                width: 8,
                height: 2,
            },
            red(),
        );
        let exposed = session.shift_region(
            DamageRect {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            },
            0,
            4,
        );
        drop(session);
        assert_eq!(exposed.len(), 1);
        let buffer = canvas.as_raster_mut().unwrap();
        assert_eq!(buffer.get_pixel(3, 5).r, 255);
    }
}
