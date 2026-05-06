//! `@keyframes` rules and `animation: ...` shorthand playback.
//!
//! Transitions only animate between two endpoints (current value, desired
//! value). Keyframe animations play through a sequence of named stops on a
//! timer, optionally looping or reversing. They are independent of state
//! changes — once `animation: pulse 1s infinite` is on a node, it runs until
//! removed.
//!
//! ## Wiring
//!
//! 1. Extend the `<style>` parser in `mesh-core-component::parser::styles`
//!    to recognize `@keyframes <name> { 0% { ... } 50% { ... } 100% { ... } }`
//!    and store rules on `ComponentFile`.
//! 2. Build a `KeyframeRegistry` keyed by name when the component compiles.
//! 3. Per-node, when `animation-name` resolves to a known keyframe, allocate
//!    an `ActiveKeyframeAnimation` that walks `KeyframeStop`s using elapsed
//!    time, easing, iteration count, direction, and fill mode.
//! 4. Stops contribute partial `AnimatableStyle` snapshots; missing fields
//!    fall back to the node's base computed style.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mesh_core_elements::style::{AnimationDirection, AnimationFillMode, AnimationIterationCount};

use super::easing::Easing;
use super::transition::AnimatableStyle;

/// A single `<percent>% { ... }` block from a `@keyframes` rule.
#[derive(Debug, Clone)]
pub struct KeyframeStop {
    /// Position in the timeline, normalized to `[0.0, 1.0]`.
    pub offset: f32,
    /// Style snapshot at this stop. Only properties present here participate
    /// in the animation; everything else stays at the node's base style.
    pub style: AnimatableStyle,
    /// Per-stop timing function (CSS `animation-timing-function` inside a
    /// keyframe stop overrides the animation-level easing for the segment
    /// starting at this stop).
    pub easing: Option<Easing>,
}

/// One named `@keyframes` rule, sorted by `offset`.
#[derive(Debug, Clone)]
pub struct KeyframeRule {
    pub name: String,
    pub stops: Vec<KeyframeStop>,
}

/// Lookup table built once at compile time, shared across all nodes that
/// reference an animation name.
#[derive(Debug, Default)]
pub struct KeyframeRegistry {
    rules: HashMap<String, KeyframeRule>,
}

impl KeyframeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, rule: KeyframeRule) {
        self.rules.insert(rule.name.clone(), rule);
    }

    pub fn get(&self, name: &str) -> Option<&KeyframeRule> {
        self.rules.get(name)
    }
}

/// One in-flight keyframe animation attached to a single widget node.
#[derive(Debug, Clone)]
pub struct ActiveKeyframeAnimation {
    pub rule_name: String,
    pub started_at: Instant,
    pub duration: Duration,
    pub delay: Duration,
    pub easing: Easing,
    pub iteration_count: AnimationIterationCount,
    pub direction: AnimationDirection,
    pub fill_mode: AnimationFillMode,
}

impl ActiveKeyframeAnimation {
    /// Resolve the displayed style at `now` by locating the surrounding pair
    /// of stops, computing the segment-local progress, and lerping.
    ///
    /// Returns `None` before `delay` has elapsed (unless `fill_mode` is
    /// `Backwards`/`Both`) and after the final iteration (unless
    /// `Forwards`/`Both`).
    pub fn current(
        &self,
        _registry: &KeyframeRegistry,
        _base: AnimatableStyle,
        _now: Instant,
    ) -> Option<AnimatableStyle> {
        // TODO:
        //   1. compute total elapsed minus delay
        //   2. derive iteration index + intra-iteration `t in [0,1]`
        //   3. flip `t` based on AnimationDirection
        //   4. find surrounding stops, lerp their styles using stop.easing or self.easing
        //   5. honor fill_mode for pre/post boundaries
        unimplemented!("ActiveKeyframeAnimation::current")
    }

    pub fn finished(&self, _now: Instant) -> bool {
        // TODO: respect Infinite iteration count (never finished).
        unimplemented!("ActiveKeyframeAnimation::finished")
    }
}

/// Parse a `@keyframes` rule body into a `KeyframeRule`.
pub fn parse_keyframes(_name: &str, _body: &str) -> KeyframeRule {
    // TODO: parse `0% { ... }`, `from { ... }`, `to { ... }`, `50%, 75% { ... }`.
    unimplemented!("animation::keyframes::parse_keyframes")
}
