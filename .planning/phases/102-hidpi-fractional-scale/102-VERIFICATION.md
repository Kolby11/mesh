---
phase: 102-hidpi-fractional-scale
verified: 2026-06-10T00:00:00Z
status: human_needed
score: 5/5 must-haves verified
overrides_applied: 0
re_verification: false
human_verification:
  - test: "On a 2× integer-scale display, verify text and icons appear sharp without upscaling artifacts"
    expected: "Visual output matches native 2× resolution; no blurring or upscaling artifacts on text/icons"
    why_human: "Requires a physical HiDPI display running a Wayland compositor with wl_output::scale=2"
  - test: "On a 1.5× fractional-scale display, verify wp_viewporter destination and buffer sizing"
    expected: "wp_viewporter::set_destination is called with logical dimensions; buffer allocated at ceil(logical × 1.5) physical pixels; compositor renders correct scaling"
    why_human: "Requires a compositor supporting wp_fractional_scale_v1 (e.g., KDE Plasma 6) and a 1.5× fractional-scale display"
  - test: "Plug in or unplug a HiDPI monitor (scale factor change) and verify full redraw without stale pixels"
    expected: "Surface resizes immediately to new scale; no leftover pixels from old scale visible; paint buffer recalculates at new physical dimensions"
    why_human: "Requires physical monitor hotplug with different scale factors; visual inspection of stale pixel artifacts"
  - test: "On a compositor without wp_fractional_scale_v1, verify wl_output::scale integer fallback keeps rendering correct"
    expected: "Surface renders correctly using integer wl_output::scale; no protocol errors; no visual corruption"
    why_human: "Requires a compositor that does not advertise wp_fractional_scale_v1 (e.g., sway 1.x); visual and protocol correctness inspection"
---

# Phase 102: HiDPI / Fractional Scale — Verification Report

**Phase Goal:** Shell surfaces render at native physical pixel density on HiDPI displays; layout coordinates stay in logical CSS pixels throughout
**Verified:** 2026-06-10
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SurfaceEntry stores a scale: f32 that reflects the compositor's current scale factor | ✓ VERIFIED | `backend.rs:91` — `scale: f32` field on `SurfaceEntry`, initialized to `1.0` at line 137 |
| 2 | wp_fractional_scale_v1::preferred_scale events update the stored scale factor on running surfaces | ✓ VERIFIED | `handlers.rs:378-407` — `Dispatch<WpFractionalScaleV1, String, State>` converts `preferred_scale` to `scale as f32 / 120.0`, clamps to `60..=480`, sets `needs_full_redraw = true` on change |
| 3 | wl_surface::scale_factor_changed events update the stored scale factor as integer fallback | ✓ VERIFIED | `handlers.rs:4-31` — `scale_factor_changed` handler clamps `new_factor` to `1..=3`, computes `(entry.scale - new_scale).abs() > f32::EPSILON`, sets `needs_full_redraw = true` |
| 4 | Scale changes mark affected surfaces for full redraw with the new scale | ✓ VERIFIED | `handlers.rs:23,98` — both handlers set `needs_full_redraw = true`; `render.rs:301-313` — render path checks `surface_needs_full_redraw()` and emits full-logical-damage `vec![DamageRect{x:0, y:0, width, height}]` |
| 5 | The integer wl_output::scale path works when wp_fractional_scale_v1 is unavailable | ✓ VERIFIED | `handlers.rs:4-31` — `CompositorHandler::scale_factor_changed` handles integer path; `backend.rs:598-601` — both protocols bound as `Option` via `.ok()` so absence does not prevent startup |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/core/presentation/Cargo.toml` | New wayland-protocols dependency | ✓ VERIFIED | `wayland-protocols = { version = "0.32", features = ["client", "staging"] }` at line 21. Note: `wayland-protocols-wlr` removed per D-03 (not in crate). |
| `crates/core/presentation/src/wayland_surface/state.rs` | wp_viewporter + fractional_scale_manager fields on State | ✓ VERIFIED | `viewporter: Option<WpViewporter>` at line 44, `fractional_scale_manager: Option<WpFractionalScaleManagerV1>` at line 45; `bind_fractional_scale()` helper at lines 309-318 |
| `crates/core/presentation/src/wayland_surface/handlers.rs` | scale_factor_changed and fractional_scale handlers | ✓ VERIFIED | `CompositorHandler::scale_factor_changed` at lines 4-31; `Dispatch<WpFractionalScaleV1, String, State>` at lines 378-407; `Dispatch<WpViewport, ()>` at lines 579-590; `Dispatch<WpViewporter, GlobalData, State>` at lines 330-341; `Dispatch<WpFractionalScaleManagerV1, GlobalData, State>` at lines 343-354 |
| `crates/core/presentation/src/wayland_surface/backend.rs` | scale: f32 on SurfaceEntry | ✓ VERIFIED | `scale: f32` at line 91, `needs_full_redraw: bool` at line 92, `fractional_scale: Option<WpFractionalScaleV1>` at line 93, `viewport: Option<WpViewport>` at line 94; all initialized in `SurfaceEntry::new()` at lines 137-140 |
| `crates/core/presentation/src/lib.rs` | scale parameter in PresentationEngine | ✓ VERIFIED | `surface_scale()` at lines 177-182; `surface_needs_full_redraw()` at lines 184-189; `clear_surface_needs_full_redraw()` at lines 191-195 |
| `crates/core/presentation/src/wayland_surface/mod.rs` | protocol imports | ✓ VERIFIED | `wp_fractional_scale_*` imports at lines 56-59; `wp_viewporter` imports at lines 60-62 |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| handlers.rs scale_factor_changed | backend.rs SurfaceEntry.scale | State.surfaces lookup | ✓ WIRED | `handlers.rs:11-14` finds entry via `surfaces.values_mut().find()`; `handlers.rs:21-22` sets `entry.scale = new_scale` |
| handlers.rs fractional_scale preferred_scale | backend.rs SurfaceEntry.scale | State.surfaces lookup | ✓ WIRED | `handlers.rs:395` does `state.surfaces.get_mut(surface_id)`; `handlers.rs:397` sets `entry.scale = new_scale` |
| backend.rs LayerShellBackend::new | state.rs wp_viewporter + fractional_scale_manager | globals.bind() | ✓ WIRED | `backend.rs:599-601` binds both as `Option` via `globals.bind(&qh, 1..=1, GlobalData).ok()` |
| render.rs PixelBuffer::new call | backend.rs SurfaceEntry.scale | presentation_engine.surface_scale() | ✓ WIRED | `render.rs:89` calls `self.presentation_engine.surface_scale(&surface_id)`; `render.rs:198-199` computes `physical_w/h` from scale |
| backend.rs attach_shm_buffer | wp_viewporter::WpViewport | entry.viewport.set_destination() | ✓ WIRED | `backend.rs:302-309` fractional scale: `viewport.set_destination(logical_width, logical_height)` + `set_buffer_scale(ceil(scale))` |
| backend.rs present_with_damage | damage rects | scale_rect_to_physical() before damage_buffer | ✓ WIRED | `backend.rs:273-276` — `scale_damage_rect_to_physical` called per rect; `backend.rs:285-292` — physical coordinates emitted to `damage_buffer` |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|--------------|--------|--------------------|--------|
| `render.rs` PixelBuffer allocation | `scale: f32` | `PresentationEngine::surface_scale()` → `SurfaceEntry.scale` ← compositor events | ✓ FLOWING | Real compositor events populate scale; `1.0` default when no compositor connected |
| `backend.rs` attach_shm_buffer scale logic | `scale: f32` | `SurfaceEntry.scale` from `present_with_damage` entry lookup | ✓ FLOWING | Scale from `entry.scale` at `backend.rs:828`; flows through integer/fractional detection at line 295 |
| `handlers.rs` needs_full_redraw flag | `needs_full_redraw: bool` | Set by scale handlers, consumed by render.rs | ✓ FLOWING | `handlers.rs:23,98` set to true; `render.rs:303-309` reads and clears |
| `backend.rs` viewport.set_destination | `logical_width, logical_height` | `entry.cfg.width, entry.cfg.height` | ✓ FLOWING | Values from real compositor configure events; used for non-integer scale only |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Presentation crate tests pass (25 tests) | `cargo test -p mesh-core-presentation -- --test-threads=1` | 25 passed, 0 failed | ✓ PASS |
| Workspace compiles | `cargo check --workspace` | Compiles with warnings only | ✓ PASS |
| Shell component tests (non-scale) | `cargo test -p mesh-core-shell -- component::tests` | 156 passed, 37 pre-existing failures | ? SKIP — 37 failures are pre-existing (service/rendering integration tests, confirmed unrelated to scale: e.g., network connect commands) |
| No `wayland-protocols-wlr` | `grep wayland-protocols-wlr Cargo.toml` | Not present | ✓ PASS — Correct deviation per D-03 |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| HDPI-01 | 102-01-PLAN | SurfaceEntry stores scale_factor updated from compositor events | ✓ SATISFIED | `scale: f32` on `SurfaceEntry` (backend.rs:91); updated by both `scale_factor_changed` (handlers.rs:4-31) and `preferred_scale` (handlers.rs:378-407) |
| HDPI-02 | 102-02-PLAN | PixelBuffer allocated at ceil(logical × scale_factor) physical pixels | ✓ SATISFIED | `render.rs:198-199` — `((width as f32 * scale).ceil() as u32).max(1)`; `render.rs:224-225` — `PixelBuffer::new(physical_w, physical_h)` |
| HDPI-03 | 102-02-PLAN | wp_viewporter sets destination size to logical dimensions | ✓ SATISFIED | `backend.rs:302-309` — `viewport.set_destination(logical_width as i32, logical_height as i32)` for non-integer scales |
| HDPI-04 | 102-01-PLAN, 102-02-PLAN | Scale factor changes trigger surface resize and full redraw | ✓ SATISFIED | `handlers.rs:23,98` sets `needs_full_redraw = true`; `render.rs:301-313` checks flag and emits full-damage |
| HDPI-05 | 102-01-PLAN | Integer wl_output::scale path as fallback | ✓ SATISFIED | `handlers.rs:4-31` — `CompositorHandler::scale_factor_changed` integer path; both protocols bound as `Option` (backend.rs:599-601) |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | None found | — | — |

No stubs, TODOs, hardcoded empty data, or placeholder implementations found in scale-related code paths.

### Deferred Items

None. All Phase 102 must-haves are implemented in this phase.

### Human Verification Required

All four ROADMAP success criteria require a running Wayland compositor with specific hardware configurations to verify. Code-level verification confirms every component is properly wired:

#### 1. 2× Integer-Scale Visual Quality (SC-1)

**Test:** Run MESH on a compositor with a 2× integer-scale HiDPI display. Inspect text rendering and icon display.
**Expected:** Text and icons appear sharp at native 2× resolution without upscaling artifacts (blurring, pixelation, or doubling).
**Why human:** Requires physical HiDPI hardware and visual inspection of rendered output.

#### 2. 1.5× Fractional-Scale Protocol Verification (SC-2)

**Test:** Run MESH on a compositor supporting `wp_fractional_scale_v1` (e.g., KDE Plasma 6) at 1.5× scale. Use `WAYLAND_DEBUG=1` or a protocol monitor to verify:
- `wp_viewporter::set_destination(logical_w, logical_h)` is called
- Buffer is allocated at `ceil(logical × 1.5)` physical pixels
- `wl_surface::set_buffer_scale(2)` is called (ceil of 1.5)
**Expected:** Buffer dimensions match expected physical size; viewport destination matches logical surface dimensions; compositor correctly scales down from physical to logical viewport.
**Why human:** Requires specific compositor with `wp_fractional_scale_v1` support and protocol-level monitoring tools.

#### 3. Hotplug Scale Change (SC-3)

**Test:** With MESH running, plug in or unplug a HiDPI monitor that reports a different scale factor than the current display. Observe the MESH surfaces.
**Expected:** Surface immediately resizes; full redraw occurs; no stale pixels from the previous scale remain visible; no visual tearing or glitches during the transition.
**Why human:** Requires physical monitor hotplug with different scale factors between outputs.

#### 4. Integer Fallback Compositor (SC-4)

**Test:** Run MESH on a compositor that does NOT advertise `wp_fractional_scale_v1` (e.g., sway 1.x at 2× scale). Verify rendering is correct.
**Expected:** `wl_output::scale` integer fallback is used; `set_buffer_scale(2)` is called for 2× display; no protocol errors in logs; visual rendering is correct at the integer scale.
**Why human:** Requires a compositor without fractional-scale support and visual inspection.

### Gaps Summary

No code-level gaps found. All five requirements (HDPI-01 through HDPI-05) are satisfied by existing code. All four ROADMAP success criteria have verified code paths but require human verification on real hardware to confirm visual correctness and compositor behavior.

**Key architectural invariants verified:**
- Damage rects remain in logical coordinates from render.rs through `present_with_damage`; single scaling boundary in `attach_shm_buffer` (backend.rs:273-276)
- `scale_damage_rect_to_physical` correctly scales x/y/w/h and clips to buffer bounds (backend.rs:534-556)
- Integer/fractional detection uses `f32::EPSILON` comparison (backend.rs:295)
- Threat model mitigations: T-102-01 (clamp 1..=3), T-102-02 (clamp 60..=480), T-102-05 (512 MB cap), T-102-06 (clip to buffer bounds)

---

_Verified: 2026-06-10_
_Verifier: OpenCode (gsd-verifier)_
