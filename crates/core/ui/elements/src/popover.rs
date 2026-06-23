//! Typed placement model for `<popover>` surface promotion.
//!
//! A `<popover>` is authored inline as a child of its trigger's component, but
//! at runtime it is *promoted* into its own compositor surface (an `xdg_popup`
//! child of the host surface) so it can paint outside the host buffer. The host
//! buffer is a fixed pixel rectangle — `PixelBuffer::set_pixel` drops every
//! out-of-bounds write — so a menu that hangs below a short bar physically needs
//! a surface of its own.
//!
//! This module owns the placement data the shell hands to the presentation
//! backend when it promotes a popover. The field set maps 1:1 onto
//! `xdg_positioner` (anchor / gravity / constraint-adjustment / offset) so the
//! Wayland translation in `mesh-core-presentation` is a direct enum mapping with
//! no policy of its own.
//!
//! The placement is parsed from the element's attributes, e.g.
//! `<popover anchor="bottom" gravity="bottom" offset-y="4" grab="hover">`.

use crate::tree::WidgetNode;
use std::collections::BTreeMap;

/// Edge/corner of the anchor rectangle the popup is positioned against.
///
/// Mirrors `xdg_positioner.anchor`. `Center` means the popup is positioned
/// relative to the center of the anchor rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopoverAnchor {
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
/// Mirrors `xdg_positioner.gravity`. For a menu dropping below a top bar the
/// gravity is `Bottom` (the popup extends downward from its anchor edge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopoverGravity {
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
/// Mirrors `xdg_positioner.constraint_adjustment`. Defaults enable flip+slide
/// on both axes so an edge-anchored menu re-anchors instead of clipping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopoverConstraintAdjustment {
    pub flip_x: bool,
    pub flip_y: bool,
    pub slide_x: bool,
    pub slide_y: bool,
    pub resize_x: bool,
    pub resize_y: bool,
}

impl Default for PopoverConstraintAdjustment {
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

/// Input-grab policy for the promoted popup.
///
/// An `xdg_popup` grab requires a recent input *serial* — i.e. it can only be
/// taken in response to a click — so a popover opened purely on hover cannot
/// grab. The two policies make that constraint explicit per popover:
///
/// - [`PopoverGrab::Hover`] — no compositor grab. The popover opens on hover and
///   the core popover controller owns the hover-bridge / dismiss logic (so the
///   cursor can travel from trigger to popup without dismissing).
/// - [`PopoverGrab::Click`] — take the compositor grab using the click serial.
///   The compositor dismisses on outside-click and routes a keyboard grab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopoverGrab {
    #[default]
    Hover,
    Click,
}

/// Fully resolved placement for a promoted `<popover>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PopoverPlacement {
    pub anchor: PopoverAnchor,
    pub gravity: PopoverGravity,
    /// Extra offset applied to the computed position, in surface-local pixels.
    pub offset_x: i32,
    pub offset_y: i32,
    pub constraint_adjustment: PopoverConstraintAdjustment,
    pub grab: PopoverGrab,
}

impl PopoverPlacement {
    /// Parse placement from a `<popover>` node's attributes. Unknown or missing
    /// attributes fall back to the menu-friendly defaults (anchor/gravity below,
    /// flip+slide on, hover grab).
    pub fn from_node(node: &WidgetNode) -> Self {
        Self::from_attributes(&node.attributes)
    }

    /// Parse placement from a raw attribute map.
    pub fn from_attributes(attrs: &BTreeMap<String, String>) -> Self {
        let mut placement = Self::default();

        if let Some(anchor) = attrs.get("anchor").and_then(|v| parse_anchor(v)) {
            placement.anchor = anchor;
        }
        if let Some(gravity) = attrs.get("gravity").and_then(|v| parse_gravity(v)) {
            placement.gravity = gravity;
        } else if attrs.get("gravity").is_none() && attrs.contains_key("anchor") {
            // When only an anchor is given, default the gravity to match it so a
            // popover "anchored left" also grows left without restating it.
            placement.gravity = gravity_for_anchor(placement.anchor);
        }
        if let Some(offset) = attrs.get("offset-x").and_then(|v| v.parse::<i32>().ok()) {
            placement.offset_x = offset;
        }
        if let Some(offset) = attrs.get("offset-y").and_then(|v| v.parse::<i32>().ok()) {
            placement.offset_y = offset;
        }
        if let Some(grab) = attrs.get("grab").and_then(|v| parse_grab(v)) {
            placement.grab = grab;
        }
        if let Some(adjust) = attrs.get("constrain") {
            placement.constraint_adjustment = parse_constraint(adjust);
        }

        placement
    }
}

fn parse_anchor(value: &str) -> Option<PopoverAnchor> {
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "center" => PopoverAnchor::Center,
        "top" => PopoverAnchor::Top,
        "bottom" => PopoverAnchor::Bottom,
        "left" => PopoverAnchor::Left,
        "right" => PopoverAnchor::Right,
        "top-left" | "top_left" => PopoverAnchor::TopLeft,
        "top-right" | "top_right" => PopoverAnchor::TopRight,
        "bottom-left" | "bottom_left" => PopoverAnchor::BottomLeft,
        "bottom-right" | "bottom_right" => PopoverAnchor::BottomRight,
        _ => return None,
    })
}

fn parse_gravity(value: &str) -> Option<PopoverGravity> {
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "center" => PopoverGravity::Center,
        "top" => PopoverGravity::Top,
        "bottom" => PopoverGravity::Bottom,
        "left" => PopoverGravity::Left,
        "right" => PopoverGravity::Right,
        "top-left" | "top_left" => PopoverGravity::TopLeft,
        "top-right" | "top_right" => PopoverGravity::TopRight,
        "bottom-left" | "bottom_left" => PopoverGravity::BottomLeft,
        "bottom-right" | "bottom_right" => PopoverGravity::BottomRight,
        _ => return None,
    })
}

fn gravity_for_anchor(anchor: PopoverAnchor) -> PopoverGravity {
    match anchor {
        PopoverAnchor::Center => PopoverGravity::Center,
        PopoverAnchor::Top => PopoverGravity::Top,
        PopoverAnchor::Bottom => PopoverGravity::Bottom,
        PopoverAnchor::Left => PopoverGravity::Left,
        PopoverAnchor::Right => PopoverGravity::Right,
        PopoverAnchor::TopLeft => PopoverGravity::TopLeft,
        PopoverAnchor::TopRight => PopoverGravity::TopRight,
        PopoverAnchor::BottomLeft => PopoverGravity::BottomLeft,
        PopoverAnchor::BottomRight => PopoverGravity::BottomRight,
    }
}

fn parse_grab(value: &str) -> Option<PopoverGrab> {
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "hover" | "none" => PopoverGrab::Hover,
        "click" | "grab" => PopoverGrab::Click,
        _ => return None,
    })
}

/// Parse a space/comma separated constraint list, e.g. `"flip-y slide-x"`.
/// An empty or `"none"` list disables all adjustments.
fn parse_constraint(value: &str) -> PopoverConstraintAdjustment {
    let mut adjust = PopoverConstraintAdjustment {
        flip_x: false,
        flip_y: false,
        slide_x: false,
        slide_y: false,
        resize_x: false,
        resize_y: false,
    };
    for token in value.split([' ', ',']).filter(|t| !t.is_empty()) {
        match token.trim().to_ascii_lowercase().as_str() {
            "none" => {
                return PopoverConstraintAdjustment {
                    flip_x: false,
                    flip_y: false,
                    slide_x: false,
                    slide_y: false,
                    resize_x: false,
                    resize_y: false,
                };
            }
            "flip" => {
                adjust.flip_x = true;
                adjust.flip_y = true;
            }
            "flip-x" | "flip_x" => adjust.flip_x = true,
            "flip-y" | "flip_y" => adjust.flip_y = true,
            "slide" => {
                adjust.slide_x = true;
                adjust.slide_y = true;
            }
            "slide-x" | "slide_x" => adjust.slide_x = true,
            "slide-y" | "slide_y" => adjust.slide_y = true,
            "resize" => {
                adjust.resize_x = true;
                adjust.resize_y = true;
            }
            "resize-x" | "resize_x" => adjust.resize_x = true,
            "resize-y" | "resize_y" => adjust.resize_y = true,
            _ => {}
        }
    }
    adjust
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_with(attrs: &[(&str, &str)]) -> WidgetNode {
        let mut node = WidgetNode::new("popover");
        for (k, v) in attrs {
            node.attributes.insert((*k).to_string(), (*v).to_string());
        }
        node
    }

    #[test]
    fn defaults_are_menu_friendly() {
        let placement = PopoverPlacement::from_node(&node_with(&[]));
        assert_eq!(placement.anchor, PopoverAnchor::Bottom);
        assert_eq!(placement.gravity, PopoverGravity::Bottom);
        assert_eq!(placement.grab, PopoverGrab::Hover);
        assert!(placement.constraint_adjustment.flip_y);
        assert!(placement.constraint_adjustment.slide_x);
        assert_eq!(placement.offset_x, 0);
        assert_eq!(placement.offset_y, 0);
    }

    #[test]
    fn parses_explicit_placement() {
        let placement = PopoverPlacement::from_node(&node_with(&[
            ("anchor", "top"),
            ("gravity", "top"),
            ("offset-x", "-12"),
            ("offset-y", "4"),
            ("grab", "click"),
        ]));
        assert_eq!(placement.anchor, PopoverAnchor::Top);
        assert_eq!(placement.gravity, PopoverGravity::Top);
        assert_eq!(placement.offset_x, -12);
        assert_eq!(placement.offset_y, 4);
        assert_eq!(placement.grab, PopoverGrab::Click);
    }

    #[test]
    fn gravity_defaults_to_anchor_direction() {
        let placement = PopoverPlacement::from_node(&node_with(&[("anchor", "right")]));
        assert_eq!(placement.anchor, PopoverAnchor::Right);
        assert_eq!(placement.gravity, PopoverGravity::Right);
    }

    #[test]
    fn constraint_list_parses_axes() {
        let placement = PopoverPlacement::from_node(&node_with(&[("constrain", "flip-y slide-x")]));
        let c = placement.constraint_adjustment;
        assert!(c.flip_y && c.slide_x);
        assert!(!c.flip_x && !c.slide_y && !c.resize_x && !c.resize_y);
    }

    #[test]
    fn constraint_none_disables_all() {
        let placement = PopoverPlacement::from_node(&node_with(&[("constrain", "none")]));
        let c = placement.constraint_adjustment;
        assert!(!c.flip_x && !c.flip_y && !c.slide_x && !c.slide_y);
    }

    #[test]
    fn unknown_values_fall_back_to_defaults() {
        let placement = PopoverPlacement::from_node(&node_with(&[
            ("anchor", "sideways"),
            ("grab", "telepathy"),
        ]));
        assert_eq!(placement.anchor, PopoverAnchor::Bottom);
        assert_eq!(placement.grab, PopoverGrab::Hover);
    }
}
