---
status: complete
phase: 19
plan: 01
---

# Summary: Phase 19 Plan 01

Implemented typed dirty invalidation observability for debug profiling.

## Changed

- Added `ProfilingInvalidationSnapshot`, `ComponentInvalidationCounts`, and `RetainedInvalidationCounts` to `mesh-core-debug`.
- Added a `ShellComponent::take_invalidation_snapshot` hook so frontend components can hand their last paint invalidation data back to the shell runtime.
- Recorded per-surface invalidation snapshots in `ProfilingRuntimeState`.
- Serialized invalidation data under `mesh.debug.profiling.surfaces[].invalidation`.
- Converted retained widget-tree dirty summaries and component dirty flags into debug-facing counts.

## Verification

- `cargo fmt --check`
- `cargo test -p mesh-core-shell typed_invalidations -- --nocapture`
- `cargo test -p mesh-core-shell retained_widget_tree_reports_dirty_categories_by_stable_id -- --nocapture`
- `cargo test -p mesh-core-shell profiling_snapshot_exposes_typed_surface_invalidation_counts -- --nocapture`
