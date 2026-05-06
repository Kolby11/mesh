//! Generic interpolation trait and primitive implementations.
//!
//! Anything that can be lerped during a transition or between keyframe stops
//! implements `Interpolate`. The trait works for primitives (`f32`, `Color`,
//! `Corners`, `Edges`) and for composite property bundles (an animatable
//! style snapshot).

use mesh_core_elements::{Corners, Edges, Transform2D, style::Color};

pub trait Interpolate {
    /// Linearly interpolate between `self` and `other` by `progress` in `[0, 1]`.
    /// `progress` is the eased value, not raw time — easing is applied upstream.
    fn lerp(self, other: Self, progress: f32) -> Self;
}

impl Interpolate for f32 {
    fn lerp(self, other: Self, progress: f32) -> Self {
        self + (other - self) * progress
    }
}

impl Interpolate for Color {
    fn lerp(self, other: Self, progress: f32) -> Self {
        Color {
            r: f32::lerp(self.r as f32, other.r as f32, progress).round() as u8,
            g: f32::lerp(self.g as f32, other.g as f32, progress).round() as u8,
            b: f32::lerp(self.b as f32, other.b as f32, progress).round() as u8,
            a: f32::lerp(self.a as f32, other.a as f32, progress).round() as u8,
        }
    }
}

impl Interpolate for Corners {
    fn lerp(self, other: Self, progress: f32) -> Self {
        Corners {
            top_left: self.top_left.lerp(other.top_left, progress),
            top_right: self.top_right.lerp(other.top_right, progress),
            bottom_right: self.bottom_right.lerp(other.bottom_right, progress),
            bottom_left: self.bottom_left.lerp(other.bottom_left, progress),
        }
    }
}

impl Interpolate for Edges {
    fn lerp(self, other: Self, progress: f32) -> Self {
        Edges {
            top: self.top.lerp(other.top, progress),
            right: self.right.lerp(other.right, progress),
            bottom: self.bottom.lerp(other.bottom, progress),
            left: self.left.lerp(other.left, progress),
        }
    }
}

impl Interpolate for Transform2D {
    fn lerp(self, other: Self, progress: f32) -> Self {
        Transform2D {
            translate_x: self.translate_x.lerp(other.translate_x, progress),
            translate_y: self.translate_y.lerp(other.translate_y, progress),
            scale_x: self.scale_x.lerp(other.scale_x, progress),
            scale_y: self.scale_y.lerp(other.scale_y, progress),
            // Take the shortest angular path so a hover-out reverses the
            // way it came rather than spinning the long way around.
            rotation: lerp_angle(self.rotation, other.rotation, progress),
        }
    }
}

fn lerp_angle(from: f32, to: f32, progress: f32) -> f32 {
    let diff = (to - from).rem_euclid(std::f32::consts::TAU);
    let shortest = if diff > std::f32::consts::PI {
        diff - std::f32::consts::TAU
    } else {
        diff
    };
    from + shortest * progress
}
