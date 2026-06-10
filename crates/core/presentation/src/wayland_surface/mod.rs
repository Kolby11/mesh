//! wlr-layer-shell backend.
//!
//! Replaces `dev_window` (minifb XDG-toplevel) with real layer-shell surfaces so
//! panels/launchers/overlays are placed by the compositor as shell chrome
//! instead of being tiled as windows.

mod backend;
mod handlers;
mod state;

pub use backend::{LayerShellBackend, LayerSurfaceConfig, LayerSurfaceSizePolicy};

use crate::PresentationError;
use crate::dev_window::{DevWindowEvent, DevWindowKeyEvent, KeyMods};
use mesh_core_render::PixelBuffer;
use mesh_core_wayland::{Edge, KeyboardMode, Layer as MeshLayer};
use rustix::event::{PollFd, PollFlags, poll};
use smithay_client_toolkit::{
    activation::{ActivationHandler, ActivationState, RequestData},
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_activation, delegate_compositor, delegate_keyboard, delegate_layer, delegate_output,
    delegate_pointer, delegate_registry, delegate_seat, delegate_shm,
    globals::GlobalData,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability as SeatCapability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RepeatInfo},
        pointer::{
            CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
        },
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{
        Shm, ShmHandler,
        slot::{Buffer, SlotPool},
    },
};
use state::State;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::time::{Duration, Instant};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle,
    backend::WaylandError,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1, wp_fractional_scale_v1::WpFractionalScaleV1,
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport, wp_viewport::WpViewport, wp_viewporter, wp_viewporter::WpViewporter,
};
use wayland_protocols_hyprland::focus_grab::v1::client::{
    hyprland_focus_grab_manager_v1, hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1,
    hyprland_focus_grab_v1, hyprland_focus_grab_v1::HyprlandFocusGrabV1,
};
use wayland_protocols_plasma::blur::client::{
    org_kde_kwin_blur, org_kde_kwin_blur::OrgKdeKwinBlur, org_kde_kwin_blur_manager,
    org_kde_kwin_blur_manager::OrgKdeKwinBlurManager,
};
