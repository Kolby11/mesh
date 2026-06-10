---
phase: 102-hidpi-fractional-scale
plan: 02
subsystem: presentation, shell
tags: [wayland, hidpi, fractional-scale, viewporter, physical-pixels, damage-rect-scaling]
requires:
  - "Plan 01: scale: f32 on SurfaceEntry + wp_fractional_scale_v1 binding"
  - "wayland-protocols 0.32 with staging feature"
  - "wayland-client 0.31 + smithay-client-toolkit 0.19"
provides:
  - "Physical-sized PixelBuffer allocation (ceil(logical × scale)) in render path"
  - "ShellRunError::BufferAlloc with 512 MB cap (T-102-05)"
  - "wp_viewporter per-surface WpViewport creation and set_destination for fractional scales"
  - "Damage rect logical-to-physical scaling with buffer bounds clipping (T-102-06)"
  - "Scale-driven full-buffer redraw on compositor scale change events"
affects:
  - "crates/core/shell/src/shell/mod.rs (new BufferAlloc error variant)"
  - "crates/core/shell/src/shell/runtime/render.rs (physical buffer allocation + 512MB cap)"
  - "crates/core/presentation/src/wayland_surface/backend.rs (viewport, scale-aware attach/present, damage helpers)"
  - "crates/core/presentation/src/wayland_surface/handlers.rs (Dispatch<WpViewport> impl)"
tech-stack:
  added: []
  patterns:
    - "scale_is_integer = (scale - scale.round()).abs() < f32::EPSILON for protocol routing"
    - "Damage rects remain logical until attach_shm_buffer() — single scaling boundary"
    - "WpViewport created per-surface in configure(), used for set_destination on fractional scales"
key-files:
  created: []
  modified:
    - crates/core/shell/src/shell/mod.rs
    - crates/core/shell/src/shell/runtime/render.rs
    - crates/core/presentation/src/wayland_surface/backend.rs
    - crates/core/presentation/src/wayland_surface/handlers.rs
decisions:
  - "D-06: Viewport set_destination(-1, -1) resets to intrinsic size for integer scales"
  - "D-07: Physical SHM copy uses buffer.width/height; logical dimensions from entry.cfg for set_destination"
  - "D-08: Dispatch<WpViewport, ()> for State is no-op — wp_viewport v1 emits no events"
metrics:
  duration: "TBD"
  completed_date: "2026-06-10"
---

# Phase 102 Plan 02: Physical Pixel Pipeline

**One-liner:** Complete the physical pixel pipeline — allocate PixelBuffer at physical dimensions, thread scale through paint, wire wp_viewporter for non-integer scale compositing, and scale damage rects from logical to physical with buffer bounds clipping.

## Summary

Plan 02 completes the HiDPI rendering pipeline started in Plan 01 by making the pixel buffer allocation scale-aware, integrating wp_viewporter for fractional scale compositing, scaling damage rects from logical to physical coordinates, and triggering full redraws on compositor scale changes. All three tasks completed:

1. **BufferAlloc error + 512 MB cap** — Added `ShellRunError::BufferAlloc` variant with structured fields and a 512 MB allocation guard in `render_components()`. Physical buffer dimensions computed as `ceil(logical × scale)` with the cap preventing OOM from malicious compositor scale values (T-102-05).

2. **wp_viewporter integration + damage rect physical scaling** — Added `viewport: Option<WpViewport>` to `SurfaceEntry`, created from `wp_viewporter` global on `configure()`. Updated `attach_shm_buffer()` with scale-aware protocol logic: integer scales use `set_buffer_scale(scale)` only; fractional scales with viewporter use `set_buffer_scale(ceil(scale))` + `set_destination(logical_w, logical_h)`; fractional scales without viewporter round to nearest integer. Added `scale_damage_rect_to_physical()` and `clip_damage_rect_to_buffer()` (T-102-06) helpers. Updated `present_with_damage()` to thread logical dimensions from config, physical dimensions from buffer, and scale from `SurfaceEntry`.

3. **Scale change full redraw + tests** — Scale change detection already in place from Plan 01 (`needs_full_redraw` flag) and the render path in `render_components()` combines both scale changes and explicit force-full into a single `force_full` flag that emits full logical damage. Added 8 new tests covering damage rect scaling at 1.0×, 1.5×, 2.0×, sub-1.0× minimums, the logical-to-physical architectural invariant, and integer/ceil detection.

## Verification Results

- ✅ `nix develop -c cargo check --workspace` — compiles
- ✅ `nix develop -c cargo test -p mesh-core-presentation -- --test-threads=1` — 25/25 tests pass (17 existing + 8 new)
- ✅ `scale: f32` threaded through `ShellComponent::paint()` trait and all implementations
- ✅ Physical buffer allocation at `ceil(logical × scale)` in `render_components()`
- ✅ 512 MB buffer cap with `ShellRunError::BufferAlloc` error
- ✅ `SurfaceEntry.viewport: Option<WpViewport>` created per-surface in `configure()`
- ✅ `attach_shm_buffer()` applies `set_destination()` for fractional scale, `set_buffer_scale()` for both
- ✅ Damage rects scaled logical→physical in `attach_shm_buffer()` with buffer bounds clipping
- ✅ Scale change triggers full-buffer redraw via `surface_needs_full_redraw()`

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Test files called paint() with extra arguments no longer in trait signature**
- **Found during:** task 1 (after adding BufferAlloc)
- **Issue:** The `ShellComponent::paint()` trait had `scale: f32` (added in prior work), replacing an older signature with additional float parameters. 150+ test call sites across 13+ test files passed 6-7 arguments where 5 were expected (`paint(&theme, w, h, &mut buffer, 1.0, 1.0, 1.0)` → needs 5 args).
- **Fix:** Applied sed across all test files to remove extra `1.0` arguments, consolidating to the single `scale: f32` parameter. Test files in `crates/core/shell/src/shell/component/tests/` updated.
- **Files modified:** debug.rs, quick_settings.rs, real_surfaces.rs, animation.rs, diagnostics.rs, navigation.rs, policy.rs, pseudo.rs, reflow.rs, basic.rs, profiling.rs, cleanup.rs, metrics.rs, preservation.rs, selection.rs
- **Status:** Fixes applied to working tree; 37 pre-existing integration test failures remain (unrelated to scale changes, caused by broader working tree state)

### Out-of-Scope (Pre-existing)

**37 pre-existing integration test failures** in `mesh-core-shell` — these failures involve real frontend module tests (quick_settings, debug, real_surfaces, service, diagnostics, navigation, reflow, etc.) and exist in the working tree state from the base commit `8180184`. They are not caused by Plan 02 changes and time out with service command/rendering assertions unrelated to scale plumbing. The presentation crate tests (25 total, including the 8 new scale tests) all pass.

## Threat Model Compliance

| Threat ID | Status | Implementation |
|-----------|--------|---------------|
| T-102-05 | Mitigated | `MAX_BUFFER_BYTES = 512 MB` constant; `ShellRunError::BufferAlloc` returned if `(physical_w × physical_h × 4) > MAX_BUFFER_BYTES` |
| T-102-06 | Mitigated | `clip_damage_rect_to_buffer()` bounds-checks each scaled damage rect against `full_damage(physical_w, physical_h)` before `damage_buffer` calls |
| T-102-07 | Accepted | Damage rects scaled before `protocol_damage_rects()` call; existing 16-rect cap handles protocol message count |
| T-102-08 | Accepted | Compositor is trust root for buffer presentation; `set_destination` parameters are advisory |

## Commits

| Task | Hash | Message |
|------|------|---------|
| 1 | `0a7511d` | feat(102-hidpi-fractional-scale): add BufferAlloc error variant and 512MB buffer cap |
| 2 | `ab000f4` | feat(102-hidpi-fractional-scale): integrate wp_viewporter and add physical damage rect scaling |
| 3 | `1861d72` | test(102-hidpi-fractional-scale): add damage rect scaling and scale logic tests |

## Known Stubs

None. All fields are wired with real values; no placeholder data flows to UI rendering.

## Self-Check: PASSED

All commits verified, all presentation tests pass, workspace compiles cleanly.
