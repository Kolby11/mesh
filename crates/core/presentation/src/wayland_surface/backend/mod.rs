//! The wlr-layer-shell / xdg-shell surface backend.
//!
//! Split by concern: [`config`] holds the shell-facing [`SurfaceConfig`] and its
//! semantic diff/clamping rules, [`protocol`] maps it onto Wayland requests,
//! [`entry`] owns one live compositor surface, [`shm`] and [`damage`] cover the
//! buffer pool and damage arithmetic, and [`surfaces`], [`present`], [`events`]
//! carry the three halves of [`WaylandSurfaceBackend`]'s inherent impl.

mod blur;
mod config;
mod damage;
mod entry;
mod events;
#[cfg(feature = "gpu")]
mod gpu;
mod present;
mod protocol;
mod shm;
mod surfaces;

#[cfg(test)]
mod tests;

pub(in crate::wayland_surface) use blur::*;
pub use config::*;
use damage::*;
pub(in crate::wayland_surface) use entry::*;
#[cfg(feature = "gpu")]
pub(in crate::wayland_surface) use gpu::{DmabufProtocol, DmabufRing, GpuPresenter};
pub(in crate::wayland_surface) use protocol::*;
use shm::*;

use super::*;

pub struct WaylandSurfaceBackend {
    _conn: Connection,
    event_queue: EventQueue<State>,
    state: State,
    #[cfg(feature = "gpu")]
    gpu: GpuPresenter,
    #[cfg(feature = "gpu")]
    scene_pending: Vec<gpu::PendingScene>,
    #[cfg(feature = "gpu")]
    scene_results: Vec<crate::SceneCompletion>,
}
