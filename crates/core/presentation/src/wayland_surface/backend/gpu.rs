//! GPU presentation: surfaces paint into a retained Skia-GL target, and each
//! present copies the damage into one of a ring of dmabuf-backed buffers that
//! the compositor imports through `zwp_linux_dmabuf_v1`.
//!
//! The EGL context lives on the compositor's main device from the default
//! dmabuf feedback, so buffers are importable without a cross-GPU copy. The
//! retained target plays the role `PixelBuffer` plays for SHM: the shell's
//! partial repaints and scroll shifts stay correct regardless of which ring
//! buffer the compositor still holds.

use super::*;
use mesh_core_render::DamageRect;
use mesh_core_render::gpu::{DRM_FORMAT_ABGR8888, DmabufTarget, GpuContext, GpuTarget};
use smallvec::{SmallVec, smallvec};
use smithay_client_toolkit::dmabuf::{DmabufFeedback, DmabufHandler, DmabufState};
use std::collections::HashSet;
use std::os::fd::AsFd;
use wayland_client::globals::GlobalList;
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1, zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
};

const DMABUF_RING_DEPTH: usize = 2;
const DMABUF_RING_MAX: usize = 3;

/// Dmabuf protocol state kept on [`State`] so dispatch can reach it.
pub(in crate::wayland_surface) struct DmabufProtocol {
    pub(in crate::wayland_surface) state: DmabufState,
    pub(in crate::wayland_surface) feedback: Option<DmabufFeedback>,
    /// `wl_buffer`s attached and not yet released by the compositor.
    pub(in crate::wayland_surface) busy: HashSet<ObjectId>,
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
    }
}

smithay_client_toolkit::delegate_dmabuf!(State);

/// Whether this backend renders on the GPU. Resolved once, on the first
/// canvas request, so the shell never mixes the two within a session unless
/// a surface is explicitly asked to paint on the CPU.
pub(in crate::wayland_surface) enum GpuPresenter {
    Untried,
    Ready(Box<GpuContext>),
    Unavailable,
}

impl GpuPresenter {
    pub(in crate::wayland_surface) fn context(
        &mut self,
        dmabuf: &DmabufProtocol,
    ) -> Option<&mut GpuContext> {
        if matches!(self, Self::Untried) {
            *self = match Self::start(dmabuf) {
                Ok(context) => {
                    tracing::info!(renderer = context.renderer(), "gpu present: enabled");
                    Self::Ready(Box::new(context))
                }
                Err(reason) => {
                    tracing::info!("gpu present: disabled, painting on the CPU: {reason}");
                    Self::Unavailable
                }
            };
        }
        match self {
            Self::Ready(context) => Some(context),
            Self::Untried | Self::Unavailable => None,
        }
    }

    /// Stop presenting GPU buffers for the rest of the session. Surfaces see
    /// the change on their next canvas check and repaint on the CPU.
    pub(in crate::wayland_surface) fn disable(&mut self, reason: &PresentationError) {
        tracing::warn!("gpu present: disabled, painting on the CPU: {reason}");
        *self = Self::Unavailable;
    }

    fn start(dmabuf: &DmabufProtocol) -> Result<GpuContext, String> {
        if std::env::var("MESH_RENDER_GPU").is_ok_and(|value| value == "0") {
            return Err("MESH_RENDER_GPU=0".into());
        }
        if dmabuf.state.version().is_none() {
            return Err("compositor lacks zwp_linux_dmabuf_v1".into());
        }
        let device = dmabuf
            .main_device()
            .ok_or("compositor reported no dmabuf main device")?;
        let mut context = GpuContext::on_drm_device(device).map_err(|error| error.to_string())?;
        if !context.supports_dmabuf_export() {
            return Err(format!("{} cannot export dmabufs", context.renderer()));
        }
        let probe = context
            .dmabuf_target(1, 1)
            .map_err(|error| error.to_string())?;
        let planes = probe.planes();
        if planes.fourcc != DRM_FORMAT_ABGR8888 || !dmabuf.accepts(planes.fourcc, planes.modifier) {
            return Err(format!(
                "compositor does not import format {:#x} modifier {:#x}",
                planes.fourcc, planes.modifier
            ));
        }
        Ok(context)
    }
}

pub(super) struct DmabufSlot {
    generation: u64,
    target: DmabufTarget,
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
    next: usize,
}

impl DmabufRing {
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

    /// Bring a free ring buffer up to date with `source` and submit the copy;
    /// the buffer carries a fence the compositor waits on.
    /// Returns the buffer and the device rects copied into it (this frame's
    /// damage plus what accumulated while it was busy), or `None` when the
    /// compositor holds every buffer.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        &mut self,
        context: &mut GpuContext,
        dmabuf: &mut DmabufProtocol,
        qh: &QueueHandle<State>,
        generations: &mut SurfaceGenerations,
        source: &mut GpuTarget,
        damage: &[DamageRect],
    ) -> Result<Option<(usize, SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]>)>, PresentationError>
    {
        let size = (source.width().max(1), source.height().max(1));
        let full = full_damage(size.0, size.1);
        if self.size != Some(size) {
            for slot in self.slots.drain(..) {
                dmabuf.busy.remove(&slot.buffer.id());
            }
            self.size = Some(size);
            self.next = 0;
        }
        while self.slots.len() < DMABUF_RING_DEPTH {
            let slot = self.allocate(context, dmabuf, qh, generations, size)?;
            self.slots.push(slot);
        }
        for slot in &mut self.slots {
            extend_pending_damage(&mut slot.pending_damage, damage, full);
        }
        let len = self.slots.len();
        let free = (0..len)
            .map(|offset| (self.next + offset) % len)
            .find(|&index| !dmabuf.busy.contains(&self.slots[index].buffer.id()));
        let index = match free {
            Some(index) => index,
            None if len < DMABUF_RING_MAX => {
                let slot = self.allocate(context, dmabuf, qh, generations, size)?;
                self.slots.push(slot);
                len
            }
            None => return Ok(None),
        };
        let slot = &mut self.slots[index];
        let copied = std::mem::take(&mut slot.pending_damage);
        slot.target.copy_from(source, &copied);
        context.submit_for(&slot.target);
        self.next = (index + 1) % self.slots.len();
        Ok(Some((index, copied)))
    }

    fn allocate(
        &self,
        context: &mut GpuContext,
        dmabuf: &DmabufProtocol,
        qh: &QueueHandle<State>,
        generations: &mut SurfaceGenerations,
        (width, height): (u32, u32),
    ) -> Result<DmabufSlot, PresentationError> {
        let target = context
            .dmabuf_target(width, height)
            .map_err(|error| PresentationError::BufferAlloc(format!("dmabuf target: {error}")))?;
        let planes = target.planes();
        // The driver picks the modifier per allocation, and importing an
        // unadvertised pair is a protocol error.
        if !dmabuf.accepts(planes.fourcc, planes.modifier) {
            return Err(PresentationError::BufferAlloc(format!(
                "compositor does not import format {:#x} modifier {:#x}",
                planes.fourcc, planes.modifier
            )));
        }
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
            width as i32,
            height as i32,
            planes.fourcc,
            zwp_linux_buffer_params_v1::Flags::empty(),
            qh,
        );
        params.destroy();
        Ok(DmabufSlot {
            generation: generations.allocate_buffer()?,
            target,
            buffer,
            pending_damage: smallvec![full_damage(width, height)],
        })
    }
}

impl WaylandSurfaceBackend {
    pub fn gpu_available(&mut self) -> bool {
        self.gpu.context(&self.state.dmabuf).is_some()
    }

    /// A retained GPU paint target, or `None` when GPU presentation is
    /// unavailable or the allocation failed.
    pub fn allocate_gpu_target(&mut self, width: u32, height: u32) -> Option<GpuTarget> {
        let context = self.gpu.context(&self.state.dmabuf)?;
        match context.offscreen_target(width.max(1), height.max(1)) {
            Ok(target) => Some(target),
            Err(error) => {
                tracing::warn!("gpu present: {error}; painting this surface on the CPU");
                None
            }
        }
    }
}
