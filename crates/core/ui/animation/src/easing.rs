//! Easing curves for transitions and keyframe animations.
//!
//! Mirrors the CSS `transition-timing-function` / `animation-timing-function`
//! keyword set, plus `cubic-bezier(x1, y1, x2, y2)`. The bezier variant is
//! how theme-driven motion tokens are realized — authors write
//! `transition-timing-function: var(--animation-curves-bezier-standard)`, the token
//! engine substitutes `cubic-bezier(0.2, 0, 0, 1)`, and the parser produces
//! a `CubicBezier` variant that this module knows how to evaluate.

use mesh_core_elements::{StepPosition, TransitionEasing};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// (x1, y1, x2, y2) — the two control points of a cubic Bézier from
    /// (0,0) to (1,1). Same convention as CSS `cubic-bezier()`.
    CubicBezier(f32, f32, f32, f32),
    /// `steps(n, <position>)` — a discrete step function with `n` intervals.
    Steps(u32, StepPosition),
}

impl From<TransitionEasing> for Easing {
    fn from(value: TransitionEasing) -> Self {
        match value {
            TransitionEasing::Linear => Easing::Linear,
            TransitionEasing::Ease => Easing::Ease,
            TransitionEasing::EaseIn => Easing::EaseIn,
            TransitionEasing::EaseOut => Easing::EaseOut,
            TransitionEasing::EaseInOut => Easing::EaseInOut,
            TransitionEasing::CubicBezier(x1, y1, x2, y2) => Easing::CubicBezier(x1, y1, x2, y2),
            TransitionEasing::Steps(count, position) => Easing::Steps(count, position),
        }
    }
}

pub fn apply_easing(easing: Easing, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => t,
        Easing::Ease => ease_in_out_cubic(t),
        Easing::EaseIn => ease_in_cubic(t),
        Easing::EaseOut => ease_out_cubic(t),
        Easing::EaseInOut => ease_in_out_cubic(t),
        Easing::CubicBezier(x1, y1, x2, y2) => cubic_bezier_eval(t, x1, y1, x2, y2),
        Easing::Steps(count, position) => steps_eval(t, count, position),
    }
}

/// Evaluate a CSS `steps(n, <position>)` timing function at progress `t`.
///
/// The output is a staircase: the current interval index divided by the number
/// of jumps. `jump-start`/`jump-both` add a leading jump; `jump-none` drops a
/// jump so both 0 and 1 are reachable. Matches the CSS Easing Functions spec.
fn steps_eval(t: f32, count: u32, position: StepPosition) -> f32 {
    let steps = count.max(1) as f32;
    let mut current = (t * steps).floor();

    if matches!(position, StepPosition::JumpStart | StepPosition::JumpBoth) {
        current += 1.0;
    }

    let jumps = match position {
        // `steps(1, jump-none)` is degenerate; guard the divisor.
        StepPosition::JumpNone => (steps - 1.0).max(1.0),
        StepPosition::JumpBoth => steps + 1.0,
        StepPosition::JumpStart | StepPosition::JumpEnd => steps,
    };

    (current / jumps).clamp(0.0, 1.0)
}

fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Evaluate a CSS-style cubic Bézier curve at progress `t in [0, 1]`.
///
/// The CSS `cubic-bezier(x1, y1, x2, y2)` curve is parametric:
///   x(s) = 3(1-s)^2 s x1 + 3(1-s) s^2 x2 + s^3
///   y(s) = 3(1-s)^2 s y1 + 3(1-s) s^2 y2 + s^3
/// where `s` is the curve parameter. We are given `x` (the eased input) and
/// must find `y`. Solve `x(s) = t` for `s`, then evaluate `y(s)`.
///
/// Strategy: a few Newton-Raphson iterations from a linear seed; fall back to
/// bisection if the derivative collapses. This is the same algorithm Chromium
/// and Firefox use; ~4 iterations is enough for sub-pixel accuracy.
fn cubic_bezier_eval(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }

    let s = solve_curve_x(t, x1, x2);
    bezier_axis(s, y1, y2)
}

fn bezier_axis(s: f32, c1: f32, c2: f32) -> f32 {
    // Cubic Bézier with endpoints at 0 and 1, control coordinates c1, c2.
    let one_minus = 1.0 - s;
    3.0 * one_minus * one_minus * s * c1 + 3.0 * one_minus * s * s * c2 + s * s * s
}

fn bezier_axis_derivative(s: f32, c1: f32, c2: f32) -> f32 {
    let one_minus = 1.0 - s;
    3.0 * one_minus * one_minus * c1 + 6.0 * one_minus * s * (c2 - c1) + 3.0 * s * s * (1.0 - c2)
}

fn solve_curve_x(target_x: f32, x1: f32, x2: f32) -> f32 {
    const NEWTON_ITERATIONS: u32 = 4;
    const NEWTON_EPSILON: f32 = 1.0e-3;

    let mut s = target_x;
    for _ in 0..NEWTON_ITERATIONS {
        let x = bezier_axis(s, x1, x2) - target_x;
        if x.abs() < NEWTON_EPSILON {
            return s;
        }
        let dx = bezier_axis_derivative(s, x1, x2);
        if dx.abs() < 1.0e-6 {
            break;
        }
        s -= x / dx;
    }

    let mut lo = 0.0f32;
    let mut hi = 1.0f32;
    let mut s = target_x;
    for _ in 0..32 {
        let x = bezier_axis(s, x1, x2);
        if (x - target_x).abs() < NEWTON_EPSILON {
            return s;
        }
        if x < target_x {
            lo = s;
        } else {
            hi = s;
        }
        s = (lo + hi) * 0.5;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_easings_pass_through_endpoints() {
        for easing in [
            Easing::Linear,
            Easing::Ease,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            assert!(apply_easing(easing, 0.0).abs() < 1e-4);
            assert!((apply_easing(easing, 1.0) - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn cubic_bezier_linear_matches_linear() {
        // (0,0,1,1) is the identity curve.
        let curve = Easing::CubicBezier(0.0, 0.0, 1.0, 1.0);
        for t in [0.1, 0.25, 0.5, 0.75, 0.9] {
            assert!((apply_easing(curve, t) - t).abs() < 1e-2);
        }
    }

    #[test]
    fn steps_jump_end_holds_start_and_reaches_end() {
        let curve = Easing::Steps(4, StepPosition::JumpEnd);
        assert_eq!(apply_easing(curve, 0.0), 0.0);
        assert_eq!(apply_easing(curve, 0.1), 0.0); // still in first interval
        assert_eq!(apply_easing(curve, 0.3), 0.25); // second interval
        assert_eq!(apply_easing(curve, 0.6), 0.5);
        assert_eq!(apply_easing(curve, 1.0), 1.0);
    }

    #[test]
    fn steps_jump_start_jumps_immediately() {
        let curve = Easing::Steps(4, StepPosition::JumpStart);
        assert_eq!(apply_easing(curve, 0.0), 0.25); // leading jump
        assert_eq!(apply_easing(curve, 0.3), 0.5);
        assert_eq!(apply_easing(curve, 1.0), 1.0);
    }

    #[test]
    fn steps_jump_none_reaches_both_ends() {
        let curve = Easing::Steps(4, StepPosition::JumpNone);
        assert_eq!(apply_easing(curve, 0.0), 0.0);
        assert!((apply_easing(curve, 0.3) - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(apply_easing(curve, 0.8), 1.0); // last stop reached before t=1
    }

    #[test]
    fn steps_jump_both_holds_neither_end() {
        let curve = Easing::Steps(4, StepPosition::JumpBoth);
        assert!((apply_easing(curve, 0.0) - 0.2).abs() < 1e-6); // 1/5
        assert!((apply_easing(curve, 0.9) - 0.8).abs() < 1e-6); // 4/5
        assert_eq!(apply_easing(curve, 1.0), 1.0);
    }

    #[test]
    fn cubic_bezier_md_standard_curve() {
        // Material standard easing: cubic-bezier(0.2, 0, 0, 1).
        let curve = Easing::CubicBezier(0.2, 0.0, 0.0, 1.0);
        // It accelerates fast then decelerates -> mid-progress should be well past 0.5.
        let mid = apply_easing(curve, 0.5);
        assert!(mid > 0.7, "expected eased mid > 0.7, got {mid}");
    }
}
