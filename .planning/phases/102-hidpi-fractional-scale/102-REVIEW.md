---
phase: 102-hidpi-fractional-scale
reviewed: 2026-06-10T00:00:00Z
depth: standard
files_reviewed: 8
files_reviewed_list:
  - crates/core/presentation/Cargo.toml
  - crates/core/presentation/src/wayland_surface/mod.rs
  - crates/core/presentation/src/wayland_surface/state.rs
  - crates/core/presentation/src/wayland_surface/handlers.rs
  - crates/core/presentation/src/wayland_surface/backend.rs
  - crates/core/presentation/src/lib.rs
  - crates/core/shell/src/shell/mod.rs
  - crates/core/shell/src/shell/runtime/render.rs
findings:
  critical: 2
  warning: 3
  info: 2
  total: 7
status: issues_found
---

# Phase 102: Code Review Report

**Reviewed:** 2026-06-10T00:00:00Z
**Depth:** standard
**Files Reviewed:** 8
**Status:** issues_found

## Summary

Phase 102 wires compositor-provided scale factors (integer via `wl_surface::preferred_buffer_scale`, fractional via `wp_fractional_scale_v1`) and pairs them with `wp_viewporter` for non-integer ratios. The architecture is sound: layout stays in logical CSS pixels, buffer allocation uses physical dimensions (`ceil(logical × scale)`), and damage rects flow through a single scaling boundary in `attach_shm_buffer`.

However, **two BLOCKER-level bugs** were found that will cause visibly corrupt rendering on any display with scale > 1.0 (which is the entire point of this phase):

1. **Coordinate-space mismatch in SHM buffer copy:** Damage rects in logical coordinates are fed to `copy_into_shm_buffer` alongside physical buffer dimensions, causing the copy to read only a fraction of the expected physical pixels. At 2× scale this means 75% of pixel data is missing on partial repaints.

2. **Integer scale overwrites fractional scale:** The `scale_factor_changed` handler (compositor integer-scale protocol) unconditionally overwrites the more precise `preferred_scale` from the fractional-scale protocol. On compositors that fire both events (common in transitional protocol support), the fractional scale value is discarded.

Three WARNING-level issues and two INFO-level observations are also documented below.

## Critical Issues

### CR-01: SHM Copy Uses Logical Damage Rects on Physical Buffers

**File:** `crates/core/presentation/src/wayland_surface/backend.rs:837-852`

**Issue:** `present_with_damage` constructs `shm_copy_damage` by taking the union of `damage_rects` — which are in logical/CSS pixel coordinates — and passes it to `copy_into_shm_buffer` alongside the physical buffer dimensions. Inside `copy_into_shm_buffer`, the damage rect is treated as if it were in physical coordinates for clipping and the per-row copy. At any scale > 1.0, the copy region covers too few physical pixels.

For example, at 2× scale with a logical damage rect of `{x:0, y:0, width:100, height:100}` (which corresponds to physical `{x:0, y:0, width:200, height:200}`), `copy_bgra_damage_to_canvas` copies only 100×100 physical pixels — leaving 75% of the affected physical buffer region with stale data. This produces visible tearing/artifacts on partial repaints at non-1.0 scales.

The secondary effect: `pending_damage` on `SurfaceShmBuffer` (line 211) accumulates rects in this mixed coordinate space, compounding the error across frames. If scale changes between frames, the accumulated damage union crosses coordinate spaces and cannot correctly represent the physical region.

**Fix:** Scale the damage rect to physical coordinates *before* passing it to `copy_into_shm_buffer`. The simplest approach:

```rust
// In present_with_damage, around line 837:
let shm_copy_damage = damage_rects
    .iter()
    .copied()
    .map(|r| scale_damage_rect_to_physical(r, scale))
    .fold(None, |acc, r| Some(union_damage(acc, r)))
    .or_else(|| Some(full_damage(physical_w, physical_h)));
```

This ensures `copy_into_shm_buffer` receives damage rects in the same coordinate space as the `width`/`height` arguments and the source `buffer.data`. The `pending_damage` fields will also accumulate correctly in physical space.

**Alternative deeper fix (preferred):** Move the scaling into `copy_into_shm_buffer` so the function is self-documenting about its coordinate space contract. Add a `scale: f32` parameter and scale the damage rect there. This keeps the scaling boundary in one place.

---

### CR-02: Integer `scale_factor_changed` Overwrites Fractional `preferred_scale`

**File:** `crates/core/presentation/src/wayland_surface/handlers.rs:3-31`

**Issue:** The `CompositorHandler::scale_factor_changed` handler responds to the deprecated `wl_surface.preferred_buffer_scale` event, which carries an integer scale factor. It unconditionally clamps `new_factor` to `1..=3` and sets `entry.scale = new_factor as f32`. However, when `wp_fractional_scale_v1` is also active on a surface, the compositor will send more precise `preferred_scale(scale_120x)` events. If `scale_factor_changed` fires *after* `preferred_scale` (ordering varies by compositor), the integer value (e.g., `2`) overwrites the fractional value (e.g., `1.5`), reverting the surface to integer-only scaling.

This is protocol-pedantically correct behavior for `wl_surface.preferred_buffer_scale`, but incorrect for the user experience: when fractional scale is available, it should be authoritative.

**Fix:** Guard the handler to skip surfaces that already have a fractional-scale object bound:

```rust
fn scale_factor_changed(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    surface: &wl_surface::WlSurface,
    new_factor: i32,
) {
    let Some(entry) = self
        .surfaces
        .values_mut()
        .find(|entry| entry.layer_surface.wl_surface() == surface)
    else {
        return;
    };
    // When wp_fractional_scale_v1 is bound for this surface, prefer its
    // more precise preferred_scale events over the deprecated integer path.
    if entry.fractional_scale.is_some() {
        return;
    }
    let new_scale = new_factor.clamp(1, 3) as f32;
    // ... rest unchanged
```

## Warnings

### WR-01: `attach_shm_buffer` Overwrites Compositor-Configured Surface Dimensions

**File:** `crates/core/presentation/src/wayland_surface/backend.rs:323-325`

**Issue:** At the end of `attach_shm_buffer`, `entry.width` and `entry.height` are overwritten with the *requested* logical dimensions:

```rust
self.width = logical_width;   // from entry.cfg.width — what we *asked for*
self.height = logical_height; // from entry.cfg.height — what we *asked for*
```

However, the compositor may respond with different dimensions via `LayerSurfaceConfigure` (handled in `handlers.rs:118-141`), which sets `entry.width`/`entry.height` from the compositor's `configure.new_size`. Overwriting these with the requested values discards the compositor's authoritative sizing. The viewport destination (`set_destination(logical_width, logical_height)`) should use the compositor-configured dimensions, not the requested ones.

**Fix:** Use the compositor-configured dimensions (already stored in `entry.width`/`entry.height` before the overwrite) for the viewport destination, or rename the fields to distinguish configured-vs-requested:

```rust
// Use configured dimensions for the viewport (before the overwrite)
let configured_w = self.width.max(1);   // compositor-configured
let configured_h = self.height.max(1);  // compositor-configured
// ... use configured_w/configured_h for set_destination
// Only overwrite if the overwrite is intentional:
self.width = configured_w;
self.height = configured_h;
```

On most compositors the requested and configured dimensions are identical, so this bug rarely manifests visually, but the logic is incorrect.

### WR-02: Asymmetric Margin Clamping in `clamp_surface_config`

**File:** `crates/core/presentation/src/wayland_surface/backend.rs:703-744`

**Issue:** The `clamp_surface_config` method clamps margins against the output size, but only clamps one margin coordinate per edge:

| Edge | Margins clamped | Missing |
|------|----------------|---------|
| `Top` | `margin_left` only | `margin_right`, `margin_bottom` |
| `Bottom` | `margin_left`, `margin_bottom` | `margin_right` |
| `Left` / `None` | `margin_left`, `margin_top` | `margin_right`, `margin_bottom` |
| `Right` | `margin_right`, `margin_top` | `margin_left`, `margin_bottom` |

For `Edge::Top` (the most common panel orientation), both left and right margins should be clamped since the top bar anchors to `LEFT | TOP | RIGHT`. An unclamped `margin_right` could stack with a clamped `margin_left` to push the surface beyond the output width.

**Fix:** Clamp all anchor-relevant margins for each edge:

```rust
Some(Edge::Top) => {
    let max_left = max_width.saturating_sub(cfg.width) as i32;
    let max_right = max_width.saturating_sub(cfg.width) as i32;
    cfg.margin_left = cfg.margin_left.clamp(0, max_left.max(0));
    cfg.margin_right = cfg.margin_right.clamp(0, max_right.max(0));
}
```

### WR-03: `scale_damage_rect_to_physical` Truncates x/y via `as u32` Cast

**File:** `crates/core/presentation/src/wayland_surface/backend.rs:534-541`

**Issue:** The x and y coordinates of damage rects are converted to physical with `(rect.x as f32 * scale) as u32`, which truncates toward zero. Width and height use `.ceil()`. This inconsistency means the damage rect origin is rounded *down* while the extent is rounded *up*, creating a potential 1-physical-pixel gap between adjacent damage regions when x/y coordinates produce non-integer physical values.

At integer scales this is fine (x*1.0 is integer), but at fractional scales (e.g., 1.5×), `x=1` → `1.5` → `1` (truncates to physical pixel 1, but the logical pixel starts at physical position 1.5). The `clip_damage_rect_to_buffer` call immediately after mitigates this by clamping to buffer bounds, but the gating by `is_some_and()` in `protocol_damage_rects` doesn't re-check.

**Fix:** Use `.floor()` for x/y positions to match the rounding behavior of width/height's `.ceil()`:

```rust
fn scale_damage_rect_to_physical(rect: DamageRect, scale: f32) -> DamageRect {
    DamageRect {
        x: (rect.x as f32 * scale).floor() as u32,
        y: (rect.y as f32 * scale).floor() as u32,
        width: ((rect.width as f32 * scale).ceil() as u32).max(1),
        height: ((rect.height as f32 * scale).ceil() as u32).max(1),
    }
}
```

Or, alternatively, use `.ceil()` for both origin and extent (slightly larger damage, no gaps).

## Info

### IN-01: Unused `_title` Parameter in `present_with_damage`

**File:** `crates/core/presentation/src/wayland_surface/backend.rs:768-771`

**Issue:** The `_title: &str` parameter is accepted but never used in the Wayland backend. It exists only for API symmetry with the `dev_window` backend. While prefixing with `_` suppresses the compiler warning, the parameter creates a misleading API — callers might expect titles to appear somewhere.

**Fix:** Either remove the parameter from the Wayland-specific method signature (breaking `PresentationEngine::present_with_damage` symmetry), or document with an explicit comment: `/// title is only used by the dev_window backend; ignored for Wayland surfaces.`

### IN-02: Circular Dependency: `wayland_surface` Imports from `dev_window`

**File:** `crates/core/presentation/src/wayland_surface/mod.rs:13-14`

**Issue:** The `wayland_surface` module imports `DevWindowEvent`, `DevWindowKeyEvent`, and `KeyMods` from the sibling `dev_window` module:

```rust
use crate::dev_window::{DevWindowEvent, DevWindowKeyEvent, KeyMods};
```

This creates a dependency from the production Wayland code path to the dev-only minifb module. The shared event types should live in a neutral location (e.g., `crate::events` or the crate root). If `dev_window` is ever conditionally compiled out, the Wayland path would fail to build.

**Fix:** Move the shared event types (`DevWindowEvent`, `DevWindowKeyEvent`, `KeyMods`) to a separate module or to `crate::lib.rs` and re-export from both `dev_window` and `wayland_surface`. The crate already re-exports them at `lib.rs:9` — the types could live there directly.

---

_Reviewed: 2026-06-10T00:00:00Z_
_Reviewer: OpenCode (gsd-code-reviewer)_
_Depth: standard_
