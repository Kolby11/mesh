---
status: complete
phase: 21
plan: 01
---

# Summary: Phase 21 Plan 01

Added a retained render-object scene-graph boundary.

## Changed

- Added `crates/core/frontend/render/src/render_object.rs`.
- Exported `RenderObjectTree` and `RenderObjectDirtySummary` from `mesh-core-render`.
- Added a retained render-object tree to `FrontendSurfaceComponent`.
- Synchronized render objects during paint and traced dirty-slot summaries.

## Verification

- `cargo fmt --check`
- `cargo test -p mesh-core-render render_object_tree_preserves_identity_and_reports_slot_diffs -- --nocapture`
- `nix develop -c cargo test -p mesh-core-shell typed_invalidations -- --nocapture`

