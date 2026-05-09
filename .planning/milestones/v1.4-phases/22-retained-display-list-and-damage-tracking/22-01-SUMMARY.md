---
status: complete
phase: 22
plan: 1
completed: 2026-05-09
---

# Summary 22-01: Retained Display List and Damage Metrics

## Completed

- Added `RetainedDisplayList` in `mesh-core-render` with entries keyed by stable `NodeId` plus primitive slot.
- Added rectangle damage unioning and clipping for changed, inserted, removed, and forced full-surface cases.
- Added retained display-list metrics for total/reused/rebuilt/removed entries, damage area, full-surface fallback, partial-present support, and skipped-paint pixels.
- Integrated retained display-list metrics into `FrontendSurfaceComponent` after retained render-object synchronization.
- Extended `ProfilingInvalidationSnapshot` and debug JSON with retained paint metrics under `mesh.debug.profiling.surfaces[].invalidation.paint`.
- Kept partial-present support honest: current integration reports `partial_present_supported=false` and `skipped_paint_pixels=0` while still computing damage opportunities.

## Files Changed

- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/frontend/render/src/lib.rs`
- `crates/core/foundation/debug/src/lib.rs`
- `crates/core/shell/src/shell/component.rs`
- `crates/core/shell/src/shell/component/shell_component.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`
- `crates/core/shell/src/shell/tests.rs`

## Verification

- `cargo fmt --check` — passed
- `cargo test -p mesh-core-render display_list` — passed
- `cargo test -p mesh-core-render` — passed
- `nix develop -c cargo test -p mesh-core-shell profiling_snapshot_exposes_typed_surface_invalidation_counts` — passed
- `nix develop -c cargo test -p mesh-core-shell profiling` — passed

## Notes

- Running shell tests outside `nix develop` fails on this machine because `xkbcommon.pc` is not available to `pkg-config`; the same test passes inside the Nix dev shell.
- No backend partial-present path was added in this phase.
