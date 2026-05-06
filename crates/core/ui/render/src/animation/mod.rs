//! Renderer-side animation primitives.
//!
//! This module is the home for everything that actually drives visual change
//! between frames: easing curves, value interpolation, transition controllers,
//! affine transforms, box shadows, and `@keyframes` playback.
//!
//! Today the only working piece is the `transition`-based interpolation that
//! lives in `mesh-core-shell` (`shell/component/animation.rs`). That logic
//! will migrate here so the shell only needs to feed the animator a tree and
//! a timestamp; it will not own the math.
//!
//! The submodules below are skeletons. Each one carries a short outline of
//! what it has to do and the integration points it touches.

pub mod box_shadow;
pub mod easing;
pub mod interpolate;
pub mod keyframes;
pub mod transform;
pub mod transition;

pub use easing::{Easing, apply_easing};
pub use interpolate::Interpolate;
pub use transform::Transform2D;
