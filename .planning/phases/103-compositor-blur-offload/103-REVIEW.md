---
phase: 103-compositor-blur-offload
reviewed: 2026-06-17T00:00:00Z
depth: standard
files_reviewed: 10
files_reviewed_list:
  - crates/core/frontend/render/src/surface/painter.rs
  - crates/core/frontend/render/src/surface/painter/tests.rs
  - crates/core/frontend/render/src/surface/painter/tree.rs
  - crates/core/presentation/Cargo.toml
  - crates/core/presentation/src/lib.rs
  - crates/core/presentation/src/wayland_surface/backend.rs
  - crates/core/presentation/src/wayland_surface/handlers.rs
  - crates/core/presentation/src/wayland_surface/mod.rs
  - crates/core/presentation/src/wayland_surface/state.rs
  - crates/core/shell/src/shell/runtime/render.rs
findings:
  critical: 2
  warning: 2
  info: 1
  total: 5
status: issues_found
---

# Phase 103: Code Review Report

**Reviewed:** 2026-06-17
**Depth:** standard
**Files Reviewed:** 10
**Status:** issues_found

## Summary

Phase 103 implements compositor blur offload via `org_kde_kwin_blur`. The
Wayland protocol binding, blur-object creation, and the no-op replacement of
the CPU blur path are structurally sound. The blur region is correctly
propagated from the display list through to `kde_blur.set_region()`.

Two blockers were found:

1. The compositor's blur region is never cleared when `backdrop-filter` is
   removed at runtime. Once any blur region has been committed to the
   compositor it persists until explicitly revoked — the implementation only
   sends `set_region(Some(...)) + commit()` but never sends the matching
   `set_region(None) + commit()` clearing call.

2. `compute_blur_region` casts `f32` layout coordinates to `u32` without
   handling negative values. Rust saturates negative `f32 as u32` to 0, which
   silently shifts the blur region origin to (0,0) for any node that is
   partially off-screen or scroll-translated into negative logical space.

---

## Critical Issues

### CR-01: Blur region never cleared from compositor when backdrop-filter is removed

**File:** `crates/core/presentation/src/wayland_surface/backend.rs:876`

**Issue:** In `present_with_damage`, the `kde_blur` region is only emitted
when `entry.blur_region` is `Some(...)`. When no display-list nodes carry
`backdrop_filter`, `compute_blur_region` returns `None`, `update_blur_region`
stores `None` in `entry.blur_region`, and the whole `if let Some(region_rect)`
block is skipped — meaning neither `set_region(None)` nor `commit()` is ever
sent. Once the compositor has received a non-empty region, it continues
blurring indefinitely even after the CSS property is removed from the surface.

The public-facing doc comment in `lib.rs:153` compounds this by stating
"`None` means skip blur protocol calls for this frame", which documents the
incorrect behaviour as intentional.

**Fix:** Track whether a blur region was previously committed and send a
clearing call whenever the new value is `None` but a previous commit has been
sent. The minimal fix is to add a `prev_blur_region: Option<DamageRect>` field
to `SurfaceEntry` (or a simple `bool blur_committed`) and only skip the
`set_region(None) + commit()` call when neither the current nor the previous
frame had a region:

```rust
// In SurfaceEntry (backend.rs):
pub(super) kde_blur: Option<OrgKdeKwinBlur>,
pub(super) blur_region: Option<DamageRect>,
pub(super) blur_committed: bool,   // NEW: true after any non-None commit

// In present_with_damage (backend.rs ~line 875):
if let Some(ref kde_blur) = entry.kde_blur {
    match entry.blur_region {
        Some(region_rect) => {
            if let Ok(region) = Region::new(&state.compositor_state) {
                region.add(
                    region_rect.x as i32,
                    region_rect.y as i32,
                    region_rect.width as i32,
                    region_rect.height as i32,
                );
                kde_blur.set_region(Some(region.wl_region()));
                kde_blur.commit();
                entry.blur_committed = true;
            }
        }
        None if entry.blur_committed => {
            // Clear the compositor's blur region.
            kde_blur.set_region(None);
            kde_blur.commit();
            entry.blur_committed = false;
        }
        None => {}
    }
}
```

Also update the doc comment in `lib.rs` to reflect that `None` means "clear
blur" not "skip".

---

### CR-02: Negative layout coordinates silently snap blur region to origin

**File:** `crates/core/shell/src/shell/runtime/render.rs:442`

**Issue:** `compute_blur_region` converts `cmd.node.layout.x` and
`cmd.node.layout.y` (both `f32`) to `u32` with a bare `as u32` cast. Rust
saturates negative `f32` values to 0 when casting to an unsigned integer (as
of Rust 1.45). A node with `backdrop-filter` that is partially scrolled off
the top or left edge of a surface will have negative `x` or `y` layout
coordinates. The saturating cast changes the blur rect origin from the true
position to (0, 0), making the union rect start at the surface corner instead
of the correct (possibly negative) position, which produces a blur region that
is larger and incorrectly positioned.

```rust
// Current — silently clips negatives to 0:
let rect = DamageRect {
    x: cmd.node.layout.x as u32,          // negative → 0
    y: cmd.node.layout.y as u32,          // negative → 0
    width: (cmd.node.layout.width as u32).max(1),
    height: (cmd.node.layout.height as u32).max(1),
};

// Fix — clamp negative origins and shrink dimensions to compensate:
let raw_x = cmd.node.layout.x;
let raw_y = cmd.node.layout.y;
let x = raw_x.max(0.0) as u32;
let y = raw_y.max(0.0) as u32;
// Shrink width/height by any clipped-off leading edge
let width = ((cmd.node.layout.width + raw_x.min(0.0)).max(0.0) as u32).max(1);
let height = ((cmd.node.layout.height + raw_y.min(0.0)).max(0.0) as u32).max(1);
let rect = DamageRect { x, y, width, height };
```

---

## Warnings

### WR-01: `update_blur_region` called outside the `if visible` guard for hidden surfaces

**File:** `crates/core/shell/src/shell/runtime/render.rs:287`

**Issue:** `compute_opaque_rect_for_root` is wrapped in `if visible { ... }`
but `compute_blur_region` and `update_blur_region` are also inside the same
`if visible` block (lines 287–300). However when the surface is hidden the
render loop `break`s early at line 130 before reaching this block, so the blur
region stored in `entry.blur_region` is never cleared to `None` on hide. If a
surface is shown (with blur), hidden, and then shown again with a changed
layout, the stale `blur_region` persists in `entry` until `update_blur_region`
is called again on the next visible frame. In practice this is benign because
the visible-frame call always overwrites the value, but if a frame is skipped
(e.g., waiting for compositor configure), the stale value may be committed.

**Fix:** Explicitly clear the blur region when hiding a surface. The easiest
point is alongside the existing `entry.hide()` call in
`present_with_damage` (backend.rs ~line 800–808):

```rust
if let Some(entry) = self.state.surfaces.get_mut(surface_id)
    && entry.configured
{
    // Clear compositor blur before hiding (BLUR-04).
    if let Some(ref kde_blur) = entry.kde_blur
        && entry.blur_committed
    {
        kde_blur.set_region(None);
        kde_blur.commit();
        entry.blur_committed = false;
    }
    entry.blur_region = None;
    entry.hide();
}
```

---

### WR-02: No unit test for `compute_blur_region`

**File:** `crates/core/shell/src/shell/runtime/render.rs:435`

**Issue:** `compute_blur_region` is the only new logic function introduced in
this phase and has no tests. It performs coordinate math (`as u32` casts,
saturating union of rects) that is easy to get wrong at boundaries (negative
coords, zero-sized nodes, single vs. multiple blur nodes). The current tests
added in `painter/tests.rs` cover the no-op CPU path
(`painter_primitive_box_rounded_shadow_and_filters_emit_effect_classes`) but
nothing exercises the region computation.

**Fix:** Add a `#[cfg(test)]` module at the bottom of `render.rs` with at
least:
- single backdrop-filter node → correct `DamageRect`
- two disjoint blur nodes → union covers both
- node with negative x/y → origin clamped, dimensions shrunk
- no blur nodes → `None` returned

---

## Info

### IN-01: Misleading doc comment on `update_blur_region`

**File:** `crates/core/presentation/src/lib.rs:153`

**Issue:** The public doc comment reads "Pass `None` to skip blur protocol
calls for this frame (BLUR-04)." The intended semantic should be "clear the
compositor's blur region", not "skip". The current wording will mislead future
callers into thinking `None` is a no-cost hint rather than an active clearing
operation. This is closely related to CR-01 but is a separate documentation
defect that should be corrected even if the implementation fix changes the
actual behaviour.

**Fix:**
```rust
/// Set the logical-coordinate blur region for a surface.
/// Only meaningful on Wayland backends with `org_kde_kwin_blur` support.
/// Pass `None` to clear any previously committed blur region from the
/// compositor. No protocol calls are emitted if no blur region has ever
/// been set for this surface.
pub fn update_blur_region(
    &mut self,
    surface_id: &str,
    blur_region: Option<DamageRect>,
) {
```

---

_Reviewed: 2026-06-17_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
