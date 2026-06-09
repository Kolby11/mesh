# Plan 97-02 Summary: NodeServiceFieldDependencies + FrontendSurfaceComponent wiring

**Completed:** 2026-06-09
**Status:** Done

## What Was Done

- Added `NodeServiceFieldDependencies` struct in `runtime_tree.rs` with `build()`, `nodes_reading_field()`, and `fields_read_by_node()` methods
- Bidirectional index: forward `HashMap<NodeId, HashSet<(String, String)>>` + reverse `HashMap<(String, String), HashSet<NodeId>>`
- `collect_node_service_deps` recursive helper walks the full tree after annotation
- Added `node_service_field_deps: NodeServiceFieldDependencies` field to `FrontendSurfaceComponent` in `component.rs`, initialised with `Default::default()`
- Added `NodeServiceFieldDependencies` to `component.rs` imports from `runtime_tree`
- In `finalize_tree` (rendering.rs), added guard: rebuild index only when `trigger_kind == "rebuild"`, skipping the restyle path

## Test Results

- `node_service_field_deps_forward_lookup` — passes
- `node_service_field_deps_reverse_lookup` — passes
- `node_service_field_deps_empty_node_not_in_forward` — passes
- `node_service_field_deps_unknown_field_empty` — passes
- `mesh-core-shell`: 276 passed (vs 272 baseline before changes) — 36 pre-existing failures unchanged
- Rebuild guard verified: `node_service_field_deps` appears only inside `trigger_kind == "rebuild"` block

## Requirements Satisfied

- SRV-02: Bidirectional `NodeServiceFieldDependencies` supports O(1) queries in both directions
