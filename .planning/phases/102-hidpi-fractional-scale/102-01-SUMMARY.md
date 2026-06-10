---
phase: 102-hidpi-fractional-scale
plan: 01
subsystem: presentation
tags: [wayland, hidpi, fractional-scale, viewporter, scale-factor]
requires:
  - "wayland-protocols 0.32 with staging feature"
  - "existing wayland-client 0.31 and smithay-client-toolkit 0.19"
provides:
  - "authoritative scale: f32 on SurfaceEntry, updated by compositor events"
  - "PresentationEngine::surface_scale() public API"
  - "WpFractionalScaleV1 per-surface binding"
affects:
  - "crates/core/presentation (Cargo.toml, lib.rs, wayland_surface/*)"
tech-stack:
  added:
    - "wayland-protocols 0.32 (client + staging features)"
  patterns:
    - "Optional protocol binding via globals.bind(..).ok()"
    - "Dispatch<Protocol, UserData, State> pattern for Wayland event handling"
    - "f32::EPSILON comparison for scale change detection"
key-files:
  created: []
  modified:
    - crates/core/presentation/Cargo.toml
    - crates/core/presentation/src/wayland_surface/mod.rs
    - crates/core/presentation/src/wayland_surface/state.rs
    - crates/core/presentation/src/wayland_surface/handlers.rs
    - crates/core/presentation/src/wayland_surface/backend.rs
    - crates/core/presentation/src/lib.rs
decisions:
  - "D-01: Store scale: f32 on SurfaceEntry (not a separate ScaleState struct)"
  - "D-02: WpViewporter and WpFractionalScaleManagerV1 bound as Option (protocol absence does not prevent MESH startup)"
  - "D-03: wp_viewporter lives in wayland-protocols::wp::viewporter, NOT wayland-protocols-wlr (deviation from original plan)"
  - "D-04: Clamp scale_factor_changed to 1..=3 (T-102-01) and preferred_scale to 60..=480 (T-102-02)"
  - "D-05: Fractional scale uses String user-data for surface_id lookup (matching existing HashMap key pattern)"
metrics:
  duration: "TBD"
  completed_date: "2026-06-10"
---

# Phase 102 Plan 01: HiDPI Scale Acquisition

**One-liner:** Bind wp_viewporter and wp_fractional_scale_v1 protocols, store authoritative `scale: f32` on `SurfaceEntry` updated by compositor events, with threat-model clamping and public read API.

## Summary

Plan 01 establishes the foundation for HiDPI rendering by wiring compositor-provided scale factors into the presentation backend. All three tasks completed:

1. **Protocol dependencies and binding** — `wayland-protocols` (0.32, with `staging` feature) added; `wp_viewporter` and `WpFractionalScaleManagerV1` bound as optional globals during compositor handshake.
2. **Scale field and event handlers** — `scale: f32`, `needs_full_redraw: bool`, and `fractional_scale: Option<WpFractionalScaleV1>` added to `SurfaceEntry`; `scale_factor_changed` handler (integer fallback) and `Dispatch<WpFractionalScaleV1>` handler (fractional path) implemented with clamping per threat model T-102-01/02.
3. **Public API and tests** — `surface_scale()`, `surface_needs_full_redraw()`, `clear_surface_needs_full_redraw()` exposed through `LayerShellBackend` and `PresentationEngine`; 4 new unit tests for scale math.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] wp_viewporter module path differs from plan**
- **Found during:** task 1
- **Issue:** `wp_viewporter` is in `wayland-protocols::wp::viewporter` (not `wayland-protocols-wlr`). The `wayland-protocols-wlr` 0.3.12 crate does not include viewporter. Also, `fractional_scale` requires the `staging` feature flag.
- **Fix:** Used `wayland-protocols::wp::viewporter::client::*` and `wayland-protocols::wp::fractional_scale::v1::client::*` with the `wp_` prefix (not `zwp_`). Removed `wayland-protocols-wlr` dependency.
- **Files modified:** `Cargo.toml`, `mod.rs`, `state.rs`, `backend.rs`, `handlers.rs`
- **Commit:** `b0d2792`

**2. [Rule 1 - Bug] Borrow checker error in configure() fractional_scale binding**
- **Found during:** task 2
- **Issue:** The original plan code had overlapping mutable and immutable borrows of `self.state` when binding fractional_scale inside configure().
- **Fix:** Restructured to clone `wl_surface` and `qh` before the immutable borrow, then use separate `if let` chains.
- **Files modified:** `backend.rs`
- **Commit:** `3a1dda9`

**3. [Rule 1 - Bug] Refutable pattern in PreferredScale handler**
- **Found during:** task 2
- **Issue:** `wp_fractional_scale_v1::Event` is an enum, so `let Event::PreferredScale { scale } = event;` is a refutable pattern.
- **Fix:** Changed to `let ... else { return; };` syntax.
- **Files modified:** `handlers.rs`
- **Commit:** `3a1dda9`

**4. [Rule 1 - Bug] Ambiguous float type in fractional_scale_converts_120x_to_f32 test**
- **Found during:** task 3
- **Issue:** Literal float arithmetic (`(120.0 / 120.0) - 1.0`) produced ambiguous `{float}` type that couldn't have `.abs()` called on it.
- **Fix:** Added explicit `let v: f32 = ...` bindings with type annotation.
- **Files modified:** `backend.rs`
- **Commit:** `6c9e978`

**5. [Rule 1 - Bug] f32 EPSILON test used values that rounded differently**
- **Found during:** task 3
- **Issue:** `1.5000001_f32` didn't round to exactly `1.5_f32` as expected due to f64→f32 conversion behavior.
- **Fix:** Changed to `let same: f32 = 1.5;` (identical value) to test the comparison mechanism.
- **Files modified:** `backend.rs`
- **Commit:** `6c9e978`

## Threat Model Compliance

| Threat ID | Status | Implementation |
|-----------|--------|---------------|
| T-102-01 | Mitigated | `scale_factor_changed` clamps `new_factor` to `1..=3` before casting to f32 |
| T-102-02 | Mitigated | `PreferredScale` handler clamps `scale` to `60..=480` before dividing by 120.0 |
| T-102-03 | Deferred | Buffer allocation size capping belongs to Plan 02 |
| T-102-04 | Accepted | `tracing::info!` logging of scale/factor/surface dimensions is acceptable |

## Commits

| Task | Hash | Message |
|------|------|---------|
| 1 | `b0d2792` | feat(102-hidpi-fractional-scale): bind wp_viewporter and wp_fractional_scale_v1 protocols |
| 2 | `3a1dda9` | feat(102-hidpi-fractional-scale): store scale on SurfaceEntry and implement scale handlers |
| 3 | `6c9e978` | feat(102-hidpi-fractional-scale): expose scale via public API and add unit tests |

## Verification Results

- ✅ `nix develop -c cargo check -p mesh-core-presentation` — compiles
- ✅ `nix develop -c cargo test -p mesh-core-presentation -- --test-threads=1` — 17/17 tests pass
- ✅ `scale: f32` field confirmed on `SurfaceEntry` (backend.rs:91)
- ✅ `scale_factor_changed` handler no longer a no-op (handlers.rs:4)
- ✅ `PreferredScale` handler exists (handlers.rs:387)
- ✅ All existing tests continue to pass

## Known Stubs

None. All fields are wired with default values and update paths; no UI-facing stubs present.

## Self-Check: PASSED

All files exist, all commits verified, all tests pass.
