---
phase: 102-hidpi-fractional-scale
fixed_at: 2026-06-10T00:00:00Z
review_path: .planning/phases/102-hidpi-fractional-scale/102-REVIEW.md
iteration: 1
findings_in_scope: 5
fixed: 5
skipped: 0
status: all_fixed
---

# Phase 102: Code Review Fix Report

**Fixed at:** 2026-06-10T00:00:00Z
**Source review:** .planning/phases/102-hidpi-fractional-scale/102-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 5
- Fixed: 5
- Skipped: 0

## Fixed Issues

### CR-01: SHM Copy Uses Logical Damage Rects on Physical Buffers

**Files modified:** `crates/core/presentation/src/wayland_surface/backend.rs`
**Commit:** `b000a67`
**Applied fix:** Added `.map(|r| scale_damage_rect_to_physical(r, scale))` to the damage rect fold chain in `present_with_damage` so that damage rects are scaled from logical to physical coordinates before being passed to `copy_into_shm_buffer`. This ensures the SHM copy region matches the physical buffer dimensions at scales > 1.0.

### CR-02: Integer `scale_factor_changed` Overwrites Fractional `preferred_scale`

**Files modified:** `crates/core/presentation/src/wayland_surface/handlers.rs`
**Commit:** `91ff47d`
**Applied fix:** Added an early return guard in `CompositorHandler::scale_factor_changed`: when `entry.fractional_scale.is_some()`, the handler skips the integer scale update, preferring the more precise `wp_fractional_scale_v1.preferred_scale` events. This prevents the deprecated integer protocol from overwriting fractional scale values on compositors that fire both events.

### WR-01: `attach_shm_buffer` Overwrites Compositor-Configured Surface Dimensions

**Files modified:** `crates/core/presentation/src/wayland_surface/backend.rs`
**Commit:** `4a99397`
**Applied fix:** Changed `present_with_damage` to derive logical dimensions from `entry.width`/`entry.height` (compositor-configured) instead of `entry.cfg.width`/`entry.cfg.height` (requested). Since `entry.width`/`entry.height` already hold the compositor's authoritative size, the subsequent store in `attach_shm_buffer` becomes a no-op and the viewport destination always reflects the compositor's actual configuration.

### WR-02: Asymmetric Margin Clamping in `clamp_surface_config`

**Files modified:** `crates/core/presentation/src/wayland_surface/backend.rs`
**Commit:** `abbffdb`
**Applied fix:** Added `margin_right` clamping for both `Edge::Top` and `Edge::Bottom` in `clamp_surface_config`. Top and bottom edges anchor to `LEFT | RIGHT`, so both horizontal margins must be clamped to prevent the surface from exceeding the output width.

### WR-03: `scale_damage_rect_to_physical` Truncates x/y via `as u32` Cast

**Files modified:** `crates/core/presentation/src/wayland_surface/backend.rs`
**Commit:** `21c3129`
**Applied fix:** Changed x/y coordinate conversion from truncation (`as u32`) to `.floor() as u32` to match the rounding behavior of width/height's `.ceil()`. This prevents a 1-physical-pixel gap between adjacent damage regions at fractional scales where x/y coordinates produce non-integer physical values.

---

_Fixed: 2026-06-10T00:00:00Z_
_Fixer: OpenCode (gsd-code-fixer)_
_Iteration: 1_
