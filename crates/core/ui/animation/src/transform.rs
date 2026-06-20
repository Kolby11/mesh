//! Renderer-side helpers around `Transform2D`.
//!
//! The struct itself lives in `mesh-core-elements::style::Transform2D` so the
//! parser, layout, and `ComputedStyle` can all reach it without depending on
//! the renderer crate. This module re-exports it and adds paint-time helpers
//! (matrix composition, point projection) that only the renderer needs.
//!
//! ## Today's painter coverage
//!
//! - `translate(...)` is applied at paint time and inherited by children:
//!   descendants render shifted along with the transformed ancestor.
//!   Hit-testing inverts the cumulative translation.
//! - `scale(...)` parses, animates, and is applied to retained paint bounds.
//! - `rotate(...)` parses, animates, and propagates through the style pipeline,
//!   but is still treated as identity by the painter.
//!
//! ## Path to full transform support
//!
//! Extend the Skia-backed painter to draw transformed subtrees through a
//! saved canvas layer for rotations. Hit-test should invert the same matrix.

pub use mesh_core_elements::Transform2D;

/// True for transforms the current painter can render correctly.
/// Rotation is now painted via software blit so all transforms are paintable.
pub fn is_paintable(_transform: &Transform2D) -> bool {
    true
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
