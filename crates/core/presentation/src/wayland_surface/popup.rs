//! `xdg_popup` promotion primitive for the layer-shell backend.
//!
//! A MESH `<popover>` is authored inline as a child of its trigger component but
//! is *promoted* at runtime into its own compositor surface so it can paint
//! outside the host surface's fixed buffer. On wlr-layer-shell the intended
//! anchored-menu primitive is an `xdg_popup` created as a child of the parent
//! layer surface via `zwlr_layer_surface_v1.get_popup` plus an `xdg_positioner`.
//!
//! This module owns the presentation-level placement description and its pure
//! mapping onto the Wayland `xdg_positioner` enums. It deliberately mirrors the
//! shell-side `mesh_core_elements::PopoverPlacement` field-for-field but stays
//! independent of that crate: the shell (which depends on both) translates the
//! element-model placement into this presentation type, so the presentation
//! backend never needs to know about the element model.

use wayland_protocols::xdg::shell::client::xdg_positioner;

/// Edge/corner of the anchor rectangle the popup is positioned against.
///
/// Mirrors `xdg_positioner.anchor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopupAnchor {
    Center,
    Top,
    #[default]
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Direction the popup grows away from the anchor point.
///
/// Mirrors `xdg_positioner.gravity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopupGravity {
    Center,
    Top,
    #[default]
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// How the compositor may adjust placement to keep the popup on-screen.
///
/// Mirrors `xdg_positioner.constraint_adjustment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupConstraint {
    pub flip_x: bool,
    pub flip_y: bool,
    pub slide_x: bool,
    pub slide_y: bool,
    pub resize_x: bool,
    pub resize_y: bool,
}

impl Default for PopupConstraint {
    fn default() -> Self {
        Self {
            flip_x: true,
            flip_y: true,
            slide_x: true,
            slide_y: true,
            resize_x: false,
            resize_y: false,
        }
    }
}

/// Fully resolved placement handed to the presentation backend when promoting a
/// `<popover>` into an `xdg_popup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupPlacement {
    /// Anchor rectangle in the parent surface's window-geometry-local logical
    /// coordinates: `(x, y, width, height)`. The popup is positioned relative to
    /// this rectangle (typically the trigger element's layout box).
    pub anchor_rect: (i32, i32, i32, i32),
    /// Requested popup size in logical pixels (the measured `<popover>` subtree).
    pub size: (u32, u32),
    pub anchor: PopupAnchor,
    pub gravity: PopupGravity,
    pub constraint: PopupConstraint,
    /// Extra offset applied to the computed position, in logical pixels.
    pub offset: (i32, i32),
}

impl Default for PopupPlacement {
    fn default() -> Self {
        Self {
            anchor_rect: (0, 0, 0, 0),
            size: (1, 1),
            anchor: PopupAnchor::default(),
            gravity: PopupGravity::default(),
            constraint: PopupConstraint::default(),
            offset: (0, 0),
        }
    }
}

/// Request to promote a component into an `xdg_popup` child of an existing
/// (layer) surface owned by the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupConfig {
    /// `surface_id` of the parent surface (must be a layer surface owned by the
    /// backend). The popup is a Wayland child of this surface.
    pub parent_surface_id: String,
    pub placement: PopupPlacement,
    /// Take a compositor input grab (click-to-dismiss-outside + keyboard).
    /// Requires a recent input serial; ignored when no serial is available.
    pub grab: bool,
    /// The input event serial used for the grab. An `xdg_popup` grab is only
    /// valid in response to a recent click, so a pure-hover popover passes
    /// `grab = false` / `serial = None` and relies on the core hover-bridge.
    pub grab_serial: Option<u32>,
}

/// Map a [`PopupAnchor`] onto the Wayland `xdg_positioner` anchor enum.
pub(super) fn map_anchor(anchor: PopupAnchor) -> xdg_positioner::Anchor {
    use xdg_positioner::Anchor;
    match anchor {
        PopupAnchor::Center => Anchor::None,
        PopupAnchor::Top => Anchor::Top,
        PopupAnchor::Bottom => Anchor::Bottom,
        PopupAnchor::Left => Anchor::Left,
        PopupAnchor::Right => Anchor::Right,
        PopupAnchor::TopLeft => Anchor::TopLeft,
        PopupAnchor::TopRight => Anchor::TopRight,
        PopupAnchor::BottomLeft => Anchor::BottomLeft,
        PopupAnchor::BottomRight => Anchor::BottomRight,
    }
}

/// Map a [`PopupGravity`] onto the Wayland `xdg_positioner` gravity enum.
pub(super) fn map_gravity(gravity: PopupGravity) -> xdg_positioner::Gravity {
    use xdg_positioner::Gravity;
    match gravity {
        PopupGravity::Center => Gravity::None,
        PopupGravity::Top => Gravity::Top,
        PopupGravity::Bottom => Gravity::Bottom,
        PopupGravity::Left => Gravity::Left,
        PopupGravity::Right => Gravity::Right,
        PopupGravity::TopLeft => Gravity::TopLeft,
        PopupGravity::TopRight => Gravity::TopRight,
        PopupGravity::BottomLeft => Gravity::BottomLeft,
        PopupGravity::BottomRight => Gravity::BottomRight,
    }
}

/// Map a [`PopupConstraint`] onto the Wayland `xdg_positioner`
/// constraint-adjustment bitflags.
pub(super) fn map_constraint(constraint: PopupConstraint) -> xdg_positioner::ConstraintAdjustment {
    use xdg_positioner::ConstraintAdjustment;
    let mut adjustment = ConstraintAdjustment::None;
    if constraint.slide_x {
        adjustment |= ConstraintAdjustment::SlideX;
    }
    if constraint.slide_y {
        adjustment |= ConstraintAdjustment::SlideY;
    }
    if constraint.flip_x {
        adjustment |= ConstraintAdjustment::FlipX;
    }
    if constraint.flip_y {
        adjustment |= ConstraintAdjustment::FlipY;
    }
    if constraint.resize_x {
        adjustment |= ConstraintAdjustment::ResizeX;
    }
    if constraint.resize_y {
        adjustment |= ConstraintAdjustment::ResizeY;
    }
    adjustment
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_maps_to_wayland_enum() {
        assert_eq!(
            map_anchor(PopupAnchor::Bottom),
            xdg_positioner::Anchor::Bottom
        );
        assert_eq!(
            map_anchor(PopupAnchor::Center),
            xdg_positioner::Anchor::None
        );
        assert_eq!(
            map_anchor(PopupAnchor::TopRight),
            xdg_positioner::Anchor::TopRight
        );
    }

    #[test]
    fn gravity_maps_to_wayland_enum() {
        assert_eq!(
            map_gravity(PopupGravity::Bottom),
            xdg_positioner::Gravity::Bottom
        );
        assert_eq!(
            map_gravity(PopupGravity::Center),
            xdg_positioner::Gravity::None
        );
    }

    #[test]
    fn default_constraint_enables_flip_and_slide_both_axes() {
        let flags = map_constraint(PopupConstraint::default());
        use xdg_positioner::ConstraintAdjustment as CA;
        assert!(flags.contains(CA::FlipX));
        assert!(flags.contains(CA::FlipY));
        assert!(flags.contains(CA::SlideX));
        assert!(flags.contains(CA::SlideY));
        assert!(!flags.contains(CA::ResizeX));
        assert!(!flags.contains(CA::ResizeY));
    }

    #[test]
    fn empty_constraint_maps_to_none() {
        let none = PopupConstraint {
            flip_x: false,
            flip_y: false,
            slide_x: false,
            slide_y: false,
            resize_x: false,
            resize_y: false,
        };
        assert_eq!(
            map_constraint(none),
            xdg_positioner::ConstraintAdjustment::None
        );
    }
}
