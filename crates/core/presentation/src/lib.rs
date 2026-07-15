mod dev_window;
mod wayland_surface;

use std::collections::HashMap;
use std::os::unix::io::BorrowedFd;

use mesh_core_render::{DamageRect, PixelBuffer};

pub use dev_window::{DevWindowEvent as WindowEvent, DevWindowKeyEvent as WindowKeyEvent, KeyMods};
pub use wayland_surface::{
    LayerSurfaceConfig, LayerSurfaceSizePolicy, PopupAnchor, PopupConfig, PopupConstraint,
    PopupGravity, PopupPlacement,
};

use dev_window::DevWindowBackend;
use wayland_surface::LayerShellBackend;

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
}

pub struct PresentationEngine {
    backend: Backend,
}

enum Backend {
    WaylandSurface(Box<LayerShellBackend>),
    DevWindow(DevWindowBackend),
    Testing(TestingBackend),
}

#[derive(Default)]
struct TestingBackend {
    popup_supported: bool,
    popup_configs: HashMap<String, PopupConfig>,
    destroyed_popups: Vec<String>,
    dismissed_popups: Vec<String>,
    events: Vec<WindowEvent>,
    presented: Vec<String>,
    presented_damage: Vec<(String, Vec<DamageRect>)>,
}

impl PresentationEngine {
    pub fn select() -> Self {
        let forced = std::env::var("MESH_BACKEND").ok();
        let want_dev = forced.as_deref() == Some("dev-window");
        let want_wayland = forced.as_deref() == Some("layer-shell");
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();

        let backend = if !want_dev && (want_wayland || wayland) {
            match LayerShellBackend::new() {
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
            backend: Backend::Testing(TestingBackend {
                popup_supported,
                ..TestingBackend::default()
            }),
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
    pub fn testing_push_dismissed_popup(&mut self, surface_id: impl Into<String>) {
        if let Backend::Testing(backend) = &mut self.backend {
            backend.dismissed_popups.push(surface_id.into());
        }
    }

    #[doc(hidden)]
    pub fn testing_push_event(&mut self, event: WindowEvent) {
        if let Backend::Testing(backend) = &mut self.backend {
            backend.events.push(event);
        }
    }

    pub fn configure(&mut self, surface_id: &str, cfg: LayerSurfaceConfig) {
        if let Backend::WaylandSurface(bridge) = &mut self.backend {
            bridge.configure(surface_id, cfg);
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
                backend.popup_configs.remove(surface_id);
                backend.destroyed_popups.push(surface_id.to_string());
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
                    backend.popup_configs.remove(&id);
                    backend.destroyed_popups.push(id);
                }
            }
        }
    }

    /// Drain the ids of popups the compositor dismissed since the last call so
    /// the shell can drop the matching popup targets. Always empty on dev-window.
    pub fn take_dismissed_popups(&mut self) -> Vec<String> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.take_dismissed_popups(),
            Backend::DevWindow(_) => Vec::new(),
            Backend::Testing(backend) => std::mem::take(&mut backend.dismissed_popups),
        }
    }

    pub fn present(
        &mut self,
        surface_id: &str,
        title: &str,
        visible: bool,
        buffer: &PixelBuffer,
    ) -> Result<(), PresentationError> {
        // `present()` is only used by DevWindow callers. Pass a full-damage
        // slice so the Wayland path would get a complete upload if ever
        // reached, but in practice this only hits Backend::DevWindow.
        let full = DamageRect {
            x: 0,
            y: 0,
            width: buffer.width.max(1),
            height: buffer.height.max(1),
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
    ) -> Result<(), PresentationError> {
        let _span =
            tracing::debug_span!("present_with_damage", surface_id, rects = damage.len()).entered();
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => {
                bridge.present_with_damage(surface_id, title, visible, buffer, damage)
            }
            Backend::DevWindow(bridge) => bridge.present(surface_id, title, visible, buffer),
            Backend::Testing(backend) => {
                if visible {
                    backend.presented.push(surface_id.to_string());
                    backend
                        .presented_damage
                        .push((surface_id.to_string(), damage.to_vec()));
                }
                Ok(())
            }
        }
    }

    pub fn update_opaque_region(&mut self, surface_id: &str, opaque_rect: Option<DamageRect>) {
        if let Backend::WaylandSurface(bridge) = &mut self.backend {
            bridge.update_opaque_region(surface_id, opaque_rect);
        }
    }

    /// Restrict the surface's input region (logical coordinates) so clicks over
    /// the tooltip-overlay buffer padding fall through to the windows beneath.
    /// `None` resets to whole-surface input.
    pub fn update_input_region(&mut self, surface_id: &str, input_rect: Option<DamageRect>) {
        if let Backend::WaylandSurface(bridge) = &mut self.backend {
            bridge.update_input_region(surface_id, input_rect);
        }
    }

    /// Set the logical-coordinate blur region for a surface.
    /// Only meaningful on Wayland backends with `org_kde_kwin_blur` support.
    /// Pass `None` to clear any previously committed blur region from the
    /// compositor. No protocol calls are emitted if no blur region has ever
    /// been set for this surface.
    pub fn update_blur_region(&mut self, surface_id: &str, blur_region: Option<DamageRect>) {
        if let Backend::WaylandSurface(bridge) = &mut self.backend {
            bridge.update_blur_region(surface_id, blur_region);
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
            Backend::Testing(_) => false,
        }
    }

    pub fn surface_scale(&self, surface_id: &str) -> f32 {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.surface_scale(surface_id),
            Backend::DevWindow(_) => 1.0,
            Backend::Testing(_) => 1.0,
        }
    }

    pub fn surface_needs_full_redraw(&self, surface_id: &str) -> bool {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.surface_needs_full_redraw(surface_id),
            Backend::DevWindow(_) => false,
            Backend::Testing(_) => false,
        }
    }

    pub fn clear_surface_needs_full_redraw(&mut self, surface_id: &str) {
        if let Backend::WaylandSurface(bridge) = &mut self.backend {
            bridge.clear_surface_needs_full_redraw(surface_id);
        }
    }

    pub fn pump(&mut self) {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.pump(),
            Backend::DevWindow(bridge) => bridge.pump(),
            Backend::Testing(_) => {}
        }
    }

    pub fn poll_events(&mut self) -> Vec<WindowEvent> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.poll_events(),
            Backend::DevWindow(bridge) => bridge.poll_events(),
            Backend::Testing(backend) => std::mem::take(&mut backend.events),
        }
    }

    pub fn set_pointer_interactive(&mut self, interactive: bool) {
        if let Backend::WaylandSurface(bridge) = &mut self.backend {
            bridge.set_pointer_interactive(interactive);
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
}

impl PendingInputEvent {
    fn surface_id(&self) -> &str {
        match self {
            Self::PointerMove { surface_id, .. } | Self::Scroll { surface_id, .. } => surface_id,
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
        matches!(event, PendingInputEvent::Scroll { .. }) && event.surface_id() == surface_id
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
        | WindowEvent::Scroll { surface_id, .. }
        | WindowEvent::Key { surface_id, .. }
        | WindowEvent::Char { surface_id, .. } => surface_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn coalesces_single_surface_pointer_moves_without_losing_latest_position() {
        let events = coalesce_input_events(vec![
            pointer_move("panel", 1.0, 2.0),
            pointer_move("panel", 3.0, 4.0),
            WindowEvent::PointerButton {
                surface_id: "panel".into(),
                x: 3.0,
                y: 4.0,
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
