//! Descriptor-only Wayland integration for render-thread GPU scenes.
//! EGL targets and copies stay on the scene worker; protocol buffer release
//! leases prevent worker texture reuse while the compositor holds the storage.

use super::*;
use mesh_core_render::DamageRect;
use mesh_core_render::gpu::DRM_FORMAT_ABGR8888;
use mesh_core_render::gpu::scene::{GpuBufferRelease, GpuSceneOutput, GpuSceneRenderer};
use mesh_core_render::prepared_scene::SceneCanvas;
use mesh_core_render::scene_thread::SceneRenderThread;
use smallvec::SmallVec;
use smithay_client_toolkit::dmabuf::{DmabufFeedback, DmabufHandler, DmabufState};
use std::collections::HashMap;
use std::collections::HashSet;
use std::os::fd::AsFd;
use std::sync::Arc;
use wayland_client::globals::GlobalList;
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1, zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
};

const DMABUF_RING_MAX: usize = 3;

/// Dmabuf protocol state kept on [`State`] so dispatch can reach it.
pub(in crate::wayland_surface) struct DmabufProtocol {
    pub(in crate::wayland_surface) state: DmabufState,
    pub(in crate::wayland_surface) feedback: Option<DmabufFeedback>,
    /// `wl_buffer`s attached and not yet released by the compositor.
    pub(in crate::wayland_surface) busy: HashSet<ObjectId>,
    releases: HashMap<ObjectId, GpuBufferRelease>,
}

impl DmabufProtocol {
    pub(in crate::wayland_surface) fn bind(globals: &GlobalList, qh: &QueueHandle<State>) -> Self {
        let state = DmabufState::new(globals, qh);
        // Version 4 reports the main device in feedback; the object only has
        // to live until the first `done`, so its handle is not retained.
        let _ = state.get_default_feedback(qh);
        Self {
            state,
            feedback: None,
            busy: HashSet::new(),
            releases: HashMap::new(),
        }
    }

    fn main_device(&self) -> Option<u64> {
        self.feedback
            .as_ref()
            .map(|feedback| feedback.main_device() as u64)
            .filter(|device| *device != 0)
    }

    fn accepts(&self, fourcc: u32, modifier: u64) -> bool {
        let matches = |format: u32, candidate: u64| format == fourcc && candidate == modifier;
        match &self.feedback {
            Some(feedback) => feedback
                .format_table()
                .iter()
                .any(|entry| matches(entry.format, entry.modifier)),
            None => self
                .state
                .modifiers()
                .iter()
                .any(|entry| matches(entry.format, entry.modifier)),
        }
    }
}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf.state
    }

    fn dmabuf_feedback(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        proxy: &ZwpLinuxDmabufFeedbackV1,
        feedback: DmabufFeedback,
    ) {
        self.dmabuf.feedback = Some(feedback);
        proxy.destroy();
    }

    fn created(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _params: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        _buffer: WlBuffer,
    ) {
        // Buffers are created with `create_immed`.
    }

    fn failed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _params: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    ) {
        tracing::warn!("gpu present: compositor rejected a dmabuf buffer");
    }

    fn released(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, buffer: &WlBuffer) {
        self.dmabuf.busy.remove(&buffer.id());
        if let Some(release) = self.dmabuf.releases.remove(&buffer.id()) {
            release.release();
        }
    }
}

smithay_client_toolkit::delegate_dmabuf!(State);

/// A started scene worker plus the eventfd it signals after publishing a
/// frame, so the shell's blocking wait wakes for completion.
pub(in crate::wayland_surface) struct GpuRuntime {
    worker: Arc<SceneRenderThread<GpuSceneRenderer>>,
    wake: Arc<std::os::fd::OwnedFd>,
    description: String,
}

/// Whether this backend renders on the GPU. Resolved once, on the first
/// canvas request, so the shell never mixes the two within a session unless
/// a surface is explicitly asked to paint on the CPU. Every outcome carries
/// the reason, readable through [`WaylandSurfaceBackend::render_path`].
pub(in crate::wayland_surface) enum GpuPresenter {
    Untried,
    Ready(GpuRuntime),
    Unavailable(String),
}

impl GpuPresenter {
    pub(in crate::wayland_surface) fn worker(
        &mut self,
        dmabuf: &DmabufProtocol,
    ) -> Option<&Arc<SceneRenderThread<GpuSceneRenderer>>> {
        self.resolve(dmabuf);
        match self {
            Self::Ready(runtime) => Some(&runtime.worker),
            _ => None,
        }
    }

    pub(in crate::wayland_surface) fn wake_fd(&self) -> Option<&std::os::fd::OwnedFd> {
        match self {
            Self::Ready(runtime) => Some(&runtime.wake),
            _ => None,
        }
    }

    fn resolve(&mut self, dmabuf: &DmabufProtocol) {
        if matches!(self, Self::Untried) {
            *self = match Self::start(dmabuf) {
                Ok(runtime) => {
                    tracing::info!("render path: GPU ({})", runtime.description);
                    Self::Ready(runtime)
                }
                Err(reason) => {
                    tracing::info!("render path: CPU/SHM ({reason})");
                    Self::Unavailable(reason)
                }
            };
        }
    }

    pub(in crate::wayland_surface) fn render_path(&mut self, dmabuf: &DmabufProtocol) -> String {
        self.resolve(dmabuf);
        match self {
            Self::Ready(runtime) => format!("GPU ({})", runtime.description),
            Self::Unavailable(reason) => format!("CPU/SHM ({reason})"),
            Self::Untried => unreachable!("resolved above"),
        }
    }

    pub(in crate::wayland_surface) fn disable(&mut self, reason: &PresentationError) {
        tracing::warn!("render path: CPU/SHM, GPU disabled at runtime ({reason})");
        *self = Self::Unavailable(format!("disabled after GPU error: {reason}"));
    }

    fn start(dmabuf: &DmabufProtocol) -> Result<GpuRuntime, String> {
        if std::env::var("MESH_RENDER_GPU").is_ok_and(|value| value == "0") {
            return Err("MESH_RENDER_GPU=0".into());
        }
        if dmabuf.state.version().is_none() {
            return Err("compositor lacks zwp_linux_dmabuf_v1".into());
        }
        let device = dmabuf
            .main_device()
            .ok_or("compositor reported no dmabuf main device")?;
        let wake = Arc::new(
            rustix::event::eventfd(
                0,
                rustix::event::EventfdFlags::CLOEXEC | rustix::event::EventfdFlags::NONBLOCK,
            )
            .map_err(|error| format!("scene wake eventfd: {error}"))?,
        );
        let signal = Arc::clone(&wake);
        let (send_probe, probe) = std::sync::mpsc::sync_channel(1);
        let worker = SceneRenderThread::spawn(
            64,
            move || {
                let mut renderer = GpuSceneRenderer::new(device)?;
                let planes = renderer.probe()?;
                let _ = send_probe.send(planes);
                Ok(renderer)
            },
            move || {
                let _ = rustix::io::write(&*signal, &1u64.to_ne_bytes());
            },
        )
        .map_err(|error| {
            format!(
                "EGL/Ganesh context or dmabuf export unavailable on the compositor device: {error:?}"
            )
        })?;
        let planes = probe.recv().map_err(|error| error.to_string())?;
        if planes.fourcc != DRM_FORMAT_ABGR8888 || !dmabuf.accepts(planes.fourcc, planes.modifier) {
            return Err(format!(
                "compositor does not import format {:#x} modifier {:#x}",
                planes.fourcc, planes.modifier
            ));
        }
        Ok(GpuRuntime {
            worker: Arc::new(worker),
            wake,
            description: format!(
                "device {device:#x}, format {:#x}, modifier {:#x}",
                planes.fourcc, planes.modifier
            ),
        })
    }
}

pub(super) struct DmabufSlot {
    generation: u64,
    output: GpuSceneOutput,
    pub(super) buffer: WlBuffer,
    pending_damage: SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]>,
}

impl Drop for DmabufSlot {
    fn drop(&mut self) {
        self.buffer.destroy();
    }
}

/// The dmabuf buffers one surface presents from, all of the retained
/// target's size.
#[derive(Default)]
pub(in crate::wayland_surface) struct DmabufRing {
    pub(super) slots: Vec<DmabufSlot>,
    size: Option<(u32, u32)>,
}

impl DmabufRing {
    pub(in crate::wayland_surface) fn retire(&mut self, dmabuf: &mut DmabufProtocol) {
        for slot in self.slots.drain(..) {
            dmabuf.busy.remove(&slot.buffer.id());
            if let Some(release) = dmabuf.releases.remove(&slot.buffer.id()) {
                release.retire();
            }
        }
    }

    pub(super) fn slot(&self, index: usize) -> &DmabufSlot {
        &self.slots[index]
    }

    pub(super) fn generation(&self, index: usize) -> u64 {
        self.slots[index].generation
    }

    pub(super) fn restore_damage(
        &mut self,
        index: usize,
        copied: &[DamageRect],
        bounds: DamageRect,
    ) {
        if let Some(slot) = self.slots.get_mut(index) {
            restore_pending_damage(&mut slot.pending_damage, copied, bounds);
        }
    }

    pub(super) fn has_free_slot(&self, busy: &HashSet<ObjectId>) -> bool {
        self.slots
            .iter()
            .any(|slot| !busy.contains(&slot.buffer.id()))
    }

    /// Import only owned descriptors. Raster, copies, fences and texture
    /// destruction run on the scene worker, never in the Wayland dispatcher.
    pub(super) fn prepare(
        &mut self,
        dmabuf: &mut DmabufProtocol,
        qh: &QueueHandle<State>,
        generations: &mut SurfaceGenerations,
        output: GpuSceneOutput,
    ) -> Result<Option<(usize, SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]>)>, PresentationError>
    {
        let size = (output.width, output.height);
        if self.size != Some(size) {
            for slot in self.slots.drain(..) {
                dmabuf.busy.remove(&slot.buffer.id());
                if let Some(release) = dmabuf.releases.remove(&slot.buffer.id()) {
                    release.retire();
                }
            }
            self.size = Some(size);
        }
        let index = self
            .slots
            .iter()
            .position(|slot| !dmabuf.busy.contains(&slot.buffer.id()))
            .unwrap_or(self.slots.len());
        if index >= DMABUF_RING_MAX {
            return Ok(None);
        }
        let planes = &output.planes;
        if !dmabuf.accepts(planes.fourcc, planes.modifier) {
            return Err(PresentationError::BufferAlloc(format!(
                "compositor does not import format {:#x} modifier {:#x}",
                planes.fourcc, planes.modifier
            )));
        }
        let generation = generations.allocate_buffer()?;
        let params = dmabuf
            .state
            .create_params(qh)
            .map_err(|error| PresentationError::BufferAlloc(format!("dmabuf params: {error}")))?;
        for (index, plane) in planes.planes.iter().enumerate() {
            params.add(
                plane.fd.as_fd(),
                index as u32,
                plane.offset,
                plane.stride,
                planes.modifier,
            );
        }
        let (buffer, params) = params.create_immed(
            size.0 as i32,
            size.1 as i32,
            planes.fourcc,
            zwp_linux_buffer_params_v1::Flags::empty(),
            qh,
        );
        params.destroy();
        let copied = output.damage.iter().copied().collect();
        let slot = DmabufSlot {
            generation,
            output,
            buffer,
            pending_damage: SmallVec::new(),
        };
        if index == self.slots.len() {
            self.slots.push(slot);
        } else {
            self.slots[index] = slot;
        }
        Ok(Some((index, copied)))
    }

    pub(super) fn mark_attached(&self, index: usize, dmabuf: &mut DmabufProtocol) {
        let slot = &self.slots[index];
        dmabuf
            .releases
            .insert(slot.buffer.id(), slot.output.attached());
    }
}

impl WaylandSurfaceBackend {
    pub fn gpu_available(&mut self) -> bool {
        self.gpu.worker(&self.state.dmabuf).is_some()
    }

    /// Which render path is active and why, for diagnostics.
    pub fn render_path(&mut self) -> String {
        self.gpu.render_path(&self.state.dmabuf)
    }

    pub fn allocate_scene_canvas(&mut self, width: u32, height: u32) -> Option<SceneCanvas> {
        let worker = Arc::clone(self.gpu.worker(&self.state.dmabuf)?);
        let mut canvas = SceneCanvas::new(width.max(1), height.max(1))?;
        canvas.on_retire(move |surface| {
            worker.remove_surface(surface);
        });
        Some(canvas)
    }
}

/// A frame handed to the scene worker and not yet imported by the compositor.
pub(in crate::wayland_surface) struct PendingScene {
    surface_id: String,
    scene: mesh_core_render::scene_thread::SceneSurface,
    commit: u64,
    /// Compositor object the frame was painted for; a recreated role must
    /// not receive pixels recorded for its predecessor.
    object: u64,
    damage: Vec<DamageRect>,
    submitted: Instant,
    waiting_for_release: bool,
}

/// How long a submitted frame may stay unpublished before the worker is
/// considered wedged and the backend falls back to SHM.
const SCENE_COMPLETION_DEADLINE: Duration = Duration::from_secs(2);

impl WaylandSurfaceBackend {
    /// Hand the finished recording to the worker. The commit, import and
    /// attach happen in [`Self::poll_scene_completions`] once the worker
    /// publishes the fenced output, so the caller can start its next frame.
    pub(super) fn submit_scene(
        &mut self,
        surface_id: &str,
        scene: &mut SceneCanvas,
        damage: &[DamageRect],
    ) -> PresentStatus {
        if self.scene_in_flight(surface_id) {
            return PresentStatus::NotReady;
        }
        let Some(worker) = self.gpu.worker(&self.state.dmabuf).cloned() else {
            return PresentStatus::NotReady;
        };
        let Some(entry) = self.state.surfaces.get(surface_id) else {
            return PresentStatus::SurfaceMissing;
        };
        let (scale, object) = (entry.scale, entry.surface_generation().object);
        scene.include_damage(damage);
        let Some(snapshot) = scene.snapshot(mesh_core_render::FractionalScale::new(scale)) else {
            self.gpu.disable(&PresentationError::BufferAlloc(
                "scene recording is incomplete".into(),
            ));
            return PresentStatus::NotReady;
        };
        match worker.submit(snapshot) {
            Ok(commit) => {
                self.scene_pending.push(PendingScene {
                    surface_id: surface_id.to_string(),
                    scene: scene.surface,
                    commit,
                    object,
                    damage: damage.to_vec(),
                    submitted: Instant::now(),
                    waiting_for_release: false,
                });
                PresentStatus::Submitted
            }
            Err(error) => {
                self.gpu.disable(&PresentationError::BufferAlloc(format!(
                    "scene submit: {error:?}"
                )));
                PresentStatus::NotReady
            }
        }
    }

    pub(super) fn scene_in_flight(&self, surface_id: &str) -> bool {
        self.scene_pending
            .iter()
            .any(|pending| pending.surface_id == surface_id)
    }

    /// The oldest queued/active job's fault deadline. Completed and
    /// release-blocked frames wait for protocol events instead.
    pub(super) fn scene_deadline(&self) -> Option<Instant> {
        let GpuPresenter::Ready(runtime) = &self.gpu else {
            return None;
        };
        self.scene_pending
            .iter()
            .filter(|pending| {
                runtime
                    .worker
                    .completion_pending(pending.scene, pending.commit)
            })
            .map(|pending| pending.submitted + SCENE_COMPLETION_DEADLINE)
            .min()
    }

    /// Forget in-flight frames of a hidden or destroyed surface. The worker
    /// output is dropped unseen, which returns its buffer lease.
    pub(super) fn cancel_scene(&mut self, surface_id: &str) {
        self.scene_pending
            .retain(|pending| pending.surface_id != surface_id);
    }

    /// Import and commit every frame the worker has published. Frames commit
    /// in submission order, and a popup never commits before a still-pending
    /// parent. Outcomes accumulate for the shell to drain.
    pub(super) fn poll_scene_completions(&mut self) {
        if let Some(fd) = self.gpu.wake_fd() {
            let mut counter = [0u8; 8];
            let _ = rustix::io::read(fd, &mut counter);
        }
        if self.scene_pending.is_empty() {
            return;
        }
        let Some(worker) = self.gpu.worker(&self.state.dmabuf).cloned() else {
            for pending in std::mem::take(&mut self.scene_pending) {
                self.scene_results.push(crate::SceneCompletion {
                    surface_id: pending.surface_id,
                    presented: false,
                });
            }
            return;
        };
        let pending = std::mem::take(&mut self.scene_pending);
        let mut waiting: Vec<PendingScene> = Vec::new();
        for mut pending in pending {
            let parent_waiting = match self.state.surfaces.get(&pending.surface_id) {
                Some(SurfaceEntry {
                    role: WaylandRole::Popup(role),
                    ..
                }) => waiting
                    .iter()
                    .any(|earlier| earlier.surface_id == role.parent_id),
                _ => false,
            };
            if parent_waiting {
                waiting.push(pending);
                continue;
            }
            if !worker.is_current(pending.scene, pending.commit) {
                self.scene_results.push(crate::SceneCompletion {
                    surface_id: pending.surface_id,
                    presented: false,
                });
                continue;
            }
            if pending.waiting_for_release {
                let Some(entry) = self.state.surfaces.get(&pending.surface_id) else {
                    self.scene_results.push(crate::SceneCompletion {
                        surface_id: pending.surface_id,
                        presented: false,
                    });
                    continue;
                };
                if !entry.configured || entry.surface_generation().object != pending.object {
                    self.scene_results.push(crate::SceneCompletion {
                        surface_id: pending.surface_id,
                        presented: false,
                    });
                    continue;
                }
                if !entry.dmabuf_slot_available(&self.state.dmabuf.busy) {
                    waiting.push(pending);
                    continue;
                }
                if !worker.retry_failed(pending.scene, pending.commit) {
                    self.scene_results.push(crate::SceneCompletion {
                        surface_id: pending.surface_id,
                        presented: false,
                    });
                    continue;
                }
                pending.waiting_for_release = false;
                pending.submitted = Instant::now();
                waiting.push(pending);
                continue;
            }
            let Some(done) = worker.take_completed(pending.scene) else {
                if pending.submitted.elapsed() > SCENE_COMPLETION_DEADLINE {
                    self.gpu.disable(&PresentationError::BufferAlloc(
                        "scene worker did not complete the frame".into(),
                    ));
                    self.scene_results.push(crate::SceneCompletion {
                        surface_id: pending.surface_id,
                        presented: false,
                    });
                } else {
                    waiting.push(pending);
                }
                continue;
            };
            if done.commit == pending.commit
                && matches!(
                    done.output,
                    Err(mesh_core_render::gpu::scene::GpuSceneError::BuffersBusy)
                )
            {
                let Some(entry) =
                    self.state
                        .surfaces
                        .get_mut(&pending.surface_id)
                        .filter(|entry| {
                            entry.configured && entry.surface_generation().object == pending.object
                        })
                else {
                    self.scene_results.push(crate::SceneCompletion {
                        surface_id: pending.surface_id,
                        presented: false,
                    });
                    continue;
                };
                entry.mark_buffer_backpressured();
                pending.waiting_for_release = true;
                if entry.dmabuf_slot_available(&self.state.dmabuf.busy) {
                    if !worker.retry_failed(pending.scene, pending.commit) {
                        self.scene_results.push(crate::SceneCompletion {
                            surface_id: pending.surface_id,
                            presented: false,
                        });
                        continue;
                    }
                    pending.waiting_for_release = false;
                    pending.submitted = Instant::now();
                }
                waiting.push(pending);
                continue;
            }
            let presented = done.commit == pending.commit
                && self.import_scene_output(&worker, &pending, done.output);
            self.scene_results.push(crate::SceneCompletion {
                surface_id: pending.surface_id,
                presented,
            });
        }
        waiting.append(&mut self.scene_pending);
        self.scene_pending = waiting;
    }

    fn import_scene_output(
        &mut self,
        worker: &SceneRenderThread<GpuSceneRenderer>,
        pending: &PendingScene,
        output: Result<GpuSceneOutput, mesh_core_render::gpu::scene::GpuSceneError>,
    ) -> bool {
        use mesh_core_render::gpu::scene::GpuSceneError;
        let surface_id = pending.surface_id.as_str();
        let output = match output {
            Ok(output) => output,
            Err(GpuSceneError::BuffersBusy) => {
                if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
                    entry.mark_buffer_backpressured();
                }
                return false;
            }
            Err(error) => {
                self.gpu
                    .disable(&PresentationError::BufferAlloc(error.to_string()));
                return false;
            }
        };
        let output_size = self.output_logical_size_for_surface(surface_id);
        let qh = self.state.qh.clone();
        let state = &mut self.state;
        let Some(entry) = state.surfaces.get_mut(surface_id) else {
            return false;
        };
        if !entry.configured || entry.surface_generation().object != pending.object {
            return false;
        }
        let (logical_w, logical_h) = entry.resolved_extent(output_size);
        let scale = entry.scale;
        let (physical_w, physical_h) = (output.width, output.height);
        let mut device_damage: SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]> =
            mesh_core_render::DeviceDamage::from_logical(
                &pending.damage,
                scale,
                physical_w,
                physical_h,
            )
            .rects()
            .iter()
            .copied()
            .collect();
        if device_damage.is_empty() {
            device_damage.push(full_damage(physical_w, physical_h));
        }
        let prepared = match entry.copy_into_dmabuf_buffer(&mut state.dmabuf, &qh, output) {
            Ok(Some((index, copied))) => (BufferSlot::Dmabuf(index), copied),
            Ok(None) => return false,
            Err(error) => {
                self.gpu.disable(&error);
                return false;
            }
        };
        match self.commit_prepared(
            surface_id,
            prepared.0,
            &prepared.1,
            (logical_w, logical_h),
            (physical_w, physical_h),
            scale,
            &device_damage,
        ) {
            Ok(PresentStatus::Presented) => {
                worker.acknowledge_presented(pending.scene, pending.commit);
                true
            }
            Ok(_) => false,
            Err(error) => {
                tracing::warn!("gpu present: committing surface '{surface_id}' failed: {error}");
                false
            }
        }
    }
}
