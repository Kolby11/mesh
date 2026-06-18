# Requirements: MESH

**Defined:** 2026-06-18
**Milestone:** v1.21 Retained Layout & Display List
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1.21 Requirements

### Retained TaffyTree

- [ ] **LAYOUT-01**: `TaffyTree` and `_mesh_key → TaffyNodeId` map are retained per surface and mutated in place across frames instead of rebuilt from scratch each layout pass
- [ ] **LAYOUT-02**: STYLE-only dirty nodes call `set_style` without geometry invalidation; LAYOUT-dirty nodes call `mark_dirty` / `set_children` to propagate geometry invalidation
- [ ] **LAYOUT-03**: Structural changes (node add/remove/reorder) use `_mesh_key` as the stable identity key — not the ephemeral `TaffyNodeId` — so `TREE_REBUILD` frames never serve stale geometry
- [ ] **LAYOUT-04**: `remove_taffy_subtree` performs a post-order walk to remove all descendants before removing a parent (Taffy does not recursively remove)
- [ ] **LAYOUT-05**: Layout output is pixel-equivalent to the current per-frame rebuild approach across style-only, layout-dirty, and tree-rebuild dirty scenarios

### Rope Display List

- [ ] **ROPE-01**: `RopeNode` enum references existing `Arc<[DisplayPaintCommand]>` slices for clean subtrees instead of byte-copying them into parent vectors on each dirty update
- [ ] **ROPE-02**: Final flat-array assembly pass is preserved so damage-rect queries continue working against a contiguous array without API changes
- [ ] **ROPE-03**: Scroll offset coordinates in rope segments are stored layout-relative to prevent stale absolute positions when scrollable content is partially dirty

### Per-stage Budget Profiling

- [ ] **PERF-01**: `ProfilingStage::LayoutRetained` variant added; `Instant::now()` acquisition is gated behind `profiling_enabled` (not just the recording step) so release builds pay nothing
- [ ] **PERF-02**: Per-stage budget constants defined alongside stage records; `tracing::warn!` emitted on overrun in debug builds
- [ ] **PERF-03**: Baseline measurements captured before and after retention changes on canonical workloads (hover, backend update, slider drag, surface open, clock tick) to confirm improvement

## Future Requirements

### Retained Display List — deferred

- **ROPE-F01**: `rpds::Vector` rope index for sub-linear span insertion in very deep trees (current `Vec<RopeNode>` is sufficient for typical shell surface depth)
- **ROPE-F02**: Parallel subtree paint collection using rayon across independent surface subtrees

### Profiling — deferred

- **PERF-F01**: puffin or Tracy backend integration as an opt-in dev feature gate (pulls in a TCP server — never in workspace.dependencies)
- **PERF-F02**: Capture/replay profiling sessions for offline analysis

## Out of Scope

| Feature | Reason |
|---------|--------|
| Full GPU renderer / Vello backend | Renderer migration is a separate milestone track |
| Myers-diff structural reconciliation | `_mesh_key` stable IDs make keyed diffing sufficient; full shadow-tree diffing is browser-engine scope |
| Parallel layout computation | Taffy's single-tree model does not support parallel subtree layout; deferred until Taffy exposes a parallel API |
| Accessible-tree (AccessKit) retained updates | Already behind `renderer-accesskit` feature gate; separate milestone |
| Compositor protocol changes | v1.20 closed damage/HiDPI/blur; no new Wayland protocol work in v1.21 |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| LAYOUT-01 | Phase 104 | Pending |
| LAYOUT-02 | Phase 104 | Pending |
| LAYOUT-03 | Phase 104 | Pending |
| LAYOUT-04 | Phase 104 | Pending |
| LAYOUT-05 | Phase 104 | Pending |
| ROPE-01 | Phase 105 | Pending |
| ROPE-02 | Phase 105 | Pending |
| ROPE-03 | Phase 105 | Pending |
| PERF-01 | Phase 106 | Pending |
| PERF-02 | Phase 106 | Pending |
| PERF-03 | Phase 106 | Pending |

**Coverage:**
- v1.21 requirements: 11 total
- Mapped to phases: 11 (100%)
- Unmapped: 0

---
*Requirements defined: 2026-06-18*
*Last updated: 2026-06-18 — traceability populated by roadmapper*
