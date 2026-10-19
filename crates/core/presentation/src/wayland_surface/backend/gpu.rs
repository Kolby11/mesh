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

/// Whether this backend renders on the GPU. Resolved once, on the first
/// canvas request, so the shell never mixes the two within a session unless
/// a surface is explicitly asked to paint on the CPU.
pub(in crate::wayland_surface) enum GpuPresenter {
    Untried,
    Ready(Arc<SceneRenderThread<GpuSceneRenderer>>),
    Unavailable,
}

impl GpuPresenter {
    pub(in crate::wayland_surface) fn worker(
        &mut self,
        dmabuf: &DmabufProtocol,
    ) -> Option<&Arc<SceneRenderThread<GpuSceneRenderer>>> {
        if matches!(self, Self::Untried) {
            *self = match Self::start(dmabuf) {
                Ok(worker) => {
                    tracing::info!("gpu present: scene worker enabled");
                    Self::Ready(Arc::new(worker))
                }
                Err(reason) => {
                    tracing::info!("gpu present: disabled, painting on the CPU: {reason}");
                    Self::Unavailable
                }
            };
        }
        match self {
            Self::Ready(worker) => Some(worker),
            _ => None,
        }
    }

    pub(in crate::wayland_surface) fn disable(&mut self, reason: &PresentationError) {
        tracing::warn!("gpu present: disabled, painting on the CPU: {reason}");
        *self = Self::Unavailable;
    }

    fn start(dmabuf: &DmabufProtocol) -> Result<SceneRenderThread<GpuSceneRenderer>, String> {
        if std::env::var("MESH_RENDER_GPU").is_ok_and(|value| value == "0") {
            return Err("MESH_RENDER_GPU=0".into());
        }
        if dmabuf.state.version().is_none() {
            return Err("compositor lacks zwp_linux_dmabuf_v1".into());
        }
        let device = dmabuf
            .main_device()
            .ok_or("compositor reported no dmabuf main device")?;
        let (send_probe, probe) = std::sync::mpsc::sync_channel(1);
        let worker = SceneRenderThread::spawn(
            64,
            move || {
                let mut renderer = GpuSceneRenderer::new(device)?;
                let planes = renderer.probe()?;
                let _ = send_probe.send(planes);
                Ok(renderer)
            },
            || {},
        )
        .map_err(|error| format!("scene worker: {error:?}"))?;
        let planes = probe.recv().map_err(|error| error.to_string())?;
        if planes.fourcc != DRM_FORMAT_ABGR8888 || !dmabuf.accepts(planes.fourcc, planes.modifier) {
            return Err(format!(
                "compositor does not import format {:#x} modifier {:#x}",
                planes.fourcc, planes.modifier
            ));
        }
        Ok(worker)
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

    pub fn allocate_scene_canvas(&mut self, width: u32, height: u32) -> Option<SceneCanvas> {
        let worker = Arc::clone(self.gpu.worker(&self.state.dmabuf)?);
        let mut canvas = SceneCanvas::new(width.max(1), height.max(1))?;
        canvas.on_retire(move |surface| {
            worker.remove_surface(surface);
        });
        Some(canvas)
    }
}
