//! Renderer-side animation primitives.
//!
//! This module is the home for everything that actually drives visual change
//! between frames: easing curves, value interpolation, transition controllers,
//! affine transforms, box shadows, and `@keyframes` playback.
//!
//! Phase 12 moves the interpolation and keyframe math here so the shell can
//! treat renderer animation playback as a reusable primitive instead of owning
//! timing logic itself.

pub mod box_shadow;
pub mod easing;
pub mod interpolate;
pub mod keyframes;
pub mod transform;
pub mod transition;

pub use easing::{Easing, apply_easing};
pub use interpolate::Interpolate;
pub use transform::Transform2D;
