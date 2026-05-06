//! Transition controller — drives interpolation of style properties between
//! the previous frame's value and the current desired value.
//!
//! ## Migration plan
//!
//! Today the working transition implementation lives in
//! `mesh-core-shell::shell::component::animation`. It interpolates four
//! visual properties (`border-radius`, `opacity`, `background-color`,
//! `color`) on every render pass. The plan is to move that logic here and
//! widen the animatable property set to also include:
//!
//! - layout box: `width`, `height`, `padding`, `margin`
//! - transform: `scale`, `rotate`, `translate` (see `transform.rs`)
//! - effects: `box-shadow` (see `box_shadow.rs`)
//! - existing: `border-radius`, `border-color`, `border-width`, `opacity`,
//!   `background-color`, `color`
//!
//! ## Shape (target API)
//!
//! ```ignore
//! let mut animator = TransitionAnimator::new();
//! animator.apply(&mut tree, now);  // mutates computed_style in-flight
//! let dirty = animator.has_active();
//! ```
//!
//! Internally the animator owns a `HashMap<NodeKey, ActiveTransition>` that
//! survives across frames so re-targeting (e.g. mid-flight hover-out)
//! reverses smoothly instead of snapping.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mesh_core_elements::{Corners, TransitionStyle, WidgetNode, style::Color};

use super::easing::{Easing, apply_easing};
use super::interpolate::Interpolate;

/// Bundle of every property that can be transitioned. Mirrors the subset of
/// `ComputedStyle` we know how to interpolate.
///
/// TODO: extend with `width`, `height`, `padding`, `margin`, `border_width`,
/// `transform`, `box_shadow` when the corresponding backend support lands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimatableStyle {
    pub border_radius: Corners,
    pub opacity: f32,
    pub background_color: Color,
    pub color: Color,
    // pub border_color: Color,
    // pub border_width: Edges,
    // pub width: f32,
    // pub height: f32,
    // pub padding: Edges,
    // pub margin: Edges,
    // pub transform: Transform2D,
    // pub box_shadow: BoxShadow,
}

impl AnimatableStyle {
    pub fn from_node(_node: &WidgetNode) -> Self {
        // TODO: copy fields out of node.computed_style.
        unimplemented!("AnimatableStyle::from_node — port from shell::component::animation")
    }

    pub fn apply_to_node(self, _node: &mut WidgetNode) {
        // TODO: write fields back into node.computed_style.
        unimplemented!("AnimatableStyle::apply_to_node")
    }
}

impl Interpolate for AnimatableStyle {
    fn lerp(self, _other: Self, _progress: f32) -> Self {
        // TODO: per-field lerp once the struct stabilizes.
        unimplemented!("AnimatableStyle::lerp")
    }
}

/// One in-flight transition for a single widget node.
#[derive(Debug, Clone)]
pub struct ActiveTransition {
    pub from: AnimatableStyle,
    pub to: AnimatableStyle,
    pub started_at: Instant,
    pub duration: Duration,
    pub delay: Duration,
    pub easing: Easing,
    pub source: TransitionStyle,
}

impl ActiveTransition {
    pub fn current(&self, now: Instant) -> AnimatableStyle {
        if self.duration.is_zero() {
            return self.to;
        }
        let elapsed = now.saturating_duration_since(self.started_at);
        if elapsed < self.delay {
            return self.from;
        }
        let active = elapsed - self.delay;
        let raw = (active.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        self.from.lerp(self.to, apply_easing(self.easing, raw))
    }

    pub fn finished(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started_at) >= self.delay + self.duration
    }
}

/// Per-component transition state. The animator is owned by the renderable
/// component (one per surface) and persists across frames.
#[derive(Debug, Default)]
#[allow(dead_code)] // skeleton: fields are wired up when `apply` is implemented.
pub struct TransitionAnimator {
    active: HashMap<String, ActiveTransition>,
    has_active: bool,
}

impl TransitionAnimator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Walk `tree`, compare each node's desired style against the displayed
    /// style from the prior frame, start/retarget transitions where needed,
    /// and write the interpolated style back into the node before paint.
    pub fn apply(&mut self, _tree: &mut WidgetNode, _now: Instant) {
        // TODO: port apply_style_animations from shell::component::animation.
        // Steps:
        //   1. collect previous frame's AnimatableStyle by node key
        //   2. recurse: for each keyed node, compute desired style
        //   3. if transition declared and previous != desired -> start/retarget
        //   4. write current(now) back to node
        //   5. set has_active if any animation not finished
        unimplemented!("TransitionAnimator::apply")
    }

    pub fn has_active(&self) -> bool {
        self.has_active
    }
}
