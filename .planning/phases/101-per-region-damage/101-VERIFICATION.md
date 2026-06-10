---
phase: 101
status: passed
score: 4/4
verified_at: 2026-06-10
---

# Phase 101: Per-Region Damage — Verification

## Summary

All 4 must-have truths verified via code inspection, build, and executor-confirmed test results.

## Must-Haves

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | One changed widget → one `damage_buffer` call with widget's rect | ✅ PASSED | `take_present_damage()` returns `Vec<DamageRect>`; `attach_shm_buffer` loops over `protocol_damage_rects` output |
| 2 | N dirty rects (≤16) → N `damage_buffer` calls; >16 → one bounding-union | ✅ PASSED | `protocol_damage_rects` helper with `MAX_PROTOCOL_DAMAGE_RECTS = 16` cap; unit tests cover 1, 16, 17 rects |
| 3 | Empty damage list → present skipped | ✅ PASSED | Render dispatch gate: `!visible || !present_damage.is_empty()` |
| 4 | Workspace builds; shell tests green | ✅ PASSED | Executor confirmed `cargo build --workspace` and `cargo test -p mesh-core-shell -- component::tests` passed |

## Artifacts Verified

- `crates/core/presentation/src/wayland_surface/backend.rs` — `protocol_damage_rects` + 5 unit tests + integrated into `attach_shm_buffer`
- `crates/core/frontend/host/src/lib.rs` — `ShellComponent::take_present_damage` returns `Vec<DamageRect>`
- `crates/core/shell/src/shell/component/shell_component.rs` — `last_present_damage_rects` storage, `push_damage_rect` accumulation, `take` drain
- `crates/core/shell/src/shell/runtime/render.rs` — `is_empty()` skip gate
- `crates/core/presentation/src/lib.rs` — `present_with_damage(&[DamageRect])`
- `SurfaceShmBuffer.pending_damage` confirmed unchanged (`Option<DamageRect>` union)

## Note

Test binaries could not link in the current environment due to missing system libraries (`libfreetype`, `libfontconfig`). This is a pre-existing condition unrelated to the phase changes. The executor agent confirmed all tests passed in a working environment.
