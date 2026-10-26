//! GPU state and exported buffers owned exclusively by the scene worker.

use super::{DmabufPlane, DmabufPlanes, DmabufTarget, GpuContext, GpuTarget};
use crate::DamageRect;
use crate::prepared_scene::PreparedScene;
use crate::scene_thread::{SceneFrame, SceneRenderer, SceneSurface};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

const MAX_SLOTS: usize = 3;
const AVAILABLE: u8 = 0;
const OUTPUT_OWNED: u8 = 1;
const COMPOSITOR_OWNED: u8 = 2;
const RETIRED: u8 = 3;

struct Slot {
    target: DmabufTarget,
    leased: Arc<AtomicU8>,
    pending: Vec<DamageRect>,
    _reservation: mesh_core_resources::ResourceByteReservation,
}

struct SurfaceState {
    retained: GpuTarget,
    slots: Vec<Slot>,
    _reservation: mesh_core_resources::ResourceByteReservation,
}

/// Only descriptors and a release flag cross the thread boundary. Releasing
/// an output never calls GL, even after renderer shutdown or surface removal.
pub struct GpuSceneOutput {
    pub planes: DmabufPlanes,
    pub width: u32,
    pub height: u32,
    pub damage: Vec<DamageRect>,
    leased: Arc<AtomicU8>,
}

#[derive(Clone)]
pub struct GpuBufferRelease(Arc<AtomicU8>);
impl GpuBufferRelease {
    /// Destroyed protocol objects cannot produce a future release event. Retire
    /// their texture instead of making compositor-held storage writable again.
    pub fn retire(self) {
        self.0.store(RETIRED, Ordering::Release);
    }

    pub fn release(self) {
        self.0.store(AVAILABLE, Ordering::Release);
    }
}
impl GpuSceneOutput {
    pub fn attached(&self) -> GpuBufferRelease {
        self.leased.store(COMPOSITOR_OWNED, Ordering::Release);
        GpuBufferRelease(Arc::clone(&self.leased))
    }
}

impl Drop for GpuSceneOutput {
    fn drop(&mut self) {
        let _ = self.leased.compare_exchange(
            OUTPUT_OWNED,
            AVAILABLE,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

#[derive(Debug)]
pub enum GpuSceneError {
    Gpu(super::GpuError),
    Descriptor(std::io::Error),
    BuffersBusy,
    ResourceBudget,
}

impl std::fmt::Display for GpuSceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gpu(error) => error.fmt(f),
            Self::Descriptor(error) => error.fmt(f),
            Self::BuffersBusy => f.write_str("all scene buffers are leased"),
            Self::ResourceBudget => f.write_str("GPU scene byte budget exhausted"),
        }
    }
}

// Raster identities keep Ganesh's upload keys live. The cache allowance is
// additional to retained targets, so large surfaces cannot evict all assets.
struct ImageResidency {
    images: mesh_core_elements::lru::ByteLruCache<u32, skia_safe::Image>,
    admissions: usize,
}

impl ImageResidency {
    fn new(bytes: usize) -> Self {
        Self {
            images: mesh_core_elements::lru::ByteLruCache::new(2048, bytes),
            admissions: 0,
        }
    }

    fn prepare(&mut self, images: &[skia_safe::Image]) {
        for image in images {
            if self.images.get(&image.unique_id()).is_some() {
                continue;
            }
            let bytes = image.width() as usize * image.height() as usize * 4;
            if self.images.insert(image.unique_id(), image.clone(), bytes) {
                self.admissions += 1;
            }
        }
    }
}

pub struct GpuSceneRenderer {
    surfaces: BTreeMap<SceneSurface, SurfaceState>,
    residency: ImageResidency,
    context: GpuContext,
    budget: mesh_core_resources::ResourceByteBudget,
    #[cfg(test)]
    rasterizations: usize,
}

impl GpuSceneRenderer {
    /// Construct on the worker on the compositor's device. Probe descriptors
    /// before enabling this renderer; unsupported export falls back to SHM.
    pub fn new(device: u64) -> Result<Self, GpuSceneError> {
        let context = GpuContext::on_drm_device(device).map_err(GpuSceneError::Gpu)?;
        Ok(Self {
            surfaces: BTreeMap::new(),
            residency: ImageResidency::new(64 * 1024 * 1024),
            context,
            budget: mesh_core_resources::ResourceByteBudget::new(512 * 1024 * 1024),
            #[cfg(test)]
            rasterizations: 0,
        })
    }

    #[cfg(test)]
    pub(crate) fn finish(&mut self) {
        self.context.finish();
    }

    #[cfg(test)]
    pub(crate) fn read_back(
        &mut self,
        surface: SceneSurface,
        output: &GpuSceneOutput,
    ) -> Result<crate::PixelBuffer, GpuSceneError> {
        let slot = self
            .surfaces
            .get_mut(&surface)
            .unwrap()
            .slots
            .iter_mut()
            .find(|slot| Arc::ptr_eq(&slot.leased, &output.leased))
            .unwrap();
        let mut pixels = crate::PixelBuffer::new(output.width, output.height);
        let stride = pixels.stride() as usize;
        if slot.target.surface.read_pixels(
            &super::target_info(output.width, output.height),
            pixels.data_mut(),
            stride,
            (0, 0),
        ) {
            Ok(pixels)
        } else {
            Err(GpuSceneError::Gpu(super::GpuError::Readback))
        }
    }

    #[cfg(test)]
    pub(crate) fn residency_admissions(&self) -> usize {
        self.residency.admissions
    }

    #[cfg(test)]
    pub(crate) fn disable_residency(&mut self) {
        self.residency = ImageResidency::new(0);
    }

    pub fn probe(&mut self) -> Result<DmabufPlanes, GpuSceneError> {
        let target = self
            .context
            .dmabuf_target(1, 1)
            .map_err(GpuSceneError::Gpu)?;
        clone_planes(target.planes())
    }
}

fn clone_planes(planes: &DmabufPlanes) -> Result<DmabufPlanes, GpuSceneError> {
    Ok(DmabufPlanes {
        fourcc: planes.fourcc,
        modifier: planes.modifier,
        planes: planes
            .planes
            .iter()
            .map(|plane| {
                Ok(DmabufPlane {
                    fd: plane.fd.try_clone().map_err(GpuSceneError::Descriptor)?,
                    offset: plane.offset,
                    stride: plane.stride,
                })
            })
            .collect::<Result<_, GpuSceneError>>()?,
    })
}

impl SceneRenderer for GpuSceneRenderer {
    type Resources = PreparedScene;
    type Output = GpuSceneOutput;
    type Error = GpuSceneError;

    fn render(
        &mut self,
        frame: &SceneFrame<PreparedScene>,
    ) -> Result<GpuSceneOutput, GpuSceneError> {
        let snapshot = frame.snapshot();
        self.residency.prepare(&snapshot.resources().images);
        let scale = snapshot.scale();
        let size = snapshot.resources().size();
        let resized = self
            .surfaces
            .get(&snapshot.surface())
            .is_none_or(|state| (state.retained.width(), state.retained.height()) != size);
        let bytes = crate::surface::checked_pixel_bytes(size.0, size.1, 4)
            .ok_or(GpuSceneError::ResourceBudget)?;
        if resized {
            self.surfaces.remove(&snapshot.surface());
            let reservation = self
                .budget
                .try_reserve(bytes)
                .ok_or(GpuSceneError::ResourceBudget)?;
            let retained = self
                .context
                .offscreen_target(size.0, size.1)
                .map_err(GpuSceneError::Gpu)?;
            self.surfaces.insert(
                snapshot.surface(),
                SurfaceState {
                    retained,
                    slots: Vec::new(),
                    _reservation: reservation,
                },
            );
        }
        let state = self
            .surfaces
            .get_mut(&snapshot.surface())
            .expect("surface target allocated");
        let full = DamageRect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        };
        let damage = if resized || frame.requires_full_replay() {
            vec![full]
        } else {
            crate::DeviceDamage::from_logical(
                &snapshot.plan().damage.logical,
                scale.factor(),
                size.0,
                size.1,
            )
            .rects()
            .to_vec()
        };
        let mut region = skia_safe::Region::new();
        for rect in &damage {
            region.op_rect(
                skia_safe::IRect::from_xywh(
                    rect.x as i32,
                    rect.y as i32,
                    rect.width as i32,
                    rect.height as i32,
                ),
                skia_safe::region::RegionOp::Union,
            );
        }
        if self.residency.images.max_bytes() > 0 {
            self.context.skia.set_resource_cache_limit(
                self.budget
                    .used_bytes()
                    .saturating_add(self.residency.images.max_bytes()),
            );
        }
        #[cfg(test)]
        {
            self.rasterizations += 1;
        }
        let canvas = state.retained.canvas();
        canvas.save();
        canvas.clip_region(&region, None);
        snapshot.resources().draw(canvas);
        canvas.restore();
        let slots = &mut state.slots;
        slots.retain(|slot| slot.leased.load(Ordering::Acquire) != RETIRED);
        for slot in slots.iter_mut() {
            slot.pending.extend_from_slice(&damage);
            if slot.pending.len() > 32 || damage.contains(&full) {
                slot.pending = vec![full];
            }
        }
        self.export(snapshot.surface())
    }

    fn retryable(error: &GpuSceneError) -> bool {
        matches!(error, GpuSceneError::BuffersBusy)
    }

    fn retry(
        &mut self,
        frame: &SceneFrame<PreparedScene>,
    ) -> Result<GpuSceneOutput, GpuSceneError> {
        self.export(frame.snapshot().surface())
    }

    fn remove_surface(&mut self, surface: SceneSurface) {
        self.surfaces.remove(&surface);
        if self.residency.images.max_bytes() > 0 {
            self.context.skia.set_resource_cache_limit(
                self.budget
                    .used_bytes()
                    .saturating_add(self.residency.images.max_bytes()),
            );
        }
    }
}

impl GpuSceneRenderer {
    fn export(&mut self, surface: SceneSurface) -> Result<GpuSceneOutput, GpuSceneError> {
        let state = self
            .surfaces
            .get_mut(&surface)
            .expect("retry retains its raster target");
        let size = (state.retained.width(), state.retained.height());
        let full = DamageRect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        };
        let bytes = crate::surface::checked_pixel_bytes(size.0, size.1, 4)
            .ok_or(GpuSceneError::ResourceBudget)?;
        let slots = &mut state.slots;
        slots.retain(|slot| slot.leased.load(Ordering::Acquire) != RETIRED);
        let index = if let Some(index) = slots
            .iter()
            .position(|slot| slot.leased.load(Ordering::Acquire) == AVAILABLE)
        {
            index
        } else {
            if slots.len() == MAX_SLOTS {
                return Err(GpuSceneError::BuffersBusy);
            }
            let reservation = self
                .budget
                .try_reserve(bytes)
                .ok_or(GpuSceneError::ResourceBudget)?;
            slots.push(Slot {
                target: self
                    .context
                    .dmabuf_target(size.0, size.1)
                    .map_err(GpuSceneError::Gpu)?,
                leased: Arc::new(AtomicU8::new(AVAILABLE)),
                pending: vec![full],
                _reservation: reservation,
            });
            slots.len() - 1
        };
        let slot = &mut slots[index];
        let copied = std::mem::take(&mut slot.pending);
        slot.target.copy_from(&mut state.retained, &copied);
        self.context.submit_for(&slot.target);
        let planes = clone_planes(slot.target.planes())?;
        slot.leased = Arc::new(AtomicU8::new(OUTPUT_OWNED));
        Ok(GpuSceneOutput {
            planes,
            width: size.0,
            height: size.1,
            damage: copied,
            leased: Arc::clone(&slot.leased),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepared_scene::SceneCanvas;
    use crate::scene_thread::{SceneRenderThread, SceneResources};
    use crate::{FractionalScale, FramePaintPlan, PixelBuffer};
    use std::os::unix::fs::MetadataExt;
    use std::time::Duration;

    fn raster_image(color: skia_safe::Color) -> skia_safe::Image {
        let mut surface = skia_safe::surfaces::raster_n32_premul((16, 16)).unwrap();
        surface.canvas().clear(color);
        surface.image_snapshot()
    }

    #[test]
    fn residency_preserves_identities_and_evicts_by_bytes() {
        let mut residency = ImageResidency::new(16 * 16 * 4);
        let red = raster_image(skia_safe::Color::RED);
        let blue = raster_image(skia_safe::Color::BLUE);
        residency.prepare(&[red.clone()]);
        residency.prepare(&[red.clone()]);
        assert_eq!(residency.admissions, 1);
        assert_eq!(
            residency.images.get(&red.unique_id()).unwrap().unique_id(),
            red.unique_id()
        );
        residency.prepare(&[blue]);
        assert_eq!(residency.images.len(), 1);
        assert!(!residency.images.contains_key(&red.unique_id()));
        residency.prepare(&[red]);
        assert_eq!(residency.admissions, 3);
        assert!(residency.images.bytes() <= residency.images.max_bytes());
        let mut disabled = ImageResidency::new(0);
        disabled.prepare(&[raster_image(skia_safe::Color::RED)]);
        assert_eq!(disabled.admissions, 0);
    }

    struct InspectRenderer(GpuSceneRenderer);
    impl SceneRenderer for InspectRenderer {
        type Resources = PreparedScene;
        type Output = (GpuSceneOutput, PixelBuffer);
        type Error = GpuSceneError;
        fn render(
            &mut self,
            frame: &SceneFrame<PreparedScene>,
        ) -> Result<Self::Output, Self::Error> {
            let before = self.0.rasterizations;
            let result = self.0.render(frame);
            assert_eq!(self.0.rasterizations - before, 1);
            let output = result?;
            let pixels = self.0.read_back(frame.snapshot().surface(), &output)?;
            Ok((output, pixels))
        }
        fn retryable(error: &GpuSceneError) -> bool {
            GpuSceneRenderer::retryable(error)
        }
        fn retry(
            &mut self,
            frame: &SceneFrame<PreparedScene>,
        ) -> Result<Self::Output, Self::Error> {
            let before = self.0.rasterizations;
            let output = self.0.retry(frame)?;
            assert_eq!(
                self.0.rasterizations, before,
                "export retry must not rasterize"
            );
            eprintln!(
                "scene_export_retry rasterizations={}",
                self.0.rasterizations - before
            );
            let pixels = self.0.read_back(frame.snapshot().surface(), &output)?;
            Ok((output, pixels))
        }
        fn remove_surface(&mut self, surface: SceneSurface) {
            self.0.remove_surface(surface);
        }
    }

    fn worker() -> Option<SceneRenderThread<InspectRenderer>> {
        let result = SceneRenderThread::spawn(
            2,
            || {
                let nodes = std::fs::read_dir("/dev/dri").map_err(GpuSceneError::Descriptor)?;
                let mut error = GpuSceneError::Gpu(super::super::GpuError::DmabufExport);
                for node in nodes
                    .flatten()
                    .filter(|node| node.file_name().to_string_lossy().starts_with("renderD"))
                {
                    let device = node.metadata().map_err(GpuSceneError::Descriptor)?.rdev();
                    match GpuSceneRenderer::new(device).and_then(|mut renderer| {
                        renderer.probe()?;
                        Ok(renderer)
                    }) {
                        Ok(renderer) => return Ok(InspectRenderer(renderer)),
                        Err(failed) => error = failed,
                    }
                }
                Err(error)
            },
            || {},
        );
        match result {
            Ok(worker) => Some(worker),
            Err(error) if std::env::var_os("MESH_REQUIRE_GPU").is_none() => {
                eprintln!("skipping scene dmabuf test: {error:?}");
                None
            }
            Err(error) => panic!("GPU required: {error:?}"),
        }
    }

    fn record(
        canvas: &mut SceneCanvas,
        color: skia_safe::Color,
        patch: bool,
    ) -> crate::scene_thread::SceneSnapshot<PreparedScene> {
        let mut plan = FramePaintPlan::empty();
        plan.inputs.surface_size = Some((32, 24));
        plan.inputs.resource_revision = 7;
        plan.damage.logical = vec![DamageRect {
            x: 4,
            y: 4,
            width: 8,
            height: 8,
        }]
        .into();
        canvas.begin(&plan);
        canvas.session().with_canvas(|canvas| {
            canvas.clear(skia_safe::Color::RED);
            if patch {
                canvas.draw_rect(
                    skia_safe::Rect::from_xywh(4., 4., 8., 8.),
                    skia_safe::Paint::default().set_color(color),
                );
            }
        });
        let snapshot = canvas.snapshot(FractionalScale::identity()).unwrap();
        assert_eq!(snapshot.resources().revision(), 7);
        snapshot
    }

    #[test]
    fn scene_worker_preserves_partial_damage_and_owns_compositor_leases() {
        let Some(worker) = worker() else {
            return;
        };
        let mut canvas = SceneCanvas::new(32, 24).unwrap();
        let key = canvas.surface;
        let mut held = Vec::new();
        let mut releases = Vec::new();
        for (index, color) in [
            skia_safe::Color::BLUE,
            skia_safe::Color::GREEN,
            skia_safe::Color::WHITE,
        ]
        .into_iter()
        .enumerate()
        {
            let commit = worker.submit(record(&mut canvas, color, true)).unwrap();
            let (output, pixels) = worker
                .wait_completed(key, Duration::from_secs(10))
                .unwrap()
                .output
                .unwrap();
            assert_eq!(pixels.get_pixel(1, 1).r, 255);
            let expected = match index {
                0 => mesh_core_elements::Color {
                    r: 0,
                    g: 0,
                    b: 255,
                    a: 255,
                },
                1 => mesh_core_elements::Color {
                    r: 0,
                    g: 255,
                    b: 0,
                    a: 255,
                },
                _ => mesh_core_elements::Color::WHITE,
            };
            assert_eq!(pixels.get_pixel(6, 6), expected);
            releases.push(output.attached());
            held.push(output);
            assert!(worker.acknowledge_presented(key, commit));
        }
        let commit = worker
            .submit(record(&mut canvas, skia_safe::Color::BLACK, true))
            .unwrap();
        assert!(matches!(
            worker
                .wait_completed(key, Duration::from_secs(10))
                .unwrap()
                .output,
            Err(GpuSceneError::BuffersBusy)
        ));
        let stale_release = releases.remove(0);
        stale_release.clone().release();
        assert!(worker.retry_failed(key, commit));
        let (output, pixels) = worker
            .wait_completed(key, Duration::from_secs(10))
            .unwrap()
            .output
            .unwrap();
        assert_eq!(pixels.get_pixel(6, 6), mesh_core_elements::Color::BLACK);
        assert!(!output.damage.contains(&DamageRect {
            x: 0,
            y: 0,
            width: 32,
            height: 24
        }));
        assert_eq!(pixels.get_pixel(1, 1).r, 255);
        assert_eq!(worker.metrics().submitted, 4);
        assert_eq!(worker.metrics().retried, 1);
        assert!(worker.acknowledge_presented(key, commit));
        releases.push(output.attached());
        stale_release.release();
        worker
            .submit(record(&mut canvas, skia_safe::Color::WHITE, true))
            .unwrap();
        assert!(matches!(
            worker
                .wait_completed(key, Duration::from_secs(10))
                .unwrap()
                .output,
            Err(GpuSceneError::BuffersBusy)
        ));
        releases.pop().unwrap().retire();
        let commit = worker
            .submit(record(&mut canvas, skia_safe::Color::WHITE, true))
            .unwrap();
        let (replacement, pixels) = worker
            .wait_completed(key, Duration::from_secs(10))
            .unwrap()
            .output
            .unwrap();
        assert_eq!(pixels.get_pixel(6, 6), mesh_core_elements::Color::WHITE);
        assert!(worker.acknowledge_presented(key, commit));
        worker.remove_surface(key);
        worker.shutdown().unwrap();
        drop(replacement);
        drop(output);
        drop(held);
        for release in releases {
            release.release();
        }
    }

    /// Damage a refused frame left behind is redrawn by the next scene that
    /// includes it, not only reported to the compositor.
    #[test]
    fn scene_worker_redraws_damage_included_from_an_earlier_frame() {
        let Some(worker) = worker() else {
            return;
        };
        let mut canvas = SceneCanvas::new(32, 24).unwrap();
        let key = canvas.surface;
        let first = worker
            .submit(record(&mut canvas, skia_safe::Color::BLUE, true))
            .unwrap();
        let (first_output, _) = worker
            .wait_completed(key, Duration::from_secs(10))
            .unwrap()
            .output
            .unwrap();
        assert!(worker.acknowledge_presented(key, first));
        let release = first_output.attached();

        let carried = DamageRect {
            x: 20,
            y: 4,
            width: 8,
            height: 8,
        };
        let mut plan = FramePaintPlan::empty();
        plan.inputs.surface_size = Some((32, 24));
        plan.inputs.resource_revision = 7;
        plan.damage.logical = vec![DamageRect {
            x: 4,
            y: 4,
            width: 8,
            height: 8,
        }]
        .into();
        canvas.begin(&plan);
        canvas.session().with_canvas(|canvas| {
            canvas.clear(skia_safe::Color::RED);
            canvas.draw_rect(
                skia_safe::Rect::from_xywh(20., 4., 8., 8.),
                skia_safe::Paint::default().set_color(skia_safe::Color::GREEN),
            );
        });
        canvas.include_damage(&[carried]);
        let second = worker
            .submit(canvas.snapshot(FractionalScale::identity()).unwrap())
            .unwrap();
        let (second_output, pixels) = worker
            .wait_completed(key, Duration::from_secs(10))
            .unwrap()
            .output
            .unwrap();
        assert_eq!(
            pixels.get_pixel(22, 6),
            mesh_core_elements::Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255
            }
        );
        assert!(worker.acknowledge_presented(key, second));
        worker.remove_surface(key);
        worker.shutdown().unwrap();
        drop(second_output);
        drop(first_output);
        release.release();
    }
}
