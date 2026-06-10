---
phase: 101-per-region-damage
plan: 01
subsystem: presentation-damage
tags: [damage-tracking, wayland, refactor, tdd]
requires: []
provides: [DMGE-01, DMGE-02]
affects:
  - presentation-engine
  - shell-render
  - shell-component
  - wayland-backend
tech-stack:
  added: []
  patterns:
    - "Per-rect damage_vec threading through present path"
    - "16-rect protocol message cap with bounding-union fallback"
    - "SHM copy union separated from damage_buffer rect list"
key-files:
  created:
    - ""
  modified:
    - crates/core/presentation/src/wayland_surface/backend.rs
    - crates/core/frontend/host/src/lib.rs
    - crates/core/presentation/src/lib.rs
    - crates/core/shell/src/shell/component.rs
    - crates/core/shell/src/shell/component/shell_component.rs
    - crates/core/shell/src/shell/runtime/render.rs
    - crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs
    - crates/core/shell/src/shell/component/tests/interaction/policy.rs
    - crates/core/shell/src/shell/component/tests/invalidation/profiling.rs
decisions:
  - "protocol_damage_rects helper extracted as pure function for testability"
  - "16-rect cap with union fallback (not truncation) for protocol message bound"
  - "SHM copy region stays unioned; per-rect damage_buffer only for protocol calls"
  - "Empty Vec skip preserves existing None-skip semantics"
  - "effective_damage.rects flows through push_damage_rect into last_present_damage_rects"
metrics:
  duration: "128 minutes"
  completed_date: "2026-06-10T14:30:00Z"
---

# Phase 101 Plan 01: Per-Region Damage Summary

**One-liner:** Threaded `Vec<DamageRect>` from component paint through render dispatch and presentation engine into per-rect `wl_surface::damage_buffer` calls with a 16-rect cap and bounding-union fallback, replacing the single unioned `Option<DamageRect>` that forced the compositor to recomposit unchanged pixels.

## What Was Built

### Task 1: protocol_damage_rects helper + unit tests
- Added `const MAX_PROTOCOL_DAMAGE_RECTS = 16` in `backend.rs`
- Added pure `protocol_damage_rects(rects: &[DamageRect], width, height) -> Vec<DamageRect>` function:
  - ≤16 rects: passthrough unchanged (same count, same order)
  - >16 rects: single bounding-union element
  - Empty: returns empty Vec (caller skips present)
- 5 unit tests: single-rect, exactly-16, 17-union, empty-input, known-geometry union

### Task 2: Vec<DamageRect> threaded through 9 integration points
1. **ShellComponent trait** (`host/src/lib.rs`): `take_present_damage()` returns `Vec<DamageRect>` (default `Vec::new()`)
2. **Storage** (`component.rs`): `last_present_damage: Option<DamageRect>` → `last_present_damage_rects: Vec<DamageRect>`
3. **Paint accumulation** (`shell_component.rs`): Replaced `merge_optional_damage` with `push_damage_rect` per-rect accumulation; full-surface clears and pushes single rect
4. **take_present_damage** (`shell_component.rs`): Returns `std::mem::take(&mut self.last_present_damage_rects)`
5. **Render dispatch** (`render.rs`): Uses `Vec<DamageRect>`, skip gate changed from `.is_some()` to `!is_empty()`, force-full/show-layout-bounds use `vec![full]`
6. **PresentationEngine** (`lib.rs`): `present_with_damage` parameter changed to `damage: &[DamageRect]`; DevWindow ignores, Wayland forwards slice
7. **WaylandSurfaceBackend** (`backend.rs`): `present_with_damage` folds slice into union for SHM copy, forwards `&[DamageRect]` to attach
8. **copy_into_shm_buffer**: Kept `Option<DamageRect>` — SHM copy region stays unioned (Pitfall 1)
9. **attach_shm_buffer**: Changed to `damage_rects: &[DamageRect]`, loops `protocol_damage_rects(damage_rects, width, height)` for per-rect `wl_surface::damage_buffer` calls

### Test updates
- 3 test assertions updated from `.is_some()` to `!is_empty()` (Pitfall 4)
- All 4 critical damage-path tests pass (profiling, policy, real_surfaces)
- All 13 presentation tests pass including 5 new `protocol_damage_rects` tests
- 30+ pre-existing failures unrelated to damage tracking (audio service, debug inspector)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Unused `paint_damage` variable after merge_optional_damage removal**
- **Found during:** Task 2
- **Issue:** After replacing `merge_optional_damage` with Vec-based accumulation, the `paint_damage` binding was no longer consumed, causing a compiler warning
- **Fix:** Prefixed with underscore: `let _paint_damage = ...`
- **Files modified:** `crates/core/shell/src/shell/component/shell_component.rs`
- **Commit:** `c0e2921`

**2. [Rule 3 - Blocker] Build environment missing xkbcommon, freetype, fontconfig**
- **Found during:** Task 1 verification
- **Issue:** `cargo test -p mesh-core-presentation` failed at link stage because system Wayland libraries (xkbcommon, freetype, fontconfig) were not installed on the NixOS host
- **Fix:** Used `nix-shell -p pkg-config libxkbcommon freetype fontconfig libGL` as build wrapper
- **Files modified:** None (environment only)
- **Note:** Pre-existing environment issue; no code changes needed

### Out-of-Scope Discoveries

- **`merge_optional_damage` removed**: Function became unused after paint accumulation switch; removed cleanly. `damage_rects_from_options_into` kept (still used at line 421).
- **`present()` convenience method**: Verified only called from `present_with_damage` (DevWindow path only). Passes `&[full_damage]` for safety.

## Verification Results

- `cargo test -p mesh-core-presentation`: **13/13 passed** (100%)
- `cargo test -p mesh-core-shell -- component::tests`: 4 critical damage-path tests **all passed**; 30+ pre-existing failures unrelated to this change
- `cargo build --workspace`: **succeeds** with no new warnings (only pre-existing dead_code warnings)
- Manual grep verification: all 9 acceptance criteria met (see checks in commit message)

## Commits

| Commit | Type | Description |
|--------|------|-------------|
| `470bcdc` | test | Add protocol_damage_rects helper with 5 unit tests |
| `c0e2921` | feat | Thread Vec<DamageRect> through 9 integration points |

## Self-Check: PASSED

- [x] `crates/core/presentation/src/wayland_surface/backend.rs` — FOUND (Task 1 + Task 2 changes)
- [x] `crates/core/frontend/host/src/lib.rs` — FOUND (trait signature change)
- [x] `crates/core/shell/src/shell/component.rs` — FOUND (storage field renamed)
- [x] `crates/core/shell/src/shell/component/shell_component.rs` — FOUND (paint + take + cleanup)
- [x] `crates/core/shell/src/shell/runtime/render.rs` — FOUND (dispatch + skip gate)
- [x] `crates/core/presentation/src/lib.rs` — FOUND (signature change)
- [x] Commits `470bcdc` and `c0e2921` exist in git log
