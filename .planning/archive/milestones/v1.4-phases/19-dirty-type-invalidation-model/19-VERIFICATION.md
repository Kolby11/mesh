---
status: passed
phase: 19
---

# Verification: Phase 19

Status: passed

## Evidence

- `DIRTY-01`: Component and retained dirty categories are available in the debug profiling snapshot.
- `DIRTY-02`: Existing `ComponentDirtyFlags` routing continues to distinguish retained style/layout/paint paths from full rebuild paths.
- `DIRTY-03`: Script and text invalidations still require the full rebuild fallback.
- `DIRTY-04`: Retained dirty summaries preserve generation and node-category context; detailed previous/next bounds are deferred to Phase 22 damage tracking.

## Commands

- `cargo fmt --check`
- `cargo test -p mesh-core-shell typed_invalidations -- --nocapture`
- `cargo test -p mesh-core-shell retained_widget_tree_reports_dirty_categories_by_stable_id -- --nocapture`
- `cargo test -p mesh-core-shell profiling_snapshot_exposes_typed_surface_invalidation_counts -- --nocapture`
