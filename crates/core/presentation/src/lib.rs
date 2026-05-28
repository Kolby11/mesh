mod dev_window;
mod wayland_surface;

use std::collections::HashMap;

use mesh_core_render::{DamageRect, PixelBuffer};

pub use dev_window::{DevWindowEvent as WindowEvent, DevWindowKeyEvent as WindowKeyEvent, KeyMods};
pub use wayland_surface::{LayerSurfaceConfig, LayerSurfaceSizePolicy};

use dev_window::DevWindowBackend;
use wayland_surface::LayerShellBackend;

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
    WaylandSurface(LayerShellBackend),
    DevWindow(DevWindowBackend),
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
                    Backend::WaylandSurface(bridge)
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

    pub fn configure(&mut self, surface_id: &str, cfg: LayerSurfaceConfig) {
        if let Backend::WaylandSurface(bridge) = &mut self.backend {
            bridge.configure(surface_id, cfg);
        }
    }

    pub fn present(
        &mut self,
        surface_id: &str,
        title: &str,
        visible: bool,
        buffer: &PixelBuffer,
    ) -> Result<(), PresentationError> {
        self.present_with_damage(surface_id, title, visible, buffer, None)
    }

    pub fn present_with_damage(
        &mut self,
        surface_id: &str,
        title: &str,
        visible: bool,
        buffer: &PixelBuffer,
        damage: Option<DamageRect>,
    ) -> Result<(), PresentationError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => {
                bridge.present_with_damage(surface_id, title, visible, buffer, damage)
            }
            Backend::DevWindow(bridge) => bridge.present(surface_id, title, visible, buffer),
        }
    }

    pub fn surface_size(
        &mut self,
        surface_id: &str,
    ) -> Result<Option<(u32, u32)>, PresentationError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.surface_size(surface_id),
            Backend::DevWindow(_) => Ok(None),
        }
    }

    pub fn surface_size_if_known(&self, surface_id: &str) -> Option<(u32, u32)> {
        match &self.backend {
            Backend::WaylandSurface(bridge) => bridge.surface_size_if_known(surface_id),
            Backend::DevWindow(_) => None,
        }
    }

    pub fn pump(&mut self) {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.pump(),
            Backend::DevWindow(bridge) => bridge.pump(),
        }
    }

    pub fn poll_events(&mut self) -> Vec<WindowEvent> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.poll_events(),
            Backend::DevWindow(bridge) => bridge.poll_events(),
        }
    }
}

impl Default for PresentationEngine {
    fn default() -> Self {
        Self::select()
    }
}

pub fn coalesce_pointer_moves(events: Vec<WindowEvent>) -> Vec<WindowEvent> {
    if events.len() < 2 {
        return events;
    }

    let mut output = Vec::with_capacity(events.len());
    let mut pending_move: Option<WindowEvent> = None;
    let mut pending_moves: Option<HashMap<String, WindowEvent>> = None;

    for event in events {
        match event {
            WindowEvent::PointerMove { surface_id, x, y } => {
                let next_move = WindowEvent::PointerMove { surface_id, x, y };
                push_pending_pointer_move(next_move, &mut pending_move, &mut pending_moves);
            }
            WindowEvent::PointerLeave { surface_id } => {
                remove_pending_pointer_move(&surface_id, &mut pending_move, &mut pending_moves);
                output.push(WindowEvent::PointerLeave { surface_id });
            }
            event => {
                let surface_id = event_surface_id(&event);
                if let Some(pointer_move) =
                    remove_pending_pointer_move(surface_id, &mut pending_move, &mut pending_moves)
                {
                    output.push(pointer_move);
                }
                output.push(event);
            }
        }
    }

    if let Some(pointer_move) = pending_move {
        output.push(pointer_move);
    }
    if let Some(pending_moves) = pending_moves {
        output.extend(pending_moves.into_values());
    }
    output
}

fn push_pending_pointer_move(
    event: WindowEvent,
    pending_move: &mut Option<WindowEvent>,
    pending_moves: &mut Option<HashMap<String, WindowEvent>>,
) {
    let WindowEvent::PointerMove { surface_id, .. } = &event else {
        return;
    };
    if let Some(map) = pending_moves.as_mut() {
        map.insert(surface_id.clone(), event);
        return;
    }
    match pending_move {
        Some(existing) if event_surface_id(existing) == surface_id => {
            *existing = event;
        }
        Some(existing) => {
            let mut map = HashMap::with_capacity(4);
            map.insert(
                event_surface_id(existing).to_string(),
                pending_move.take().unwrap(),
            );
            map.insert(surface_id.clone(), event);
            *pending_moves = Some(map);
        }
        None => {
            *pending_move = Some(event);
        }
    }
}

fn remove_pending_pointer_move(
    surface_id: &str,
    pending_move: &mut Option<WindowEvent>,
    pending_moves: &mut Option<HashMap<String, WindowEvent>>,
) -> Option<WindowEvent> {
    if pending_move
        .as_ref()
        .is_some_and(|event| event_surface_id(event) == surface_id)
    {
        return pending_move.take();
    }
    pending_moves
        .as_mut()
        .and_then(|map| map.remove(surface_id))
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
            surface_id: surface_id.to_string(),
            x,
            y,
        }
    }

    #[test]
    fn coalesces_single_surface_pointer_moves_without_losing_latest_position() {
        let events = coalesce_pointer_moves(vec![
            pointer_move("panel", 1.0, 2.0),
            pointer_move("panel", 3.0, 4.0),
            WindowEvent::PointerButton {
                surface_id: "panel".to_string(),
                x: 3.0,
                y: 4.0,
                pressed: true,
            },
        ]);

        assert_eq!(events.len(), 2);
        match &events[0] {
            WindowEvent::PointerMove { surface_id, x, y } => {
                assert_eq!(surface_id, "panel");
                assert_eq!((*x, *y), (3.0, 4.0));
            }
            event => panic!("expected pointer move, got {event:?}"),
        }
    }

    #[test]
    fn coalesces_multiple_surfaces_only_until_surface_specific_event() {
        let events = coalesce_pointer_moves(vec![
            pointer_move("panel", 1.0, 1.0),
            pointer_move("popover", 2.0, 2.0),
            pointer_move("panel", 3.0, 3.0),
            WindowEvent::Scroll {
                surface_id: "panel".to_string(),
                x: 3.0,
                y: 3.0,
                dx: 0.0,
                dy: 1.0,
            },
        ]);

        assert_eq!(events.len(), 3);
        match &events[0] {
            WindowEvent::PointerMove { surface_id, x, y } => {
                assert_eq!(surface_id, "panel");
                assert_eq!((*x, *y), (3.0, 3.0));
            }
            event => panic!("expected panel pointer move, got {event:?}"),
        }
        assert!(matches!(events[1], WindowEvent::Scroll { .. }));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, WindowEvent::PointerMove { surface_id, x, y } if surface_id == "popover" && (*x, *y) == (2.0, 2.0)))
        );
    }
}
