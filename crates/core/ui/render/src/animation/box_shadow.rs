//! `box-shadow` parsing, interpolation, and rasterization.
//!
//! Not currently part of `ComputedStyle`. Lands as:
//!
//! 1. Add `box_shadow: BoxShadow` field to `ComputedStyle`.
//! 2. Add the `box-shadow` parser in `mesh-core-elements::style`.
//! 3. Implement `Interpolate` for `BoxShadow` (per-component lerp).
//! 4. Paint pass: blur a rounded-rect mask at `(offset_x, offset_y)` with
//!    `blur_radius` into the buffer underneath the node, before drawing the
//!    node body. `tiny_skia` provides Gaussian blur via `Pixmap::filter`.
//!
//! Multiple shadows per element (CSS allows comma-separated list) is a v2
//! concern — start with a single shadow.

use mesh_core_elements::style::Color;

use super::interpolate::Interpolate;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub spread_radius: f32,
    pub color: Color,
    pub inset: bool,
}

impl BoxShadow {
    pub const NONE: Self = Self {
        offset_x: 0.0,
        offset_y: 0.0,
        blur_radius: 0.0,
        spread_radius: 0.0,
        color: Color::TRANSPARENT,
        inset: false,
    };

    pub fn is_none(self) -> bool {
        self.color.a == 0
            && self.offset_x == 0.0
            && self.offset_y == 0.0
            && self.blur_radius == 0.0
            && self.spread_radius == 0.0
    }
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self::NONE
    }
}

impl Interpolate for BoxShadow {
    fn lerp(self, other: Self, progress: f32) -> Self {
        Self {
            offset_x: self.offset_x.lerp(other.offset_x, progress),
            offset_y: self.offset_y.lerp(other.offset_y, progress),
            blur_radius: self.blur_radius.lerp(other.blur_radius, progress),
            spread_radius: self.spread_radius.lerp(other.spread_radius, progress),
            color: self.color.lerp(other.color, progress),
            // `inset` doesn't interpolate; snap at midpoint.
            inset: if progress < 0.5 { self.inset } else { other.inset },
        }
    }
}

/// Parse the CSS `box-shadow` value. Accepts the `[inset] <ox> <oy> <blur>?
/// <spread>? <color>` form. Multi-shadow lists are not yet supported.
pub fn parse_box_shadow(_value: &str) -> BoxShadow {
    // TODO: lex tokens, classify lengths/colors, build BoxShadow.
    unimplemented!("animation::box_shadow::parse_box_shadow")
}
