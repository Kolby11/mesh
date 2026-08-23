mod dev_window;
mod wayland_surface;

use std::collections::{HashMap, HashSet};
use std::os::unix::io::BorrowedFd;
use std::sync::Arc;

use mesh_core_render::{DamageRect, PixelBuffer};
use mesh_core_wayland::WindowStates;

pub use dev_window::{DevWindowEvent as WindowEvent, DevWindowKeyEvent as WindowKeyEvent, KeyMods};
pub use wayland_surface::{
    LayerSurfaceSizePolicy, PopupAnchor, PopupConfig, PopupConstraint, PopupGravity,
    PopupPlacement, SurfaceConfig, SurfacePadding,
};

/// The Wayland seat and button-press serial that authorize an `xdg_popup` grab.
///
/// The protocol identity is deliberately separate from the button code carried
/// by [`WindowEvent::PointerButton`]. Developer-window and test backends do not
/// have a protocol seat or serial, while a Wayland popup must use the exact
/// seat that delivered the triggering press rather than a process-global
/// "active" seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PointerButtonIdentity {
    pub seat_id: u32,
    pub serial: u32,
}

/// Linux input-event code for the primary pointer button (`BTN_LEFT`).
pub const PRIMARY_POINTER_BUTTON: u32 = 0x110;

/// The surrounding text state published for the currently focused editable
/// element. Offsets use UTF-8 bytes, matching `zwp_text_input_v3`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInputState {
    pub surrounding_text: String,
    pub cursor: usize,
    pub anchor: usize,
}

impl TextInputState {
    pub fn new(surrounding_text: impl Into<String>, cursor: usize, anchor: usize) -> Self {
        Self {
            surrounding_text: surrounding_text.into(),
            cursor,
            anchor,
        }
    }
}

pub(crate) fn validate_text_input_state(
    surface_id: Option<&str>,
    state: Option<&TextInputState>,
) -> Result<(), PresentationError> {
    if let Some(snapshot) = state {
        let text_len = snapshot.surrounding_text.len();
        if text_len > 4000 {
            return Err(PresentationError::TextInputInvalid(
                "surrounding text exceeds the 4000-byte Wayland message limit".into(),
            ));
        }
        for (name, offset) in [("cursor", snapshot.cursor), ("anchor", snapshot.anchor)] {
            if offset > text_len || !snapshot.surrounding_text.is_char_boundary(offset) {
                return Err(PresentationError::TextInputInvalid(format!(
                    "text-input {name} offset {offset} is not a UTF-8 boundary in {text_len} bytes"
                )));
            }
        }
    }

    match (surface_id.filter(|id| !id.is_empty()), state) {
        (Some(_), Some(_)) | (None, None) => Ok(()),
        (Some(_), None) => Err(PresentationError::TextInputInvalid(
            "a focused surface requires a text-input state".into(),
        )),
        (None, Some(_)) => Err(PresentationError::TextInputInvalid(
            "text-input state requires a focused surface".into(),
        )),
    }
}

use dev_window::DevWindowBackend;
use wayland_surface::WaylandSurfaceBackend;

/// Why a blocking wait returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitReason {
    /// The Wayland connection fd became readable.
    WaylandEvent,
    /// The IPC/backend eventfd was signaled.
    IpcEvent,
    /// The deadline expired before any fd became ready.
    DeadlineExpired,
}

impl WaitReason {
    /// Profiling trigger-kind string suitable for `ProfilingStage::SchedulerIdle`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WaylandEvent => "wayland_event",
            Self::IpcEvent => "ipc_event",
            Self::DeadlineExpired => "deadline_expired",
        }
    }
}

/// Result of a blocking wait on the presentation backend.
#[derive(Debug, Clone, Copy)]
pub struct WaitResult {
    pub reason: WaitReason,
}

impl WaitResult {
    pub fn deadline_expired() -> Self {
        Self {
            reason: WaitReason::DeadlineExpired,
        }
    }
}

/// Outcome of attempting to commit a surface buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentStatus {
    /// The backend accepted the frame (or the surface was intentionally hidden).
    Presented,
    /// The compositor has not configured the surface yet. The caller must keep
    /// the frame and retry after presentation events are dispatched.
    NotReady,
    /// The compositor surface is no longer present. The caller must retain the
    /// frame and retry configuration before treating a later frame as delivered.
    SurfaceMissing,
}

/// Outcome of committing compositor state that does not require a new pixel
/// buffer, such as opaque, blur, input-region, or window-geometry changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceStateStatus {
    /// Pending surface state was committed to the compositor.
    Committed,
    /// No compositor state was pending, so no commit was needed.
    Unchanged,
    /// The compositor has not configured the surface yet.
    NotReady,
    /// The compositor surface no longer exists.
    SurfaceMissing,
}

/// Monotonic identities for the compositor object and the protocol state it
/// has accepted. A replacement role gets a new object generation, configure
/// callbacks advance the configure generation, allocated SHM slots get a
/// buffer generation, and each requested frame gets a new frame generation.
/// Presentation callbacks must match the object and frame generations that are
/// still current before they can release pacing. The buffer generation is the
/// identity of the slot whose contents were most recently committed. The
/// output generation advances whenever the surface's output association or
/// the associated output's geometry is revised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceGeneration {
    pub object: u64,
    pub configure: u64,
    pub frame: u64,
    /// Zero until the first buffer commit on this compositor object.
    pub buffer: u64,
    /// Zero until the compositor reports the surface's first output.
    pub output: u64,
}

/// Outputs currently associated with a surface, ordered by most recent
/// `wl_surface::enter`. Wayland surfaces may overlap more than one output;
/// retaining the complete membership prevents a late leave for one output from
/// clearing another output that still owns the surface. The last member is the
/// deterministic output used for geometry that requires one logical extent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutputMembership<T> {
    members: Vec<T>,
}

impl<T> Default for OutputMembership<T> {
    fn default() -> Self {
        Self {
            members: Vec::new(),
        }
    }
}

impl<T: PartialEq> OutputMembership<T> {
    pub(crate) fn enter(&mut self, output: T) -> bool {
        if let Some(index) = self.members.iter().position(|member| member == &output) {
            if index + 1 == self.members.len() {
                return false;
            }
            let output = self.members.remove(index);
            self.members.push(output);
            return true;
        }
        self.members.push(output);
        true
    }

    pub(crate) fn leave(&mut self, output: &T) -> bool {
        let Some(index) = self.members.iter().position(|member| member == output) else {
            return false;
        };
        self.members.remove(index);
        true
    }

    pub(crate) fn contains(&self, output: &T) -> bool {
        self.members.iter().any(|member| member == output)
    }

    pub(crate) fn active(&self) -> Option<&T> {
        self.members.last()
    }

    pub(crate) fn as_slice(&self) -> &[T] {
        &self.members
    }
}

/// Protocol versions and optional globals negotiated when the Wayland
/// connection was established. A zero version means that the compositor did
/// not advertise that protocol. The generation identifies the revision of
/// this capability snapshot within its presentation connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NegotiatedCapabilities {
    /// Monotonic within one presentation backend connection.
    pub generation: u64,
    pub layer_shell_version: u32,
    pub xdg_shell_version: u32,
    pub viewporter_version: u32,
    pub fractional_scale_version: u32,
    pub blur_version: u32,
    pub activation_version: u32,
    pub focus_grab_version: u32,
    pub pointer_gestures_version: u32,
    pub text_input_version: u32,
}

impl NegotiatedCapabilities {
    /// Construct a snapshot from advertised versions after clamping each
    /// version to the protocol surface supported by this build.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_versions(
        generation: u64,
        layer_shell_version: u32,
        xdg_shell_version: u32,
        viewporter_version: u32,
        fractional_scale_version: u32,
        blur_version: u32,
        activation_version: u32,
        focus_grab_version: u32,
        pointer_gestures_version: u32,
        text_input_version: u32,
    ) -> Self {
        Self {
            generation,
            layer_shell_version: layer_shell_version.min(4),
            xdg_shell_version: xdg_shell_version.min(6),
            viewporter_version: viewporter_version.min(1),
            fractional_scale_version: fractional_scale_version.min(1),
            blur_version: blur_version.min(1),
            activation_version: activation_version.min(1),
            focus_grab_version: focus_grab_version.min(1),
            pointer_gestures_version: pointer_gestures_version.min(3),
            text_input_version: text_input_version.min(1),
        }
    }

    /// `xdg_popup.reposition` was introduced by xdg-shell version 3.
    pub const fn supports_xdg_popup_reposition(self) -> bool {
        self.xdg_shell_version >= 3
    }

    /// Reactive popup configures use the xdg-positioner `set_reactive` request,
    /// which was introduced by the same xdg-shell version as repositioning.
    pub const fn supports_xdg_popup_reactive_positioner(self) -> bool {
        self.xdg_shell_version >= 3
    }
}

/// A compositor-owned surface lifecycle transition that the shell must
/// reconcile with its retained surface targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceLifecycleEvent {
    /// The compositor closed a layer surface. The shell may keep its component
    /// alive, but must invalidate the accepted object state before retrying.
    Closed { surface_id: String },
    /// The compositor dismissed an xdg popup, usually because of outside-click
    /// handling or destruction of its parent surface.
    Dismissed { surface_id: String },
    /// The Wayland connection was lost while this surface was live. The
    /// compositor object is gone and the shell must invalidate its accepted
    /// configuration before attempting recovery.
    Lost { surface_id: String, reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum PresentationError {
    #[error("failed to connect to Wayland: {0}")]
    WaylandConnect(String),

    #[error("failed to create surface: {0}")]
    SurfaceCreate(String),

    #[error("protocol not supported: {0}")]
    ProtocolUnsupported(String),

    #[error("buffer allocation failed: {0}")]
    BufferAlloc(String),

    #[error("buffer copy failed: {0}")]
    BufferCopy(String),

    #[error("buffer attach failed: {0}")]
    BufferAttach(String),

    #[error("invalid text-input state: {0}")]
    TextInputInvalid(String),

    #[error("Wayland connection lost: {0}")]
    ConnectionLost(String),
}

pub struct PresentationEngine {
    backend: Backend,
}

enum Backend {
    WaylandSurface(Box<WaylandSurfaceBackend>),
    DevWindow(DevWindowBackend),
    // Boxed for the same reason as the Wayland backend: the testing backend's
    // recording vectors and maps would otherwise set the size of every
    // `PresentationEngine`.
    Testing(Box<TestingBackend>),
}

#[derive(Default)]
struct TestingBackend {
    popup_supported: bool,
    configure_error: Option<String>,
    popup_configure_error: Option<String>,
    popup_configs: HashMap<String, PopupConfig>,
    surface_configs: HashMap<String, SurfaceConfig>,
    surface_config_history: Vec<(String, SurfaceConfig)>,
    destroyed_popups: Vec<String>,
    destroyed_surfaces: Vec<String>,
    destroyed_popup_ids: HashSet<String>,
    destroyed_surface_ids: HashSet<String>,
    lifecycle_events: Vec<SurfaceLifecycleEvent>,
    connection_lost: Option<String>,
    close_requests: Vec<String>,
    events: Vec<WindowEvent>,
    presented: Vec<String>,
    presented_damage: Vec<(String, Vec<DamageRect>)>,
    pending_surface_states: HashSet<String>,
    surface_state_commits: Vec<String>,
    completed_frames: usize,
    window_states: HashMap<String, WindowStates>,
    unconfigured_surfaces: HashSet<String>,
    missing_surfaces: HashSet<String>,
    frame_pending_surfaces: HashSet<String>,
    buffer_backpressured_surfaces: HashSet<String>,
    surface_scales: HashMap<String, f32>,
    full_redraw_surfaces: HashSet<String>,
    surface_outputs: HashMap<String, OutputMembership<u32>>,
    surface_output_generations: HashMap<String, u64>,
    text_input_state: Option<(String, TextInputState)>,
    text_input_surface: Option<String>,
    text_input_pending: TestingPendingTextInput,
}

#[derive(Default)]
struct TestingPendingTextInput {
    /// `Some(None)` represents the protocol's nullable empty preedit, while
    /// `None` means no preedit event was received in this done group.
    preedit: Option<Option<String>>,
    preedit_cursor_begin: i32,
    preedit_cursor_end: i32,
    commit: Option<String>,
    before_bytes: usize,
    after_bytes: usize,
}

impl TestingPendingTextInput {
    fn is_empty(&self) -> bool {
        self.preedit.is_none()
            && self.commit.is_none()
            && self.before_bytes == 0
            && self.after_bytes == 0
    }

    fn take_edit(&mut self, surface_id: String) -> Option<WindowEvent> {
        if self.is_empty() {
            return None;
        }
        let event = WindowEvent::TextInputEdit {
            surface_id: Arc::from(surface_id),
            preedit_present: self.preedit.is_some(),
            preedit: self.preedit.take().flatten().map(Arc::from),
            preedit_cursor_begin: self.preedit_cursor_begin,
            preedit_cursor_end: self.preedit_cursor_end,
            commit: self.commit.take().map(Arc::from),
            before_bytes: std::mem::take(&mut self.before_bytes),
            after_bytes: std::mem::take(&mut self.after_bytes),
        };
        self.preedit_cursor_begin = 0;
        self.preedit_cursor_end = 0;
        Some(event)
    }
}

fn drop_testing_events_for_surface(backend: &mut TestingBackend, surface_id: &str) {
    backend
        .events
        .retain(|event| event_surface_id(event) != surface_id);
}

fn clear_testing_presentation_waits(backend: &mut TestingBackend, surface_id: &str) {
    backend.frame_pending_surfaces.remove(surface_id);
    backend.buffer_backpressured_surfaces.remove(surface_id);
}

fn clear_testing_surface_runtime_state(backend: &mut TestingBackend, surface_id: &str) {
    clear_testing_presentation_waits(backend, surface_id);
    backend.surface_scales.remove(surface_id);
    backend.full_redraw_surfaces.remove(surface_id);
    backend.surface_outputs.remove(surface_id);
    backend.surface_output_generations.remove(surface_id);
}

/// Keep the deterministic backend's retained text-input publication aligned
/// with the compositor identity it represents. The real Wayland backend drops
/// this snapshot while tearing down a surface, so a later shell sync cannot
/// accidentally replay surrounding text for a destroyed object.
fn clear_testing_text_input_transaction_for_surface(
    backend: &mut TestingBackend,
    surface_id: &str,
) -> bool {
    if backend.text_input_surface.as_deref() == Some(surface_id) {
        backend.text_input_surface = None;
        backend.text_input_pending = TestingPendingTextInput::default();
        true
    } else {
        false
    }
}

fn clear_testing_text_input_for_surface(backend: &mut TestingBackend, surface_id: &str) {
    clear_testing_surface_runtime_state(backend, surface_id);
    clear_testing_text_input_transaction_for_surface(backend, surface_id);
    if backend
        .text_input_state
        .as_ref()
        .is_some_and(|(active_surface_id, _)| active_surface_id == surface_id)
    {
        backend.text_input_state = None;
    }
}

impl PresentationEngine {
    pub fn select() -> Self {
        let forced = std::env::var("MESH_BACKEND").ok();
        let want_dev = forced.as_deref() == Some("dev-window");
        let want_wayland = forced.as_deref() == Some("layer-shell");
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();

        let backend = if !want_dev && (want_wayland || wayland) {
            match WaylandSurfaceBackend::new() {
                Ok(bridge) => {
                    tracing::info!("using wayland surface bridge");
                    Backend::WaylandSurface(Box::new(bridge))
                }
                Err(err) => {
                    tracing::warn!(
                        "failed to initialise wayland surface bridge, falling back to dev window: {err}"
                    );
                    tracing::info!("using dev-window bridge");
                    Backend::DevWindow(DevWindowBackend::new())
                }
            }
        } else {
            tracing::info!("using dev-window bridge");
            Backend::DevWindow(DevWindowBackend::new())
        };

        Self { backend }
    }

    #[doc(hidden)]
    pub fn testing_with_popup_support(popup_supported: bool) -> Self {
        Self {
            backend: Backend::Testing(Box::new(TestingBackend {
                popup_supported,
                ..TestingBackend::default()
            })),
        }
    }

    /// Stand in for a compositor configure that put a window into the given
    /// states, so shell-side projection can be tested without a compositor.
    #[doc(hidden)]
    pub fn testing_set_window_states(&mut self, surface_id: &str, states: WindowStates) {
        if let Backend::Testing(backend) = &mut self.backend {
            backend.window_states.insert(surface_id.to_string(), states);
        }
    }

    /// Stand in for the configure lifecycle in shell/presentation tests.
    #[doc(hidden)]
    pub fn testing_set_surface_configured(&mut self, surface_id: &str, configured: bool) {
        if let Backend::Testing(backend) = &mut self.backend {
            if configured {
                backend.unconfigured_surfaces.remove(surface_id);
            } else {
                backend.unconfigured_surfaces.insert(surface_id.to_string());
            }
        }
    }

    /// Stand in for an outstanding `wl_surface.frame` callback in presentation
    /// scheduler tests. This is intentionally separate from buffer ownership:
    /// a released buffer may still be waiting for a frame callback, and vice
    /// versa.
    #[doc(hidden)]
    pub fn testing_set_surface_frame_pending(&mut self, surface_id: &str, pending: bool) {
        if let Backend::Testing(backend) = &mut self.backend {
            if pending {
                backend
                    .frame_pending_surfaces
                    .insert(surface_id.to_string());
            } else {
                backend.frame_pending_surfaces.remove(surface_id);
            }
        }
    }

    /// Stand in for all SHM slots being owned by the compositor. Clearing this
    /// flag represents a dispatched `wl_buffer.release` event.
    #[doc(hidden)]
    pub fn testing_set_surface_buffer_backpressured(&mut self, surface_id: &str, blocked: bool) {
        if let Backend::Testing(backend) = &mut self.backend {
            if blocked {
                backend
                    .buffer_backpressured_surfaces
                    .insert(surface_id.to_string());
            } else {
                backend.buffer_backpressured_surfaces.remove(surface_id);
            }
        }
    }

    /// Simulate a compositor preferred-scale update for a live surface. The
    /// bounds mirror `wp_fractional_scale_v1`'s supported 0.5x..4x range;
    /// accepted changes invalidate retained pixels exactly as the Wayland
    /// backend does, so the next shell pass must repaint the full buffer.
    #[doc(hidden)]
    pub fn testing_set_surface_scale(&mut self, surface_id: &str, scale: f32) -> bool {
        let Backend::Testing(backend) = &mut self.backend else {
            return false;
        };
        if !scale.is_finite()
            || backend.missing_surfaces.contains(surface_id)
            || !(backend.surface_configs.contains_key(surface_id)
                || backend.popup_configs.contains_key(surface_id))
        {
            return false;
        }
        let scale = scale.clamp(0.5, 4.0);
        let previous = backend
            .surface_scales
            .get(surface_id)
            .copied()
            .unwrap_or(1.0);
        if (previous - scale).abs() > f32::EPSILON {
            backend.surface_scales.insert(surface_id.to_string(), scale);
            backend.full_redraw_surfaces.insert(surface_id.to_string());
        }
        true
    }

    /// Simulate `wl_surface::enter`/`leave` output membership for a live
    /// testing surface. The output id is a deterministic stand-in for the
    /// compositor's `wl_output` object identity. Entered outputs are retained
    /// together, with the most recent one selected for geometry resolution;
    /// every real membership change invalidates retained pixels.
    #[doc(hidden)]
    pub fn testing_set_surface_output(
        &mut self,
        surface_id: &str,
        output_id: u32,
        entered: bool,
    ) -> bool {
        let Backend::Testing(backend) = &mut self.backend else {
            return false;
        };
        if backend.connection_lost.is_some()
            || backend.missing_surfaces.contains(surface_id)
            || (!backend.surface_configs.contains_key(surface_id)
                && !backend.popup_configs.contains_key(surface_id))
        {
            return false;
        }

        let membership = backend
            .surface_outputs
            .entry(surface_id.to_string())
            .or_default();
        let changed = if entered {
            membership.enter(output_id)
        } else {
            membership.leave(&output_id)
        };
        if !changed {
            return false;
        }

        let generation = backend
            .surface_output_generations
            .entry(surface_id.to_string())
            .or_default();
        *generation = generation.saturating_add(1);
        backend.full_redraw_surfaces.insert(surface_id.to_string());
        true
    }

    #[doc(hidden)]
    pub fn testing_surface_outputs(&self, surface_id: &str) -> Vec<u32> {
        match &self.backend {
            Backend::Testing(backend) => backend
                .surface_outputs
                .get(surface_id)
                .map(|outputs| outputs.as_slice().to_vec())
                .unwrap_or_default(),
            Backend::WaylandSurface(_) | Backend::DevWindow(_) => Vec::new(),
        }
    }

    #[doc(hidden)]
    pub fn testing_surface_output_generation(&self, surface_id: &str) -> u64 {
        match &self.backend {
            Backend::Testing(backend) => backend
                .surface_output_generations
                .get(surface_id)
                .copied()
                .unwrap_or_default(),
            Backend::WaylandSurface(_) | Backend::DevWindow(_) => 0,
        }
    }

    #[doc(hidden)]
    pub fn testing_set_surface_missing(&mut self, surface_id: &str, missing: bool) {
        if let Backend::Testing(backend) = &mut self.backend {
            if missing {
                clear_testing_surface_runtime_state(backend, surface_id);
                backend.missing_surfaces.insert(surface_id.to_string());
            } else {
                backend.missing_surfaces.remove(surface_id);
            }
        }
    }

    #[doc(hidden)]
    pub fn testing_fail_next_configure(&mut self, message: impl Into<String>) {
        if let Backend::Testing(backend) = &mut self.backend {
            backend.configure_error = Some(message.into());
        }
    }

    #[doc(hidden)]
    pub fn testing_fail_next_popup_configure(&mut self, message: impl Into<String>) {
        if let Backend::Testing(backend) = &mut self.backend {
            backend.popup_configure_error = Some(message.into());
        }
    }

    #[doc(hidden)]
    pub fn testing_popup_config(&self, surface_id: &str) -> Option<&PopupConfig> {
        match &self.backend {
            Backend::Testing(backend) => backend.popup_configs.get(surface_id),
            _ => None,
        }
    }

    #[doc(hidden)]
    pub fn testing_destroyed_popups(&self) -> &[String] {
        match &self.backend {
            Backend::Testing(backend) => &backend.destroyed_popups,
            _ => &[],
        }
    }

    #[doc(hidden)]
    pub fn testing_destroyed_surfaces(&self) -> &[String] {
        match &self.backend {
            Backend::Testing(backend) => &backend.destroyed_surfaces,
            _ => &[],
        }
    }

    #[doc(hidden)]
    pub fn testing_presented_surfaces(&self) -> &[String] {
        match &self.backend {
            Backend::Testing(backend) => &backend.presented,
            _ => &[],
        }
    }

    #[doc(hidden)]
    pub fn testing_presented_damage(&self) -> &[(String, Vec<DamageRect>)] {
        match &self.backend {
            Backend::Testing(backend) => &backend.presented_damage,
            _ => &[],
        }
    }

    #[doc(hidden)]
    pub fn testing_surface_state_commits(&self) -> &[String] {
        match &self.backend {
            Backend::Testing(backend) => &backend.surface_state_commits,
            _ => &[],
        }
    }

    #[doc(hidden)]
    pub fn testing_push_close_request(&mut self, surface_id: impl Into<String>) {
        if let Backend::Testing(backend) = &mut self.backend {
            backend.close_requests.push(surface_id.into());
        }
    }

    #[doc(hidden)]
    pub fn testing_push_dismissed_popup(&mut self, surface_id: impl Into<String>) {
        if let Backend::Testing(backend) = &mut self.backend {
            let surface_id = surface_id.into();
            if backend.popup_configs.remove(&surface_id).is_some() {
                drop_testing_events_for_surface(backend, &surface_id);
                clear_testing_text_input_for_surface(backend, &surface_id);
                backend.pending_surface_states.remove(&surface_id);
                backend.missing_surfaces.insert(surface_id.clone());
                backend.destroyed_popup_ids.insert(surface_id.clone());
                backend
                    .lifecycle_events
                    .push(SurfaceLifecycleEvent::Dismissed { surface_id });
            }
        }
    }

    #[doc(hidden)]
    pub fn testing_push_surface_closed(&mut self, surface_id: impl Into<String>) {
        if let Backend::Testing(backend) = &mut self.backend {
            let surface_id = surface_id.into();
            if backend.surface_configs.remove(&surface_id).is_some() {
                let mut child_ids = backend
                    .popup_configs
                    .iter()
                    .filter_map(|(id, config)| {
                        (config.parent_surface_id == surface_id).then_some(id.clone())
                    })
                    .collect::<Vec<_>>();
                child_ids.sort();
                for child_id in child_ids {
                    drop_testing_events_for_surface(backend, &child_id);
                    clear_testing_text_input_for_surface(backend, &child_id);
                    backend.popup_configs.remove(&child_id);
                    backend.pending_surface_states.remove(&child_id);
                    backend.missing_surfaces.insert(child_id.clone());
                    backend.destroyed_popup_ids.insert(child_id.clone());
                    backend
                        .lifecycle_events
                        .push(SurfaceLifecycleEvent::Dismissed {
                            surface_id: child_id,
                        });
                }
                drop_testing_events_for_surface(backend, &surface_id);
                clear_testing_text_input_for_surface(backend, &surface_id);
                backend.pending_surface_states.remove(&surface_id);
                backend.missing_surfaces.insert(surface_id.clone());
                backend.destroyed_surface_ids.insert(surface_id.clone());
                backend
                    .lifecycle_events
                    .push(SurfaceLifecycleEvent::Closed { surface_id });
            }
        }
    }

    /// Simulate the Wayland connection disappearing. This follows the same
    /// one-shot lifecycle contract as the real backend: every live surface is
    /// removed before its `Lost` event is exposed, and later injections do not
    /// duplicate events.
    #[doc(hidden)]
    pub fn testing_push_connection_lost(&mut self, reason: impl Into<String>) {
        if let Backend::Testing(backend) = &mut self.backend {
            if backend.connection_lost.is_some() {
                return;
            }
            let reason = reason.into();
            backend.connection_lost = Some(reason.clone());
            backend.events.clear();
            backend.text_input_surface = None;
            backend.text_input_pending = TestingPendingTextInput::default();
            backend.text_input_state = None;
            backend.frame_pending_surfaces.clear();
            backend.buffer_backpressured_surfaces.clear();
            backend.surface_scales.clear();
            backend.full_redraw_surfaces.clear();
            let mut surface_ids = backend
                .surface_configs
                .keys()
                .chain(backend.popup_configs.keys())
                .cloned()
                .collect::<Vec<_>>();
            surface_ids.sort();
            surface_ids.dedup();
            for surface_id in surface_ids {
                backend.surface_configs.remove(&surface_id);
                backend.popup_configs.remove(&surface_id);
                backend.pending_surface_states.remove(&surface_id);
                backend.missing_surfaces.insert(surface_id.clone());
                backend.lifecycle_events.push(SurfaceLifecycleEvent::Lost {
                    surface_id,
                    reason: reason.clone(),
                });
            }
        }
    }

    #[doc(hidden)]
    pub fn testing_push_event(&mut self, event: WindowEvent) {
        if let Backend::Testing(backend) = &mut self.backend {
            backend.events.push(event);
        }
    }

    /// Simulate `zwp_text_input_v3.enter` for a live testing surface.
    ///
    /// The deterministic backend has one protocol seat. Enter replaces the
    /// previous focus and starts a fresh done-group, matching the real
    /// per-seat text-input object without exposing protocol handles to tests.
    #[doc(hidden)]
    pub fn testing_text_input_enter(&mut self, surface_id: impl Into<String>) -> bool {
        let Backend::Testing(backend) = &mut self.backend else {
            return false;
        };
        let surface_id = surface_id.into();
        if backend.connection_lost.is_some()
            || (!backend.surface_configs.contains_key(&surface_id)
                && !backend.popup_configs.contains_key(&surface_id))
        {
            return false;
        }
        backend.text_input_surface = Some(surface_id);
        backend.text_input_pending = TestingPendingTextInput::default();
        true
    }

    /// Simulate `zwp_text_input_v3.leave` and cancel the unfinished done-group.
    #[doc(hidden)]
    pub fn testing_text_input_leave(&mut self, surface_id: &str) -> bool {
        let Backend::Testing(backend) = &mut self.backend else {
            return false;
        };
        if backend.text_input_surface.as_deref() != Some(surface_id) {
            return false;
        }
        clear_testing_text_input_transaction_for_surface(backend, surface_id)
    }

    /// Buffer one `zwp_text_input_v3.preedit_string` event until `done`.
    #[doc(hidden)]
    pub fn testing_text_input_preedit(
        &mut self,
        surface_id: &str,
        text: Option<String>,
        cursor_begin: i32,
        cursor_end: i32,
    ) -> bool {
        let Backend::Testing(backend) = &mut self.backend else {
            return false;
        };
        if backend.text_input_surface.as_deref() != Some(surface_id) {
            return false;
        }
        backend.text_input_pending.preedit = Some(text);
        backend.text_input_pending.preedit_cursor_begin = cursor_begin;
        backend.text_input_pending.preedit_cursor_end = cursor_end;
        true
    }

    /// Buffer one `zwp_text_input_v3.commit_string` event until `done`.
    #[doc(hidden)]
    pub fn testing_text_input_commit(&mut self, surface_id: &str, text: impl Into<String>) -> bool {
        let Backend::Testing(backend) = &mut self.backend else {
            return false;
        };
        if backend.text_input_surface.as_deref() != Some(surface_id) {
            return false;
        }
        backend.text_input_pending.commit = Some(text.into());
        true
    }

    /// Buffer one `zwp_text_input_v3.delete_surrounding_text` event until
    /// `done`.
    #[doc(hidden)]
    pub fn testing_text_input_delete(
        &mut self,
        surface_id: &str,
        before_bytes: usize,
        after_bytes: usize,
    ) -> bool {
        let Backend::Testing(backend) = &mut self.backend else {
            return false;
        };
        if backend.text_input_surface.as_deref() != Some(surface_id) {
            return false;
        }
        backend.text_input_pending.before_bytes = before_bytes;
        backend.text_input_pending.after_bytes = after_bytes;
        true
    }

    /// Simulate `zwp_text_input_v3.done`, publishing at most one atomic edit.
    #[doc(hidden)]
    pub fn testing_text_input_done(&mut self) -> bool {
        let Backend::Testing(backend) = &mut self.backend else {
            return false;
        };
        let Some(surface_id) = backend.text_input_surface.clone() else {
            return false;
        };
        let Some(event) = backend.text_input_pending.take_edit(surface_id) else {
            return false;
        };
        backend.events.push(event);
        true
    }

    #[doc(hidden)]
    pub fn testing_text_input_state(&self) -> Option<(String, TextInputState)> {
        match &self.backend {
            Backend::Testing(backend) => backend.text_input_state.clone(),
            Backend::WaylandSurface(_) | Backend::DevWindow(_) => None,
        }
    }

    pub fn configure(
        &mut self,
        surface_id: &str,
        cfg: SurfaceConfig,
    ) -> Result<(), PresentationError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.configure(surface_id, cfg),
            Backend::DevWindow(_) => Ok(()),
            Backend::Testing(backend) => {
                if let Some(reason) = &backend.connection_lost {
                    return Err(PresentationError::ConnectionLost(reason.clone()));
                }
                if let Some(message) = backend.configure_error.take() {
                    return Err(PresentationError::SurfaceCreate(message));
                }
                if backend.missing_surfaces.remove(surface_id) {
                    clear_testing_surface_runtime_state(backend, surface_id);
                }
                backend.destroyed_surface_ids.remove(surface_id);
                backend
                    .surface_config_history
                    .push((surface_id.to_string(), cfg.clone()));
                backend.surface_configs.insert(surface_id.to_string(), cfg);
                Ok(())
            }
        }
    }

    /// Every surface config this engine has been given, newest per surface.
    /// Testing backend only; lets a test assert what geometry the shell asked
    /// the compositor for and what part of it was declared reserve.
    #[doc(hidden)]
    pub fn testing_surface_configs(&self) -> Vec<(String, SurfaceConfig)> {
        match &self.backend {
            Backend::Testing(backend) => backend
                .surface_configs
                .iter()
                .map(|(id, cfg)| (id.clone(), cfg.clone()))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Every surface config in call order. Testing backend only; unlike
    /// `testing_surface_configs`, this preserves superseded geometry so tests
    /// can catch transient compositor requests.
    #[doc(hidden)]
    pub fn testing_surface_config_history(&self) -> &[(String, SurfaceConfig)] {
        match &self.backend {
            Backend::Testing(backend) => &backend.surface_config_history,
            _ => &[],
        }
    }

    /// The size the compositor last configured for a `role: "window"` surface,
    /// when it named one. `None` means the client's CSS-measured size still
    /// governs — either the surface is not a window, or its compositor has not
    /// yet decided a size (the usual state of a freshly mapped toplevel).
    pub fn window_configured_size(&self, surface_id: &str) -> Option<(u32, u32)> {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.window_configured_size(surface_id),
            _ => None,
        }
    }

    /// The `xdg_toplevel` states the compositor last configured for a
    /// `role: "window"` surface. Everything false for a layer surface, a popup,
    /// or a window whose first configure has not arrived — a window that has
    /// not been told it is fullscreen is not fullscreen.
    pub fn window_states(&self, surface_id: &str) -> WindowStates {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.window_states(surface_id),
            Backend::Testing(backend) => backend
                .window_states
                .get(surface_id)
                .copied()
                .unwrap_or_default(),
            Backend::DevWindow(_) => WindowStates::default(),
        }
    }

    /// Drain the ids of window surfaces the user asked to close. xdg-shell's
    /// close is a request, not a destruction: the surface stays mapped until
    /// the shell acts on it.
    pub fn take_close_requests(&mut self) -> Vec<String> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.take_close_requests(),
            Backend::DevWindow(_) => Vec::new(),
            Backend::Testing(backend) => std::mem::take(&mut backend.close_requests),
        }
    }

    /// True when the active backend can realize a surface as an `xdg_toplevel`.
    /// Same requirement as [`Self::popup_supported`] — `xdg_wm_base` — but a
    /// distinct question, so that a caller refusing to promote a surface says
    /// which capability it was missing.
    ///
    /// Callers must check this *before* flipping a surface's role: creating the
    /// toplevel fails inside `configure`, by which point the old layer surface
    /// has already been destroyed and the surface would be left unmapped.
    pub fn window_role_supported(&self) -> bool {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.window_role_supported(),
            Backend::DevWindow(_) => false,
            // The testing backend records configures without creating compositor
            // objects, so role changes are always exercisable there.
            Backend::Testing(_) => true,
        }
    }

    /// True when the active backend can promote a `<popover>` into a compositor
    /// `xdg_popup` (Wayland backend with `xdg_wm_base`). The dev-window backend
    /// cannot, so callers should keep popover content inline there.
    pub fn popup_supported(&self) -> bool {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.popup_supported(),
            Backend::DevWindow(_) => false,
            Backend::Testing(backend) => backend.popup_supported,
        }
    }

    /// Promote `surface_id` into an `xdg_popup` child of `config.parent_surface_id`,
    /// or reposition it if it already exists. No-op on the dev-window backend.
    pub fn configure_popup(
        &mut self,
        surface_id: &str,
        config: PopupConfig,
    ) -> Result<(), PresentationError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.configure_popup(surface_id, config),
            Backend::DevWindow(_) => Ok(()),
            Backend::Testing(backend) => {
                if let Some(reason) = &backend.connection_lost {
                    return Err(PresentationError::ConnectionLost(reason.clone()));
                }
                if let Some(message) = backend.popup_configure_error.take() {
                    return Err(PresentationError::SurfaceCreate(message));
                }
                let existing_popup_parent = backend
                    .popup_configs
                    .get(surface_id)
                    .map(|existing| existing.parent_surface_id.clone());
                if backend.surface_configs.contains_key(surface_id) {
                    return Err(PresentationError::SurfaceCreate(format!(
                        "surface '{surface_id}' is already a non-popup surface"
                    )));
                }
                if let Some(existing_parent) = existing_popup_parent.as_deref() {
                    if existing_parent != config.parent_surface_id {
                        return Err(PresentationError::SurfaceCreate(format!(
                            "popup '{surface_id}' cannot be reparented from '{existing_parent}' to '{}'",
                            config.parent_surface_id
                        )));
                    }
                } else if !backend.popup_supported {
                    return Err(PresentationError::ProtocolUnsupported(
                        "xdg_wm_base unavailable; cannot promote popover".into(),
                    ));
                }
                if backend
                    .popup_configs
                    .contains_key(&config.parent_surface_id)
                {
                    return Err(PresentationError::SurfaceCreate(
                        "popup parent cannot itself be a popup".into(),
                    ));
                }
                if !backend
                    .surface_configs
                    .contains_key(&config.parent_surface_id)
                {
                    return Err(PresentationError::SurfaceCreate(format!(
                        "popup parent surface '{}' not found",
                        config.parent_surface_id
                    )));
                }
                if backend.missing_surfaces.remove(surface_id) {
                    clear_testing_surface_runtime_state(backend, surface_id);
                }
                backend.destroyed_popup_ids.remove(surface_id);
                backend.popup_configs.insert(surface_id.to_string(), config);
                Ok(())
            }
        }
    }

    /// Destroy a previously promoted popup surface. No-op on the dev-window backend.
    pub fn destroy_popup(&mut self, surface_id: &str) {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.destroy_popup(surface_id),
            Backend::DevWindow(_) => {}
            Backend::Testing(backend) => {
                drop_testing_events_for_surface(backend, surface_id);
                clear_testing_text_input_for_surface(backend, surface_id);
                backend.popup_configs.remove(surface_id);
                backend.pending_surface_states.remove(surface_id);
                if backend.destroyed_popup_ids.insert(surface_id.to_string()) {
                    backend.destroyed_popups.push(surface_id.to_string());
                }
            }
        }
    }

    /// Destroy a top-level surface and every popup parented to it.
    pub fn destroy_surface(&mut self, surface_id: &str) {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.destroy_surface(surface_id),
            Backend::DevWindow(bridge) => bridge.destroy_surface(surface_id),
            Backend::Testing(backend) => {
                backend.missing_surfaces.remove(surface_id);
                drop_testing_events_for_surface(backend, surface_id);
                clear_testing_text_input_for_surface(backend, surface_id);
                let child_ids = backend
                    .popup_configs
                    .iter()
                    .filter_map(|(id, config)| {
                        (config.parent_surface_id == surface_id).then_some(id.clone())
                    })
                    .collect::<Vec<_>>();
                for child_id in child_ids {
                    drop_testing_events_for_surface(backend, &child_id);
                    clear_testing_text_input_for_surface(backend, &child_id);
                    backend.popup_configs.remove(&child_id);
                    backend.pending_surface_states.remove(&child_id);
                    if backend.destroyed_popup_ids.insert(child_id.clone()) {
                        backend.destroyed_popups.push(child_id);
                    }
                }
                backend.surface_configs.remove(surface_id);
                clear_testing_text_input_for_surface(backend, surface_id);
                backend.pending_surface_states.remove(surface_id);
                if backend.destroyed_surface_ids.insert(surface_id.to_string()) {
                    backend.destroyed_surfaces.push(surface_id.to_string());
                }
            }
        }
    }

    /// Destroy every popup parented to `parent_surface_id` (e.g. when the host
    /// surface is hidden). No-op on the dev-window backend.
    pub fn destroy_popups_for_parent(&mut self, parent_surface_id: &str) {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.destroy_popups_for_parent(parent_surface_id),
            Backend::DevWindow(_) => {}
            Backend::Testing(backend) => {
                let ids = backend
                    .popup_configs
                    .iter()
                    .filter_map(|(id, config)| {
                        (config.parent_surface_id == parent_surface_id).then_some(id.clone())
                    })
                    .collect::<Vec<_>>();
                for id in ids {
                    drop_testing_events_for_surface(backend, &id);
                    clear_testing_text_input_for_surface(backend, &id);
                    backend.popup_configs.remove(&id);
                    if backend.destroyed_popup_ids.insert(id.clone()) {
                        backend.destroyed_popups.push(id);
                    }
                }
            }
        }
    }

    /// Drain compositor-owned surface lifecycle transitions.
    pub fn take_surface_lifecycle_events(&mut self) -> Vec<SurfaceLifecycleEvent> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.take_surface_lifecycle_events(),
            Backend::DevWindow(_) => Vec::new(),
            Backend::Testing(backend) => std::mem::take(&mut backend.lifecycle_events),
        }
    }

    /// Drain the ids of popups the compositor dismissed since the last call so
    /// the shell can drop the matching popup targets. Always empty on dev-window.
    pub fn take_dismissed_popups(&mut self) -> Vec<String> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.take_dismissed_popups(),
            Backend::DevWindow(_) => Vec::new(),
            Backend::Testing(backend) => {
                let events = std::mem::take(&mut backend.lifecycle_events);
                let mut dismissed = Vec::new();
                for event in events {
                    match event {
                        SurfaceLifecycleEvent::Dismissed { surface_id } => {
                            dismissed.push(surface_id)
                        }
                        other => backend.lifecycle_events.push(other),
                    }
                }
                dismissed
            }
        }
    }

    pub fn present(
        &mut self,
        surface_id: &str,
        title: &str,
        visible: bool,
        buffer: &PixelBuffer,
    ) -> Result<PresentStatus, PresentationError> {
        // `present()` is only used by DevWindow callers. Pass a full-damage
        // slice so the Wayland path would get a complete upload if ever
        // reached, but in practice this only hits Backend::DevWindow.
        let full = DamageRect {
            x: 0,
            y: 0,
            width: buffer.width().max(1),
            height: buffer.height().max(1),
        };
        self.present_with_damage(surface_id, title, visible, buffer, &[full])
    }

    pub fn present_with_damage(
        &mut self,
        surface_id: &str,
        title: &str,
        visible: bool,
        buffer: &PixelBuffer,
        damage: &[DamageRect],
    ) -> Result<PresentStatus, PresentationError> {
        let _span =
            tracing::debug_span!("present_with_damage", surface_id, rects = damage.len()).entered();
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => {
                bridge.present_with_damage(surface_id, title, visible, buffer, damage)
            }
            Backend::DevWindow(bridge) => bridge.present(surface_id, title, visible, buffer),
            Backend::Testing(backend) => {
                if let Some(reason) = &backend.connection_lost {
                    return Err(PresentationError::ConnectionLost(reason.clone()));
                }
                if visible && backend.missing_surfaces.contains(surface_id) {
                    return Ok(PresentStatus::SurfaceMissing);
                }
                if visible && backend.unconfigured_surfaces.contains(surface_id) {
                    return Ok(PresentStatus::NotReady);
                }
                if visible
                    && (backend.frame_pending_surfaces.contains(surface_id)
                        || backend.buffer_backpressured_surfaces.contains(surface_id))
                {
                    return Ok(PresentStatus::NotReady);
                }
                if visible {
                    backend.presented.push(surface_id.to_string());
                    backend
                        .presented_damage
                        .push((surface_id.to_string(), damage.to_vec()));
                }
                Ok(PresentStatus::Presented)
            }
        }
    }

    /// Commit pending compositor surface state without attaching a new pixel
    /// buffer. This keeps region-only changes observable when a render pass
    /// produced no pixel damage.
    pub fn commit_surface_state(
        &mut self,
        surface_id: &str,
    ) -> Result<SurfaceStateStatus, PresentationError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.commit_surface_state(surface_id),
            Backend::DevWindow(_) => Ok(SurfaceStateStatus::Unchanged),
            Backend::Testing(backend) => {
                if let Some(reason) = &backend.connection_lost {
                    return Err(PresentationError::ConnectionLost(reason.clone()));
                }
                if backend.missing_surfaces.contains(surface_id) {
                    return Ok(SurfaceStateStatus::SurfaceMissing);
                }
                if backend.unconfigured_surfaces.contains(surface_id) {
                    return Ok(SurfaceStateStatus::NotReady);
                }
                if backend.pending_surface_states.remove(surface_id) {
                    backend.surface_state_commits.push(surface_id.to_string());
                    Ok(SurfaceStateStatus::Committed)
                } else {
                    Ok(SurfaceStateStatus::Unchanged)
                }
            }
        }
    }

    /// Finish one shell frame after all surface presents have staged their
    /// protocol requests. The Wayland backend flushes and progresses its event
    /// queue once here instead of doing connection work for every surface.
    pub fn finish_frame(&mut self) -> Result<(), PresentationError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.finish_frame(),
            Backend::DevWindow(bridge) => {
                bridge.pump();
                Ok(())
            }
            Backend::Testing(backend) => {
                if let Some(reason) = &backend.connection_lost {
                    return Err(PresentationError::ConnectionLost(reason.clone()));
                }
                backend.completed_frames += 1;
                Ok(())
            }
        }
    }

    #[doc(hidden)]
    pub fn testing_completed_frames(&self) -> usize {
        match &self.backend {
            Backend::Testing(backend) => backend.completed_frames,
            _ => 0,
        }
    }

    /// Whether a surface can accept a buffer without waiting for a compositor
    /// configure. Missing surfaces return true so the shell can run the first
    /// render pass that creates/configures them.
    pub fn surface_ready_to_present(&self, surface_id: &str) -> bool {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.surface_ready_to_present(surface_id),
            Backend::DevWindow(_) => true,
            Backend::Testing(backend) => !backend.unconfigured_surfaces.contains(surface_id),
        }
    }

    pub fn update_opaque_region(&mut self, surface_id: &str, opaque_rect: Option<DamageRect>) {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.update_opaque_region(surface_id, opaque_rect),
            Backend::Testing(backend) => {
                if backend.surface_configs.contains_key(surface_id)
                    || backend.popup_configs.contains_key(surface_id)
                {
                    backend
                        .pending_surface_states
                        .insert(surface_id.to_string());
                }
            }
            Backend::DevWindow(_) => {}
        }
    }

    /// The pointer/touch input region currently in force for a surface, in
    /// surface-local logical coordinates. `None` means the whole surface takes
    /// input (the `wl_surface` default).
    ///
    /// There is deliberately no setter. A surface's input region is *derived*
    /// from the reserve it declared in its [`SurfaceConfig`]/[`PopupConfig`]
    /// (see [`SurfacePadding`]) so that inflating a surface and confining its
    /// input cannot come apart — they are the same decision, expressed once.
    /// This accessor exists so tests can assert the result.
    pub fn input_region(&self, surface_id: &str) -> Option<DamageRect> {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.input_region(surface_id),
            Backend::DevWindow(_) => None,
            Backend::Testing(backend) => backend
                .surface_configs
                .get(surface_id)
                .and_then(|cfg| {
                    cfg.padding
                        .content_rect(cfg.width.max(1), cfg.height.max(1))
                })
                .or_else(|| {
                    backend.popup_configs.get(surface_id).and_then(|cfg| {
                        cfg.padding
                            .content_rect(cfg.placement.size.0.max(1), cfg.placement.size.1.max(1))
                    })
                }),
        }
    }

    /// Set the logical-coordinate blur regions for a surface.
    /// Only meaningful on Wayland backends with `org_kde_kwin_blur` support.
    /// Pass an empty vector to clear any previously committed blur region from the
    /// compositor. No protocol calls are emitted if no blur region has ever
    /// been set for this surface.
    pub fn update_blur_regions(&mut self, surface_id: &str, blur_regions: Vec<DamageRect>) {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.update_blur_regions(surface_id, blur_regions),
            Backend::Testing(backend) => {
                if backend.surface_configs.contains_key(surface_id)
                    || backend.popup_configs.contains_key(surface_id)
                {
                    backend
                        .pending_surface_states
                        .insert(surface_id.to_string());
                }
            }
            Backend::DevWindow(_) => {}
        }
    }

    pub fn surface_size(
        &mut self,
        surface_id: &str,
    ) -> Result<Option<(u32, u32)>, PresentationError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.surface_size(surface_id),
            Backend::DevWindow(_) => Ok(None),
            Backend::Testing(_) => Ok(None),
        }
    }

    pub fn surface_size_if_known(&self, surface_id: &str) -> Option<(u32, u32)> {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.surface_size_if_known(surface_id),
            Backend::DevWindow(_) => None,
            Backend::Testing(_) => None,
        }
    }

    pub fn surface_waiting_for_frame_callback(&self, surface_id: &str) -> bool {
        match &self.backend {
            Backend::WaylandSurface(bridge) => {
                bridge.surface_waiting_for_frame_callback(surface_id)
            }
            Backend::DevWindow(_) => false,
            Backend::Testing(backend) => backend.frame_pending_surfaces.contains(surface_id),
        }
    }

    pub fn surface_waiting_for_buffer_release(&self, surface_id: &str) -> bool {
        match &self.backend {
            Backend::WaylandSurface(bridge) => {
                bridge.surface_waiting_for_buffer_release(surface_id)
            }
            Backend::DevWindow(_) => false,
            Backend::Testing(backend) => backend.buffer_backpressured_surfaces.contains(surface_id),
        }
    }

    /// Return the compositor-object/configure/frame generations for a live
    /// Wayland surface. The value is intended for diagnostics and lifecycle
    /// correlation; non-Wayland backends do not have compositor generations.
    pub fn surface_generation(&self, surface_id: &str) -> Option<SurfaceGeneration> {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.surface_generation(surface_id),
            Backend::DevWindow(_) | Backend::Testing(_) => None,
        }
    }

    /// Return the protocol versions negotiated for the live presentation
    /// connection. Non-Wayland backends report an empty snapshot.
    pub fn negotiated_capabilities(&self) -> NegotiatedCapabilities {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.negotiated_capabilities(),
            Backend::DevWindow(_) | Backend::Testing(_) => NegotiatedCapabilities::default(),
        }
    }

    pub fn surface_scale(&self, surface_id: &str) -> f32 {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.surface_scale(surface_id),
            Backend::DevWindow(_) => 1.0,
            Backend::Testing(backend) => backend
                .surface_scales
                .get(surface_id)
                .copied()
                .unwrap_or(1.0),
        }
    }

    pub fn surface_needs_full_redraw(&self, surface_id: &str) -> bool {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.surface_needs_full_redraw(surface_id),
            Backend::DevWindow(_) => false,
            Backend::Testing(backend) => backend.full_redraw_surfaces.contains(surface_id),
        }
    }

    pub fn clear_surface_needs_full_redraw(&mut self, surface_id: &str) {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.clear_surface_needs_full_redraw(surface_id),
            Backend::DevWindow(_) => {}
            Backend::Testing(backend) => {
                backend.full_redraw_surfaces.remove(surface_id);
            }
        }
    }

    pub fn pump(&mut self) -> Result<(), PresentationError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.pump(),
            Backend::DevWindow(bridge) => {
                bridge.pump();
                Ok(())
            }
            Backend::Testing(backend) => {
                backend.connection_lost.as_ref().map_or(Ok(()), |reason| {
                    Err(PresentationError::ConnectionLost(reason.clone()))
                })
            }
        }
    }

    pub fn poll_events(&mut self) -> Result<Vec<WindowEvent>, PresentationError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.poll_events(),
            Backend::DevWindow(bridge) => Ok(bridge.poll_events()),
            Backend::Testing(backend) => backend.connection_lost.as_ref().map_or_else(
                || Ok(std::mem::take(&mut backend.events)),
                |reason| Err(PresentationError::ConnectionLost(reason.clone())),
            ),
        }
    }

    pub fn set_pointer_interactive(&mut self, interactive: bool) {
        if let Backend::WaylandSurface(bridge) = &mut self.backend {
            bridge.set_pointer_interactive(interactive);
        }
    }

    /// Publish the focused component's text-input-v3 surrounding-text
    /// snapshot. Non-Wayland backends accept the state so shell behavior stays
    /// backend-neutral, while only the Wayland backend sends protocol requests.
    pub fn set_text_input_state(
        &mut self,
        surface_id: Option<&str>,
        state: Option<TextInputState>,
    ) -> Result<(), PresentationError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.set_text_input_state(surface_id, state),
            Backend::DevWindow(_) => {
                validate_text_input_state(surface_id, state.as_ref())?;
                Ok(())
            }
            Backend::Testing(backend) => {
                if let Some(reason) = &backend.connection_lost {
                    return Err(PresentationError::ConnectionLost(reason.clone()));
                }
                validate_text_input_state(surface_id, state.as_ref())?;
                backend.text_input_state = match (surface_id.filter(|id| !id.is_empty()), state) {
                    (Some(surface_id), Some(state)) => Some((surface_id.to_string(), state)),
                    (None, None) => None,
                    _ => unreachable!("text-input state was validated before storage"),
                };
                Ok(())
            }
        }
    }

    /// Returns true when the backend supports fd-based blocking dispatch (WaylandSurface).
    /// Returns false for DevWindow, which uses internal polling.
    pub fn supports_blocking_dispatch(&self) -> bool {
        matches!(&self.backend, Backend::WaylandSurface(_))
    }

    /// Returns true for backends that must be periodically pumped to surface
    /// input events. The dev-window/minifb backend has no fd-based blocking
    /// primitive, but only needs this while it has open windows.
    pub fn needs_polling_dispatch(&self) -> bool {
        match &self.backend {
            Backend::WaylandSurface(_) => false,
            Backend::DevWindow(bridge) => bridge.needs_polling_dispatch(),
            Backend::Testing(_) => false,
        }
    }

    /// Block on the backend until `timeout` elapses or a wakeup occurs.
    ///
    /// `eventfd_fd` is an optional IPC/backend wakeup fd checked *after*
    /// the Wayland connection fd (non-blocking check). For `Backend::DevWindow`
    /// this returns `DeadlineExpired` immediately.
    pub fn wait_for_events(
        &mut self,
        timeout: std::time::Duration,
        eventfd_fd: BorrowedFd<'_>,
    ) -> Result<WaitResult, PresentationError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.wait_for_events(timeout, eventfd_fd),
            Backend::DevWindow(_) => Ok(WaitResult::deadline_expired()),
            Backend::Testing(_) => Ok(WaitResult::deadline_expired()),
        }
    }
}

impl Default for PresentationEngine {
    fn default() -> Self {
        Self::select()
    }
}

pub fn coalesce_input_events(events: Vec<WindowEvent>) -> Vec<WindowEvent> {
    if events.len() < 2 {
        return events;
    }

    let mut output = Vec::with_capacity(events.len());
    let mut pending = Vec::new();

    for event in events {
        match event {
            WindowEvent::PointerMove { surface_id, x, y } => {
                flush_pending_scroll_for_surface(&surface_id, &mut pending, &mut output);
                push_or_replace_pending(
                    &mut pending,
                    PendingInputEvent::PointerMove { surface_id, x, y },
                );
            }
            WindowEvent::Scroll {
                surface_id,
                x,
                y,
                dx,
                dy,
            } => {
                flush_pending_pointer_move_for_surface(&surface_id, &mut pending, &mut output);
                flush_pending_two_finger_scroll_for_surface(&surface_id, &mut pending, &mut output);
                push_or_replace_pending(
                    &mut pending,
                    PendingInputEvent::Scroll {
                        surface_id,
                        x,
                        y,
                        dx,
                        dy,
                    },
                );
            }
            WindowEvent::TwoFingerScroll {
                surface_id,
                x,
                y,
                dx,
                dy,
            } => {
                flush_pending_pointer_move_for_surface(&surface_id, &mut pending, &mut output);
                flush_pending_wheel_scroll_for_surface(&surface_id, &mut pending, &mut output);
                push_or_replace_pending(
                    &mut pending,
                    PendingInputEvent::TwoFingerScroll {
                        surface_id,
                        x,
                        y,
                        dx,
                        dy,
                    },
                );
            }
            WindowEvent::PointerLeave { surface_id } => {
                remove_pending_for_surface(&surface_id, &mut pending);
                output.push(WindowEvent::PointerLeave { surface_id });
            }
            event => {
                let surface_id = event_surface_id(&event);
                flush_pending_for_surface(surface_id, &mut pending, &mut output);
                output.push(event);
            }
        }
    }

    output.extend(
        pending
            .into_iter()
            .map(PendingInputEvent::into_window_event),
    );
    output
}

pub fn coalesce_pointer_moves(events: Vec<WindowEvent>) -> Vec<WindowEvent> {
    coalesce_input_events(events)
}

#[derive(Debug)]
enum PendingInputEvent {
    PointerMove {
        surface_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
    },
    Scroll {
        surface_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    TwoFingerScroll {
        surface_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
}

impl PendingInputEvent {
    fn surface_id(&self) -> &str {
        match self {
            Self::PointerMove { surface_id, .. }
            | Self::Scroll { surface_id, .. }
            | Self::TwoFingerScroll { surface_id, .. } => surface_id,
        }
    }

    fn into_window_event(self) -> WindowEvent {
        match self {
            Self::PointerMove { surface_id, x, y } => WindowEvent::PointerMove { surface_id, x, y },
            Self::Scroll {
                surface_id,
                x,
                y,
                dx,
                dy,
            } => WindowEvent::Scroll {
                surface_id,
                x,
                y,
                dx,
                dy,
            },
            Self::TwoFingerScroll {
                surface_id,
                x,
                y,
                dx,
                dy,
            } => WindowEvent::TwoFingerScroll {
                surface_id,
                x,
                y,
                dx,
                dy,
            },
        }
    }

    fn same_kind_and_surface(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (
                Self::PointerMove { surface_id: a, .. },
                Self::PointerMove { surface_id: b, .. }
            ) if a == b
        ) || matches!(
            (self, other),
            (Self::Scroll { surface_id: a, .. }, Self::Scroll { surface_id: b, .. }) if a == b
        ) || matches!(
            (self, other),
            (
                Self::TwoFingerScroll { surface_id: a, .. },
                Self::TwoFingerScroll { surface_id: b, .. }
            ) if a == b
        )
    }

    fn merge(&mut self, next: Self) {
        match (self, next) {
            (
                Self::PointerMove { x, y, .. },
                Self::PointerMove {
                    x: next_x,
                    y: next_y,
                    ..
                },
            ) => {
                *x = next_x;
                *y = next_y;
            }
            (
                Self::Scroll { x, y, dx, dy, .. },
                Self::Scroll {
                    x: next_x,
                    y: next_y,
                    dx: next_dx,
                    dy: next_dy,
                    ..
                },
            ) => {
                *x = next_x;
                *y = next_y;
                *dx += next_dx;
                *dy += next_dy;
            }
            (
                Self::TwoFingerScroll { x, y, dx, dy, .. },
                Self::TwoFingerScroll {
                    x: next_x,
                    y: next_y,
                    dx: next_dx,
                    dy: next_dy,
                    ..
                },
            ) => {
                *x = next_x;
                *y = next_y;
                *dx += next_dx;
                *dy += next_dy;
            }
            _ => {}
        }
    }
}

fn push_or_replace_pending(pending: &mut Vec<PendingInputEvent>, event: PendingInputEvent) {
    if let Some(existing) = pending
        .iter_mut()
        .find(|existing| existing.same_kind_and_surface(&event))
    {
        existing.merge(event);
    } else {
        pending.push(event);
    }
}

fn flush_pending_for_surface(
    surface_id: &str,
    pending: &mut Vec<PendingInputEvent>,
    output: &mut Vec<WindowEvent>,
) {
    drain_pending_where(pending, output, |event| event.surface_id() == surface_id);
}

fn flush_pending_pointer_move_for_surface(
    surface_id: &str,
    pending: &mut Vec<PendingInputEvent>,
    output: &mut Vec<WindowEvent>,
) {
    drain_pending_where(pending, output, |event| {
        matches!(event, PendingInputEvent::PointerMove { .. }) && event.surface_id() == surface_id
    });
}

fn flush_pending_scroll_for_surface(
    surface_id: &str,
    pending: &mut Vec<PendingInputEvent>,
    output: &mut Vec<WindowEvent>,
) {
    drain_pending_where(pending, output, |event| {
        matches!(
            event,
            PendingInputEvent::Scroll { .. } | PendingInputEvent::TwoFingerScroll { .. }
        ) && event.surface_id() == surface_id
    });
}

fn flush_pending_wheel_scroll_for_surface(
    surface_id: &str,
    pending: &mut Vec<PendingInputEvent>,
    output: &mut Vec<WindowEvent>,
) {
    drain_pending_where(pending, output, |event| {
        matches!(event, PendingInputEvent::Scroll { .. }) && event.surface_id() == surface_id
    });
}

fn flush_pending_two_finger_scroll_for_surface(
    surface_id: &str,
    pending: &mut Vec<PendingInputEvent>,
    output: &mut Vec<WindowEvent>,
) {
    drain_pending_where(pending, output, |event| {
        matches!(event, PendingInputEvent::TwoFingerScroll { .. })
            && event.surface_id() == surface_id
    });
}

fn remove_pending_for_surface(surface_id: &str, pending: &mut Vec<PendingInputEvent>) {
    pending.retain(|event| event.surface_id() != surface_id);
}

fn drain_pending_where(
    pending: &mut Vec<PendingInputEvent>,
    output: &mut Vec<WindowEvent>,
    mut should_drain: impl FnMut(&PendingInputEvent) -> bool,
) {
    let mut index = 0;
    while index < pending.len() {
        if should_drain(&pending[index]) {
            output.push(pending.remove(index).into_window_event());
        } else {
            index += 1;
        }
    }
}

pub fn event_surface_id(event: &WindowEvent) -> &str {
    match event {
        WindowEvent::PointerMove { surface_id, .. }
        | WindowEvent::PointerLeave { surface_id }
        | WindowEvent::PointerButton { surface_id, .. }
        | WindowEvent::PointerButtonWithIdentity { surface_id, .. }
        | WindowEvent::Scroll { surface_id, .. }
        | WindowEvent::TwoFingerScroll { surface_id, .. }
        | WindowEvent::Key { surface_id, .. }
        | WindowEvent::Char { surface_id, .. }
        | WindowEvent::TextInput { surface_id, .. }
        | WindowEvent::TextDelete { surface_id, .. }
        | WindowEvent::TextInputEdit { surface_id, .. }
        | WindowEvent::GestureSwipeBegin { surface_id, .. }
        | WindowEvent::GestureSwipeUpdate { surface_id, .. }
        | WindowEvent::GestureSwipeEnd { surface_id, .. }
        | WindowEvent::GesturePinchBegin { surface_id, .. }
        | WindowEvent::GesturePinchUpdate { surface_id, .. }
        | WindowEvent::GesturePinchEnd { surface_id, .. }
        | WindowEvent::GestureHoldBegin { surface_id, .. }
        | WindowEvent::GestureHoldEnd { surface_id, .. }
        | WindowEvent::TouchDown { surface_id, .. }
        | WindowEvent::TouchMove { surface_id, .. }
        | WindowEvent::TouchUp { surface_id, .. }
        | WindowEvent::TouchCancel { surface_id } => surface_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testing_backend_publishes_and_clears_text_input_state() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        let state = TextInputState::new("A🙂B", 5, 1);

        engine
            .set_text_input_state(Some("launcher"), Some(state.clone()))
            .expect("valid surrounding state should be accepted");
        assert_eq!(
            engine.testing_text_input_state(),
            Some(("launcher".to_string(), state))
        );

        engine
            .set_text_input_state(None, None)
            .expect("clearing text-input state should be accepted");
        assert!(engine.testing_text_input_state().is_none());
    }

    #[test]
    fn testing_text_input_simulator_preserves_done_boundary_and_ownership() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        engine
            .configure("launcher", SurfaceConfig::default())
            .expect("surface config should be accepted");
        let published_state = TextInputState::new("launcher", 0, 0);
        engine
            .set_text_input_state(Some("launcher"), Some(published_state.clone()))
            .expect("surrounding state should be accepted");

        assert!(!engine.testing_text_input_enter("missing"));
        assert!(engine.testing_text_input_enter("launcher"));
        assert!(engine.testing_text_input_preedit("launcher", Some("候".to_string()), 0, 3,));
        assert!(engine.testing_text_input_delete("launcher", 3, 1));
        assert!(engine.testing_text_input_commit("launcher", "候"));
        assert!(
            engine
                .poll_events()
                .expect("testing backend remains connected")
                .is_empty()
        );

        assert!(engine.testing_text_input_done());
        let events = engine
            .poll_events()
            .expect("testing backend remains connected");
        assert!(matches!(
            events.as_slice(),
            [WindowEvent::TextInputEdit {
                surface_id,
                preedit_present: true,
                preedit: Some(preedit),
                preedit_cursor_begin: 0,
                preedit_cursor_end: 3,
                commit: Some(commit),
                before_bytes: 3,
                after_bytes: 1,
            }] if surface_id.as_ref() == "launcher"
                && preedit.as_ref() == "候"
                && commit.as_ref() == "候"
        ));
        assert!(!engine.testing_text_input_done());
        assert!(!engine.testing_text_input_commit("other", "stale"));

        assert!(engine.testing_text_input_preedit("launcher", Some("stale".to_string()), 0, 6,));
        assert!(engine.testing_text_input_leave("launcher"));
        assert!(!engine.testing_text_input_done());
        assert_eq!(
            engine.testing_text_input_state(),
            Some(("launcher".to_string(), published_state))
        );
        assert!(
            engine
                .poll_events()
                .expect("testing backend remains connected")
                .is_empty()
        );

        assert!(engine.testing_text_input_enter("launcher"));
        assert!(
            engine.testing_text_input_preedit("launcher", Some("cancelled".to_string()), 0, 9,)
        );
        engine.testing_push_surface_closed("launcher");
        assert!(!engine.testing_text_input_done());
        assert!(
            engine
                .poll_events()
                .expect("testing backend remains connected")
                .is_empty()
        );
    }

    #[test]
    fn text_input_state_rejects_non_boundary_cursor_without_storage() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);

        let error = engine
            .set_text_input_state(Some("launcher"), Some(TextInputState::new("A🙂B", 2, 2)))
            .expect_err("cursor inside a UTF-8 scalar must be rejected");
        assert!(matches!(
            error,
            PresentationError::TextInputInvalid(message)
                if message.contains("cursor") && message.contains("UTF-8 boundary")
        ));
        assert!(engine.testing_text_input_state().is_none());
    }

    #[test]
    fn testing_popup_validation_matches_live_identity_contract() {
        let mut unsupported = PresentationEngine::testing_with_popup_support(false);
        unsupported
            .configure("panel", SurfaceConfig::default())
            .expect("parent surface should be accepted");
        let error = unsupported
            .configure_popup(
                "popup",
                PopupConfig {
                    parent_surface_id: "panel".to_string(),
                    placement: PopupPlacement::default(),
                    padding: SurfacePadding::default(),
                    grab: false,
                    grab_identity: None,
                },
            )
            .expect_err("unsupported popup promotion should be rejected");
        assert!(matches!(
            error,
            PresentationError::ProtocolUnsupported(message)
                if message.contains("xdg_wm_base")
        ));
        assert!(unsupported.testing_popup_config("popup").is_none());

        let mut engine = PresentationEngine::testing_with_popup_support(true);
        engine
            .configure("panel", SurfaceConfig::default())
            .expect("parent surface should be accepted");
        let popup_config = |parent_surface_id: &str| PopupConfig {
            parent_surface_id: parent_surface_id.to_string(),
            placement: PopupPlacement::default(),
            padding: SurfacePadding::default(),
            grab: false,
            grab_identity: None,
        };

        let error = engine
            .configure_popup("panel", popup_config("panel"))
            .expect_err("popup id must not replace a regular surface");
        assert!(matches!(
            error,
            PresentationError::SurfaceCreate(message)
                if message.contains("already a non-popup surface")
        ));

        let error = engine
            .configure_popup("missing-popup", popup_config("missing"))
            .expect_err("popup parent identity must be validated");
        assert!(matches!(
            error,
            PresentationError::SurfaceCreate(message) if message.contains("not found")
        ));

        engine
            .configure_popup("popup", popup_config("panel"))
            .expect("valid popup config should be accepted");
        let original = engine
            .testing_popup_config("popup")
            .cloned()
            .expect("accepted popup config should be retained");
        let mut reparented = original.clone();
        reparented.parent_surface_id = "other".to_string();
        let error = engine
            .configure_popup("popup", reparented)
            .expect_err("an existing popup must not be reparented");
        assert!(matches!(
            error,
            PresentationError::SurfaceCreate(message)
                if message.contains("cannot be reparented")
        ));
        assert_eq!(engine.testing_popup_config("popup"), Some(&original));

        let error = engine
            .configure_popup("nested", popup_config("popup"))
            .expect_err("nested popup parents are not supported");
        assert!(matches!(
            error,
            PresentationError::SurfaceCreate(message)
                if message.contains("parent cannot itself be a popup")
        ));
    }

    #[test]
    fn testing_parent_close_dismisses_popup_descendants_before_close() {
        let mut engine = PresentationEngine::testing_with_popup_support(true);
        engine
            .configure("panel", SurfaceConfig::default())
            .expect("parent surface should be accepted");
        for popup_id in ["popup-b", "popup-a"] {
            engine
                .configure_popup(
                    popup_id,
                    PopupConfig {
                        parent_surface_id: "panel".to_string(),
                        placement: PopupPlacement::default(),
                        padding: SurfacePadding::default(),
                        grab: false,
                        grab_identity: None,
                    },
                )
                .expect("popup config should be accepted");
        }
        engine
            .set_text_input_state(Some("popup-a"), Some(TextInputState::new("stale", 5, 5)))
            .expect("popup text-input state should be accepted");
        engine.testing_push_event(WindowEvent::TextInput {
            surface_id: "popup-a".into(),
            text: "stale".into(),
        });

        engine.testing_push_surface_closed("panel");
        engine.testing_push_surface_closed("panel");

        assert!(engine.testing_surface_configs().is_empty());
        assert!(engine.testing_popup_config("popup-a").is_none());
        assert!(engine.testing_popup_config("popup-b").is_none());
        assert!(engine.testing_text_input_state().is_none());
        assert!(
            engine
                .poll_events()
                .expect("testing backend remains connected")
                .is_empty()
        );
        assert_eq!(
            engine.take_surface_lifecycle_events(),
            [
                SurfaceLifecycleEvent::Dismissed {
                    surface_id: "popup-a".to_string(),
                },
                SurfaceLifecycleEvent::Dismissed {
                    surface_id: "popup-b".to_string(),
                },
                SurfaceLifecycleEvent::Closed {
                    surface_id: "panel".to_string(),
                },
            ]
        );
        assert!(engine.take_surface_lifecycle_events().is_empty());
    }

    #[test]
    fn testing_surface_teardown_drops_queued_input_for_destroyed_surface() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        engine
            .configure("panel", SurfaceConfig::default())
            .expect("surface config should be accepted");
        engine
            .configure("other", SurfaceConfig::default())
            .expect("second surface config should be accepted");
        engine.testing_push_event(WindowEvent::TextInputEdit {
            surface_id: "panel".into(),
            preedit_present: true,
            preedit: Some("候".into()),
            preedit_cursor_begin: 0,
            preedit_cursor_end: 3,
            commit: None,
            before_bytes: 0,
            after_bytes: 0,
        });
        engine.testing_push_event(WindowEvent::TextInput {
            surface_id: "other".into(),
            text: "kept".into(),
        });

        engine.testing_push_surface_closed("panel");

        let events = engine
            .poll_events()
            .expect("testing backend remains connected");
        assert!(matches!(
            events.as_slice(),
            [WindowEvent::TextInput { surface_id, text }]
                if surface_id.as_ref() == "other" && text.as_ref() == "kept"
        ));
        assert_eq!(
            engine.take_surface_lifecycle_events(),
            [SurfaceLifecycleEvent::Closed {
                surface_id: "panel".to_string()
            }]
        );
    }

    #[test]
    fn testing_surface_teardown_clears_only_its_text_input_state() {
        let mut engine = PresentationEngine::testing_with_popup_support(true);
        engine
            .configure("panel", SurfaceConfig::default())
            .expect("surface config should be accepted");
        engine
            .configure("other", SurfaceConfig::default())
            .expect("second surface config should be accepted");

        let other_state = TextInputState::new("other", 5, 5);
        engine
            .set_text_input_state(Some("other"), Some(other_state.clone()))
            .expect("other surface text-input state should be accepted");
        engine.testing_push_surface_closed("panel");
        assert_eq!(
            engine.testing_text_input_state(),
            Some(("other".to_string(), other_state))
        );

        engine
            .set_text_input_state(Some("panel"), Some(TextInputState::new("panel", 5, 5)))
            .expect("panel text-input state should be accepted");
        engine
            .configure("panel", SurfaceConfig::default())
            .expect("a closed surface should be configurable again");
        engine.destroy_surface("panel");
        assert!(engine.testing_text_input_state().is_none());

        engine
            .configure("panel", SurfaceConfig::default())
            .expect("the parent should be configurable again");
        engine
            .configure_popup(
                "popup",
                PopupConfig {
                    parent_surface_id: "panel".to_string(),
                    placement: PopupPlacement::default(),
                    padding: SurfacePadding::default(),
                    grab: false,
                    grab_identity: None,
                },
            )
            .expect("popup config should be accepted");
        engine
            .set_text_input_state(Some("popup"), Some(TextInputState::new("popup", 5, 5)))
            .expect("popup text-input state should be accepted");
        engine.testing_push_dismissed_popup("popup");
        assert!(engine.testing_text_input_state().is_none());

        engine
            .configure_popup(
                "popup",
                PopupConfig {
                    parent_surface_id: "panel".to_string(),
                    placement: PopupPlacement::default(),
                    padding: SurfacePadding::default(),
                    grab: false,
                    grab_identity: None,
                },
            )
            .expect("popup config should be accepted again");
        engine
            .set_text_input_state(Some("popup"), Some(TextInputState::new("popup", 5, 5)))
            .expect("popup text-input state should be accepted again");
        engine.destroy_surface("panel");
        assert!(engine.testing_text_input_state().is_none());

        engine
            .configure("settings", SurfaceConfig::default())
            .expect("another surface should be accepted");
        engine
            .set_text_input_state(
                Some("settings"),
                Some(TextInputState::new("settings", 8, 8)),
            )
            .expect("settings text-input state should be accepted");
        engine.testing_push_connection_lost("compositor exited");
        assert!(engine.testing_text_input_state().is_none());
    }

    #[test]
    fn state_only_commit_reports_pending_region_work_once() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        engine
            .configure("panel", SurfaceConfig::default())
            .expect("testing surface configuration succeeds");

        assert_eq!(
            engine.commit_surface_state("panel").unwrap(),
            SurfaceStateStatus::Unchanged
        );

        engine.update_opaque_region(
            "panel",
            Some(DamageRect {
                x: 2,
                y: 3,
                width: 20,
                height: 10,
            }),
        );
        assert_eq!(
            engine.commit_surface_state("panel").unwrap(),
            SurfaceStateStatus::Committed
        );
        assert_eq!(engine.testing_surface_state_commits(), ["panel"]);
        assert_eq!(
            engine.commit_surface_state("panel").unwrap(),
            SurfaceStateStatus::Unchanged
        );

        engine.testing_set_surface_configured("panel", false);
        engine.update_blur_regions("panel", Vec::new());
        assert_eq!(
            engine.commit_surface_state("panel").unwrap(),
            SurfaceStateStatus::NotReady
        );
        engine.testing_set_surface_configured("panel", true);
        assert_eq!(
            engine.commit_surface_state("panel").unwrap(),
            SurfaceStateStatus::Committed
        );
        assert_eq!(engine.testing_surface_state_commits(), ["panel", "panel"]);
    }

    #[test]
    fn unconfigured_surface_returns_typed_not_ready_without_recording_present() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        engine.testing_set_surface_configured("panel", false);
        let buffer = PixelBuffer::new(32, 16);
        let damage = [DamageRect {
            x: 0,
            y: 0,
            width: 32,
            height: 16,
        }];

        let status = engine
            .present_with_damage("panel", "Panel", true, &buffer, &damage)
            .expect("not-ready is a normal presentation outcome");

        assert_eq!(status, PresentStatus::NotReady);
        assert!(!engine.surface_ready_to_present("panel"));
        assert!(engine.testing_presented_surfaces().is_empty());

        engine.testing_set_surface_configured("panel", true);
        assert_eq!(
            engine
                .present_with_damage("panel", "Panel", true, &buffer, &damage)
                .expect("configured surface should present"),
            PresentStatus::Presented
        );
        assert_eq!(engine.testing_presented_surfaces(), ["panel"]);
    }

    #[test]
    fn testing_backend_separates_frame_pacing_from_buffer_backpressure() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        engine
            .configure("panel", SurfaceConfig::default())
            .expect("testing surface configuration succeeds");
        let buffer = PixelBuffer::new(32, 16);
        let damage = [DamageRect {
            x: 0,
            y: 0,
            width: 32,
            height: 16,
        }];

        engine.testing_set_surface_frame_pending("panel", true);
        assert!(engine.surface_waiting_for_frame_callback("panel"));
        assert!(!engine.surface_waiting_for_buffer_release("panel"));
        assert_eq!(
            engine
                .present_with_damage("panel", "Panel", true, &buffer, &damage)
                .unwrap(),
            PresentStatus::NotReady
        );

        engine.testing_set_surface_frame_pending("panel", false);
        engine.testing_set_surface_buffer_backpressured("panel", true);
        assert!(!engine.surface_waiting_for_frame_callback("panel"));
        assert!(engine.surface_waiting_for_buffer_release("panel"));
        assert_eq!(
            engine
                .present_with_damage("panel", "Panel", true, &buffer, &damage)
                .unwrap(),
            PresentStatus::NotReady
        );

        engine.testing_set_surface_buffer_backpressured("panel", false);
        assert!(!engine.surface_waiting_for_buffer_release("panel"));
        assert_eq!(
            engine
                .present_with_damage("panel", "Panel", true, &buffer, &damage)
                .unwrap(),
            PresentStatus::Presented
        );
    }

    #[test]
    fn testing_backend_models_scale_change_as_full_redraw() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        assert!(!engine.testing_set_surface_scale("panel", 1.5));
        engine
            .configure("panel", SurfaceConfig::default())
            .expect("testing surface configuration succeeds");

        assert_eq!(engine.surface_scale("panel"), 1.0);
        assert!(!engine.surface_needs_full_redraw("panel"));
        assert!(engine.testing_set_surface_scale("panel", 1.5));
        assert_eq!(engine.surface_scale("panel"), 1.5);
        assert!(engine.surface_needs_full_redraw("panel"));

        // Repeating the same compositor scale does not create another redraw
        // obligation, and clearing the consumed obligation is observable.
        assert!(engine.testing_set_surface_scale("panel", 1.5));
        engine.clear_surface_needs_full_redraw("panel");
        assert!(!engine.surface_needs_full_redraw("panel"));

        // The simulator follows the real preferred-scale bounds and clears
        // scale/redraw state when the compositor destroys the surface.
        assert!(engine.testing_set_surface_scale("panel", 5.0));
        assert_eq!(engine.surface_scale("panel"), 4.0);
        engine.testing_set_surface_missing("panel", true);
        assert!(!engine.testing_set_surface_scale("panel", 2.0));
        engine.testing_set_surface_missing("panel", false);
        engine.testing_push_surface_closed("panel");
        assert_eq!(engine.surface_scale("panel"), 1.0);
        assert!(!engine.surface_needs_full_redraw("panel"));
        assert!(!engine.testing_set_surface_scale("panel", f32::NAN));
    }

    #[test]
    fn testing_backend_retains_multi_output_membership_and_invalidates_once_per_change() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        engine
            .configure("panel", SurfaceConfig::default())
            .expect("testing surface configuration succeeds");

        assert!(engine.testing_set_surface_output("panel", 1, true));
        assert!(engine.testing_set_surface_output("panel", 2, true));
        assert_eq!(engine.testing_surface_outputs("panel"), [1, 2]);
        assert_eq!(engine.testing_surface_output_generation("panel"), 2);
        assert!(engine.surface_needs_full_redraw("panel"));
        engine.clear_surface_needs_full_redraw("panel");

        // A leave for output 1 must preserve output 2 and still record the
        // membership revision. Re-entering output 1 promotes it to the active
        // geometry choice without creating a duplicate member.
        assert!(engine.testing_set_surface_output("panel", 1, false));
        assert_eq!(engine.testing_surface_outputs("panel"), [2]);
        assert_eq!(engine.testing_surface_output_generation("panel"), 3);
        assert!(engine.surface_needs_full_redraw("panel"));
        engine.clear_surface_needs_full_redraw("panel");

        assert!(engine.testing_set_surface_output("panel", 1, true));
        assert_eq!(engine.testing_surface_outputs("panel"), [2, 1]);
        assert!(!engine.testing_set_surface_output("panel", 1, true));
        assert_eq!(engine.testing_surface_output_generation("panel"), 4);

        engine.testing_set_surface_missing("panel", true);
        assert!(engine.testing_surface_outputs("panel").is_empty());
        assert_eq!(engine.testing_surface_output_generation("panel"), 0);
    }

    #[test]
    fn missing_surface_returns_non_delivery_without_recording_present() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        engine.testing_set_surface_missing("panel", true);
        let buffer = PixelBuffer::new(32, 16);
        let damage = [DamageRect {
            x: 0,
            y: 0,
            width: 32,
            height: 16,
        }];

        assert_eq!(
            engine
                .present_with_damage("panel", "Panel", true, &buffer, &damage)
                .unwrap(),
            PresentStatus::SurfaceMissing
        );
        assert!(engine.testing_presented_surfaces().is_empty());
    }

    #[test]
    fn failed_configure_does_not_replace_the_last_accepted_config() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        let accepted = SurfaceConfig::default();
        engine
            .configure("panel", accepted.clone())
            .expect("the initial config should be accepted");
        engine.testing_fail_next_configure("synthetic surface creation failure");

        let replacement = SurfaceConfig {
            width: 640,
            height: 480,
            ..accepted.clone()
        };
        let error = engine
            .configure("panel", replacement)
            .expect_err("the injected creation failure must be observable");
        assert!(
            matches!(error, PresentationError::SurfaceCreate(message) if message == "synthetic surface creation failure")
        );
        assert_eq!(
            engine.testing_surface_configs(),
            [("panel".to_string(), accepted.clone())]
        );
        assert_eq!(
            engine.testing_surface_config_history(),
            [("panel".to_string(), accepted)]
        );
    }

    #[test]
    fn lifecycle_events_are_typed_and_testing_teardown_is_idempotent() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        engine
            .configure("panel", SurfaceConfig::default())
            .expect("surface config should be accepted");
        engine.testing_push_surface_closed("panel");

        assert_eq!(
            engine.take_surface_lifecycle_events(),
            [SurfaceLifecycleEvent::Closed {
                surface_id: "panel".to_string()
            }]
        );
        assert!(engine.take_surface_lifecycle_events().is_empty());
        assert!(engine.testing_surface_configs().is_empty());

        engine
            .configure_popup(
                "popup",
                PopupConfig {
                    parent_surface_id: "panel".to_string(),
                    placement: PopupPlacement::default(),
                    padding: SurfacePadding::default(),
                    grab: false,
                    grab_identity: None,
                },
            )
            .expect("popup config should be accepted by the testing backend");
        engine.testing_push_dismissed_popup("popup");
        assert_eq!(
            engine.take_surface_lifecycle_events(),
            [SurfaceLifecycleEvent::Dismissed {
                surface_id: "popup".to_string()
            }]
        );

        engine
            .configure("panel", SurfaceConfig::default())
            .expect("a closed surface should be configurable again");
        engine.destroy_surface("panel");
        engine.destroy_surface("panel");
        assert_eq!(engine.testing_destroyed_surfaces(), ["panel"]);
    }

    #[test]
    fn connection_loss_tears_down_all_surfaces_once_and_stays_typed() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        engine
            .configure("settings", SurfaceConfig::default())
            .unwrap();
        engine.configure("panel", SurfaceConfig::default()).unwrap();

        engine.testing_push_connection_lost("compositor exited");
        assert!(engine.testing_surface_configs().is_empty());
        assert_eq!(
            engine.take_surface_lifecycle_events(),
            [
                SurfaceLifecycleEvent::Lost {
                    surface_id: "panel".to_string(),
                    reason: "compositor exited".to_string(),
                },
                SurfaceLifecycleEvent::Lost {
                    surface_id: "settings".to_string(),
                    reason: "compositor exited".to_string(),
                },
            ]
        );

        engine.testing_push_connection_lost("a second failure");
        assert!(engine.take_surface_lifecycle_events().is_empty());
        assert!(matches!(
            engine.configure("panel", SurfaceConfig::default()),
            Err(PresentationError::ConnectionLost(reason)) if reason == "compositor exited"
        ));
        assert!(matches!(
            engine.poll_events(),
            Err(PresentationError::ConnectionLost(reason)) if reason == "compositor exited"
        ));
    }

    #[test]
    fn presents_share_one_explicit_frame_completion() {
        let mut engine = PresentationEngine::testing_with_popup_support(false);
        let buffer = PixelBuffer::new(32, 16);
        let damage = [DamageRect {
            x: 0,
            y: 0,
            width: 32,
            height: 16,
        }];

        engine
            .present_with_damage("panel", "Panel", true, &buffer, &damage)
            .unwrap();
        engine
            .present_with_damage("overlay", "Overlay", true, &buffer, &damage)
            .unwrap();
        assert_eq!(engine.testing_completed_frames(), 0);

        engine.finish_frame().unwrap();
        assert_eq!(engine.testing_completed_frames(), 1);
        assert_eq!(engine.testing_presented_surfaces(), ["panel", "overlay"]);
    }

    fn pointer_move(surface_id: &str, x: f32, y: f32) -> WindowEvent {
        WindowEvent::PointerMove {
            surface_id: surface_id.into(),
            x,
            y,
        }
    }

    fn scroll(surface_id: &str, x: f32, y: f32, dx: f32, dy: f32) -> WindowEvent {
        WindowEvent::Scroll {
            surface_id: surface_id.into(),
            x,
            y,
            dx,
            dy,
        }
    }

    fn two_finger_scroll(surface_id: &str, x: f32, y: f32, dx: f32, dy: f32) -> WindowEvent {
        WindowEvent::TwoFingerScroll {
            surface_id: surface_id.into(),
            x,
            y,
            dx,
            dy,
        }
    }

    #[test]
    fn coalesces_single_surface_pointer_moves_without_losing_latest_position() {
        let events = coalesce_input_events(vec![
            pointer_move("panel", 1.0, 2.0),
            pointer_move("panel", 3.0, 4.0),
            WindowEvent::PointerButton {
                surface_id: "panel".into(),
                x: 3.0,
                y: 4.0,
                button: PRIMARY_POINTER_BUTTON,
                pressed: true,
            },
        ]);

        assert_eq!(events.len(), 2);
        match &events[0] {
            WindowEvent::PointerMove { surface_id, x, y } => {
                assert_eq!(surface_id.as_ref(), "panel");
                assert_eq!((*x, *y), (3.0, 4.0));
            }
            event => panic!("expected pointer move, got {event:?}"),
        }
    }

    #[test]
    fn coalesces_multiple_surfaces_only_until_surface_specific_event() {
        let events = coalesce_input_events(vec![
            pointer_move("panel", 1.0, 1.0),
            pointer_move("popover", 2.0, 2.0),
            pointer_move("panel", 3.0, 3.0),
            scroll("panel", 3.0, 3.0, 0.0, 1.0),
        ]);

        assert_eq!(events.len(), 3);
        match &events[0] {
            WindowEvent::PointerMove { surface_id, x, y } => {
                assert_eq!(surface_id.as_ref(), "panel");
                assert_eq!((*x, *y), (3.0, 3.0));
            }
            event => panic!("expected panel pointer move, got {event:?}"),
        }
        assert!(
            events
                .iter()
                .any(|event| matches!(event, WindowEvent::PointerMove { surface_id, x, y } if surface_id.as_ref() == "popover" && (*x, *y) == (2.0, 2.0)))
        );
        assert!(matches!(events[2], WindowEvent::Scroll { .. }));
    }

    #[test]
    fn coalesces_scroll_deltas_for_same_surface() {
        let events = coalesce_input_events(vec![
            scroll("panel", 10.0, 20.0, 0.0, 1.0),
            scroll("panel", 11.0, 21.0, 0.5, 2.0),
            scroll("panel", 12.0, 22.0, 1.0, 3.0),
        ]);

        assert_eq!(events.len(), 1);
        match &events[0] {
            WindowEvent::Scroll {
                surface_id,
                x,
                y,
                dx,
                dy,
            } => {
                assert_eq!(surface_id.as_ref(), "panel");
                assert_eq!((*x, *y), (12.0, 22.0));
                assert_eq!((*dx, *dy), (1.5, 6.0));
            }
            event => panic!("expected scroll, got {event:?}"),
        }
    }

    #[test]
    fn coalesces_two_finger_scroll_deltas_for_same_surface() {
        let events = coalesce_input_events(vec![
            two_finger_scroll("panel", 10.0, 20.0, 1.0, 2.0),
            two_finger_scroll("panel", 12.0, 22.0, 3.0, 4.0),
        ]);

        assert!(matches!(
            events.as_slice(),
            [WindowEvent::TwoFingerScroll { x, y, dx, dy, .. }]
                if (*x, *y, *dx, *dy) == (12.0, 22.0, 4.0, 6.0)
        ));
    }

    #[test]
    fn pointer_moves_and_scrolls_flush_each_other_in_order() {
        let events = coalesce_input_events(vec![
            pointer_move("panel", 1.0, 1.0),
            pointer_move("panel", 2.0, 2.0),
            scroll("panel", 2.0, 2.0, 0.0, 1.0),
            scroll("panel", 2.0, 2.0, 0.0, 2.0),
            pointer_move("panel", 3.0, 3.0),
        ]);

        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0],
            WindowEvent::PointerMove { ref surface_id, x, y }
                if surface_id.as_ref() == "panel" && (x, y) == (2.0, 2.0)
        ));
        assert!(matches!(
            events[1],
            WindowEvent::Scroll { ref surface_id, dx, dy, .. }
                if surface_id.as_ref() == "panel" && (dx, dy) == (0.0, 3.0)
        ));
        assert!(matches!(
            events[2],
            WindowEvent::PointerMove { ref surface_id, x, y }
                if surface_id.as_ref() == "panel" && (x, y) == (3.0, 3.0)
        ));
    }
}
