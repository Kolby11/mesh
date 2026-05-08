//! Renderer-side helpers around `Transform2D`.
//!
//! The struct itself lives in `mesh-core-elements::style::Transform2D` so the
//! parser, layout, and `ComputedStyle` can all reach it without depending on
//! the renderer crate. This module re-exports it and adds paint-time helpers
//! (matrix composition, point projection) that only the renderer needs.
//!
//! ## Today's painter coverage
//!
//! - `translate(...)` is fully applied at paint time and inherited by
//!   children: descendants render shifted along with the transformed
//!   ancestor. Hit-testing inverts the cumulative translation.
//! - `scale(...)` and `rotate(...)` parse, animate, and propagate through
//!   the style/transition pipeline, but the painter currently treats them as
//!   identity because the rasterizer is axis-aligned-only.
//!
//! ## Path to full transform support
//!
//! Route the painter through `tiny_skia` (already a transitive dep via
//! `resvg`) for any subtree where `transform.is_identity()` is false but
//! goes beyond a pure translate. Build a `tiny_skia::Transform` from the
//! decomposed components and feed it to the path painter. Hit-test inverts
//! the same matrix.

pub use mesh_core_elements::Transform2D;

/// True for transforms the current painter can render correctly. Subtrees
/// rooted at any node returning `false` here will need the tiny_skia path
/// once it lands.
pub fn is_paintable(transform: &Transform2D) -> bool {
    let nearly_one = |v: f32| (v - 1.0).abs() < f32::EPSILON;
    let nearly_zero = |v: f32| v.abs() < f32::EPSILON;
    nearly_one(transform.scale_x)
        && nearly_one(transform.scale_y)
        && nearly_zero(transform.rotation)
}

/// Compose two transforms as if `outer` were applied after `inner` in CSS
/// (inner is the child, outer the ancestor). Translation accumulates; scale
/// multiplies; rotation adds. Sufficient for axis-aligned + translate paths.
pub fn compose(outer: Transform2D, inner: Transform2D) -> Transform2D {
    Transform2D {
        translate_x: outer.translate_x + inner.translate_x * outer.scale_x,
        translate_y: outer.translate_y + inner.translate_y * outer.scale_y,
        scale_x: outer.scale_x * inner.scale_x,
        scale_y: outer.scale_y * inner.scale_y,
        rotation: outer.rotation + inner.rotation,
    }
}
