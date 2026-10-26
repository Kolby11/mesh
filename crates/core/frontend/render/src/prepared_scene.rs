//! Immutable Skia scenes pin prepared images and glyph pages without retaining
//! a live component, resource resolver, font database, or GPU context.

mod node_cache;

pub use node_cache::ScenePreparationMetrics;
pub(crate) use node_cache::{NodePaintKey, NodePicture, SceneNodeCache};

use crate::{FramePaintPlan, PixelCanvasSession};
use skia_safe::{Picture, PictureRecorder, Rect};
use std::sync::Arc;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

static RESOURCE_COMPLETION_EPOCH: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

pub(crate) fn resource_completed() {
    RESOURCE_COMPLETION_EPOCH.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
}

pub(crate) fn resource_completion_epoch() -> u64 {
    RESOURCE_COMPLETION_EPOCH.load(std::sync::atomic::Ordering::Acquire)
}

fn resources_pending() -> bool {
    crate::surface::icon_resolution_jobs_pending()
        || crate::surface::icon_raster_jobs_pending()
        || crate::surface::glyph_raster_jobs_pending()
        || crate::surface::image_decode_jobs_pending()
}

pub(crate) type SceneImages = Rc<RefCell<BTreeMap<u32, skia_safe::Image>>>;

thread_local! {
    static RECORDING_IMAGES: RefCell<Option<SceneImages>> = const { RefCell::new(None) };
}

pub(crate) fn recording_images() -> bool {
    RECORDING_IMAGES.with(|images| images.borrow().is_some())
}

pub(crate) fn retain_image(image: &skia_safe::Image) {
    RECORDING_IMAGES.with(|images| {
        if let Some(images) = images.borrow().as_ref() {
            images
                .borrow_mut()
                .entry(image.unique_id())
                .or_insert_with(|| image.clone());
        }
    });
}

pub(crate) fn with_images<R>(images: &SceneImages, draw: impl FnOnce() -> R) -> R {
    struct Restore(Option<SceneImages>);
    impl Drop for Restore {
        fn drop(&mut self) {
            RECORDING_IMAGES.with(|images| images.replace(self.0.take()));
        }
    }
    let _restore =
        Restore(RECORDING_IMAGES.with(|current| current.replace(Some(Rc::clone(images)))));
    draw()
}

/// A complete scene. Skia's immutable picture owns every referenced raster
/// image, shader and glyph page, including assets removed from live caches.
pub struct PreparedScene {
    picture: Picture,
    pub(crate) images: Vec<skia_safe::Image>,
    revision: u64,
    size: (u32, u32),
}

impl crate::scene_thread::SceneResources for PreparedScene {
    fn revision(&self) -> u64 {
        self.revision
    }
}

impl PreparedScene {
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    pub fn draw(&self, canvas: &skia_safe::Canvas) {
        canvas.draw_picture(&self.picture, None, None);
    }
}

/// UI-owned scene preparation. Each recording replaces the complete topology;
/// a worker can replay it after coalescing without depending on skipped pixels.
pub struct SceneCanvas {
    width: u32,
    height: u32,
    recorder: PictureRecorder,
    images: SceneImages,
    node_cache: Rc<RefCell<SceneNodeCache>>,
    plan: Option<Arc<FramePaintPlan>>,
    prepared: Option<Arc<PreparedScene>>,
    pub surface: crate::scene_thread::SceneSurface,
    retire: Option<Arc<dyn Fn(crate::scene_thread::SceneSurface) + Send + Sync>>,
}

impl SceneCanvas {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .ok()?;
        if width == 0
            || height == 0
            || width > crate::PixelBuffer::MAX_DIMENSION
            || height > crate::PixelBuffer::MAX_DIMENSION
        {
            return None;
        }
        Some(Self {
            width,
            height,
            recorder: PictureRecorder::new(),
            images: SceneImages::default(),
            node_cache: Rc::default(),
            plan: None,
            prepared: None,
            surface: crate::scene_thread::SceneSurface { id, incarnation: 1 },
            retire: None,
        })
    }

    pub fn on_retire(
        &mut self,
        retire: impl Fn(crate::scene_thread::SceneSurface) + Send + Sync + 'static,
    ) {
        self.retire = Some(Arc::new(retire));
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn begin(&mut self, plan: &FramePaintPlan) {
        crate::surface::image_pages::refresh();
        self.images.borrow_mut().clear();
        self.node_cache
            .borrow_mut()
            .begin(plan.inputs.resource_revision);
        self.recorder = PictureRecorder::new();
        self.recorder
            .begin_recording(Rect::from_wh(self.width as f32, self.height as f32), false);
        self.plan = Some(Arc::new(plan.clone()));
        self.prepared = None;
    }

    pub fn session(&mut self) -> PixelCanvasSession<'_> {
        PixelCanvasSession::on_recording(
            self.recorder
                .recording_canvas()
                .expect("begin a complete scene before painting"),
            self.width,
            self.height,
            Rc::clone(&self.images),
        )
        .with_scene_cache(Rc::clone(&self.node_cache))
    }

    /// Widen this frame's redraw region by `logical` damage that an earlier,
    /// refused or unpresented frame left for this present. The scene worker
    /// redraws its retained target only inside the plan's damage, so damage
    /// carried forward by the caller would otherwise be reported to the
    /// compositor over stale pixels.
    pub fn include_damage(&mut self, logical: &[crate::DamageRect]) {
        let Some(plan) = self.plan.as_mut() else {
            return;
        };
        let missing: Vec<_> = logical
            .iter()
            .filter(|rect| rect.width > 0 && rect.height > 0)
            .filter(|rect| !plan.damage.logical.contains(rect))
            .copied()
            .collect();
        if missing.is_empty() {
            return;
        }
        let plan = Arc::make_mut(plan);
        let mut bounds = plan.damage.logical_bounds;
        for rect in &missing {
            bounds = if bounds.width == 0 || bounds.height == 0 {
                *rect
            } else {
                let left = bounds.x.min(rect.x);
                let top = bounds.y.min(rect.y);
                let right = (bounds.x + bounds.width).max(rect.x + rect.width);
                let bottom = (bounds.y + bounds.height).max(rect.y + rect.height);
                crate::DamageRect {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                }
            };
        }
        let mut rects = plan.damage.logical.to_vec();
        rects.extend(missing);
        plan.damage.logical = rects.into();
        plan.damage.logical_bounds = bounds;
    }

    /// Return preparation work since the most recent `begin`, excluding GPU replay.
    pub fn preparation_metrics(&self) -> ScenePreparationMetrics {
        self.node_cache.borrow().metrics
    }

    #[cfg(test)]
    pub(crate) fn set_node_cache_enabled(&mut self, enabled: bool) {
        self.node_cache.borrow_mut().disabled = !enabled;
    }

    pub fn snapshot(
        &mut self,
        scale: crate::FractionalScale,
    ) -> Option<crate::scene_thread::SceneSnapshot<PreparedScene>> {
        let plan = self.plan.as_ref()?;
        if self.prepared.is_none() {
            self.prepared = Some(Arc::new(PreparedScene {
                picture: self.recorder.finish_recording_as_picture(None)?,
                images: self.images.borrow().values().cloned().collect(),
                revision: plan.inputs.resource_revision,
                size: (self.width, self.height),
            }));
        }
        crate::scene_thread::SceneSnapshot::new(
            self.surface,
            Arc::clone(plan),
            Arc::clone(self.prepared.as_ref()?),
            scale,
        )
        .ok()
    }
}

impl Drop for SceneCanvas {
    fn drop(&mut self) {
        if let Some(retire) = &self.retire {
            retire(self.surface);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FractionalScale, PixelBuffer};
    use skia_safe::{Color, Paint};

    fn plan() -> FramePaintPlan {
        let mut plan = FramePaintPlan::empty();
        plan.inputs.surface_size = Some((32, 24));
        plan.inputs.resource_revision = 42;
        plan
    }

    fn raster(scene: &PreparedScene) -> PixelBuffer {
        let mut buffer = PixelBuffer::new(32, 24);
        PixelCanvasSession::new(&mut buffer).with_canvas(|canvas| scene.draw(canvas));
        buffer
    }

    #[test]
    fn frozen_scene_owns_images_after_producer_and_source_are_dropped() {
        let mut recording = SceneCanvas::new(32, 24).unwrap();
        recording.begin(&plan());
        let mut source = skia_safe::surfaces::raster_n32_premul((16, 16)).unwrap();
        source.canvas().clear(Color::RED);
        let image = source.image_snapshot();
        recording.session().with_canvas(|canvas| {
            canvas.draw_image(&image, (4., 3.), None);
        });
        let snapshot = recording.snapshot(FractionalScale::identity()).unwrap();
        source.canvas().clear(Color::BLUE);
        drop(image);
        drop(source);
        drop(recording);
        let buffer = raster(snapshot.resources());
        assert_eq!(buffer.get_pixel(8, 8).r, 255);
        assert_eq!(buffer.get_pixel(8, 8).b, 0);
    }

    #[test]
    fn later_recordings_cannot_mutate_committed_scenes() {
        let mut recording = SceneCanvas::new(32, 24).unwrap();
        recording.begin(&plan());
        recording.session().with_canvas(|canvas| {
            canvas.draw_rect(
                Rect::from_wh(32., 24.),
                Paint::default().set_color(Color::RED),
            );
        });
        let first = recording.snapshot(FractionalScale::identity()).unwrap();
        recording.begin(&plan());
        recording.session().with_canvas(|canvas| {
            canvas.draw_rect(
                Rect::from_wh(32., 24.),
                Paint::default().set_color(Color::BLUE),
            );
        });
        let second = recording.snapshot(FractionalScale::identity()).unwrap();
        assert_eq!(raster(first.resources()).get_pixel(8, 8).r, 255);
        assert_eq!(raster(second.resources()).get_pixel(8, 8).b, 255);
    }

    #[test]
    fn complete_recording_can_replay_under_disjoint_damage() {
        let mut recording = SceneCanvas::new(32, 24).unwrap();
        recording.begin(&plan());
        recording.session().with_canvas(|canvas| {
            canvas.draw_rect(
                Rect::from_wh(32., 24.),
                Paint::default().set_color(Color::RED),
            );
        });
        let snapshot = recording.snapshot(FractionalScale::identity()).unwrap();
        let mut buffer = PixelBuffer::new(32, 24);
        buffer.clear(mesh_core_elements::Color::WHITE);
        PixelCanvasSession::new(&mut buffer).with_canvas(|canvas| {
            let mut region = skia_safe::Region::new();
            region.op_rect(
                skia_safe::IRect::from_xywh(0, 0, 8, 8),
                skia_safe::region::RegionOp::Union,
            );
            region.op_rect(
                skia_safe::IRect::from_xywh(16, 16, 8, 8),
                skia_safe::region::RegionOp::Union,
            );
            canvas.clip_region(&region, None);
            snapshot.resources().draw(canvas);
        });
        assert_eq!(buffer.get_pixel(4, 4).g, 0);
        assert_eq!(buffer.get_pixel(20, 20).g, 0);
        assert_eq!(buffer.get_pixel(12, 12).g, 255);
    }

    #[test]
    fn included_damage_widens_the_snapshot_plan() {
        let mut recording = SceneCanvas::new(32, 24).unwrap();
        let mut frame = plan();
        let own = crate::DamageRect {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        };
        let carried = crate::DamageRect {
            x: 10,
            y: 10,
            width: 6,
            height: 2,
        };
        frame.damage.logical = vec![own].into();
        frame.damage.logical_bounds = own;
        recording.begin(&frame);
        recording.include_damage(&[carried, own]);
        let snapshot = recording.snapshot(FractionalScale::identity()).unwrap();
        assert_eq!(&*snapshot.plan().damage.logical, &[own, carried]);
        assert_eq!(
            snapshot.plan().damage.logical_bounds,
            crate::DamageRect {
                x: 0,
                y: 0,
                width: 16,
                height: 12
            }
        );
    }
}

#[cfg(test)]
mod retained_tests;
