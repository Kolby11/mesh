mod dev_window;
mod wayland_surface;

use super::{PixelBuffer, RenderError};

pub use dev_window::{DevWindowEvent as WindowEvent, DevWindowKeyEvent as WindowKeyEvent};
pub use wayland_surface::LayerSurfaceConfig;

use dev_window::DevWindowBackend;
use wayland_surface::LayerShellBackend;

pub struct PresentationBridge {
    backend: Backend,
}

enum Backend {
    WaylandSurface(LayerShellBackend),
    DevWindow(DevWindowBackend),
}

impl PresentationBridge {
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
    ) -> Result<(), RenderError> {
        match &mut self.backend {
            Backend::WaylandSurface(bridge) => bridge.present(surface_id, title, visible, buffer),
            Backend::DevWindow(bridge) => bridge.present(surface_id, title, visible, buffer),
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
