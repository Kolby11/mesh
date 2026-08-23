//! The wlr-layer-shell / xdg-shell surface backend.
//!
//! Split by concern: [`config`] holds the shell-facing [`SurfaceConfig`] and its
//! semantic diff/clamping rules, [`protocol`] maps it onto Wayland requests,
//! [`entry`] owns one live compositor surface, [`shm`] and [`damage`] cover the
//! buffer pool and damage arithmetic, and [`surfaces`], [`present`], [`events`]
//! carry the three halves of [`WaylandSurfaceBackend`]'s inherent impl.

mod config;
mod damage;
mod entry;
mod events;
mod present;
mod protocol;
mod shm;
mod surfaces;

#[cfg(test)]
mod tests;

pub use config::*;
use damage::*;
pub(in crate::wayland_surface) use entry::*;
pub(in crate::wayland_surface) use protocol::*;
use shm::*;

use super::*;

pub struct WaylandSurfaceBackend {
    _conn: Connection,
    event_queue: EventQueue<State>,
    state: State,
}
