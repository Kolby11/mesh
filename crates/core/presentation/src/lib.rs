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
    let mut pending_moves: HashMap<String, WindowEvent> = HashMap::new();

    for event in events {
        match event {
            WindowEvent::PointerMove { surface_id, x, y } => {
                pending_moves.insert(
                    surface_id.clone(),
                    WindowEvent::PointerMove { surface_id, x, y },
                );
            }
            WindowEvent::PointerLeave { surface_id } => {
                pending_moves.remove(&surface_id);
                output.push(WindowEvent::PointerLeave { surface_id });
            }
            event => {
                let surface_id = event_surface_id(&event).to_string();
                if let Some(pointer_move) = pending_moves.remove(&surface_id) {
                    output.push(pointer_move);
                }
                output.push(event);
            }
        }
    }

    output.extend(pending_moves.into_values());
    output
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
