---
gsd_state_version: 1.0
milestone: v1.21
milestone_name: Retained Layout & Display List
status: executing
stopped_at: Roadmap created for v1.21; Phase 104 ready for planning
last_updated: "2026-06-18T15:49:32.140Z"
last_activity: 2026-06-18 -- Phase 104 planning complete
progress:
  total_phases: 7
  completed_phases: 0
  total_plans: 3
  completed_plans: 0
  percent: 0
---

# State: MESH v1.21

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-06-18)

**Core value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.
**Current focus:** v1.21 Retained Layout & Display List — Phase 104 next

## Current Position

Phase: 104 (not started)
Plan: —
Status: Ready to execute
Last activity: 2026-06-18 -- Phase 104 planning complete

```
Progress: [░░░░░░░░░░░░░░░░░░░░] 0% (0/3 phases)
```

## Performance Metrics

| Metric | Value |
|--------|-------|
| Requirements defined | 11 |
| Requirements mapped | 11/11 |
| Phases defined | 3 |
| Phases complete | 0 |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table. v1.20 decisions archived in milestones/v1.20-ROADMAP.md.

### Architecture Notes (v1.21)

- `PerSurfaceLayoutState` belongs in `layout.rs` (`mesh-core-elements`) or on `FrontendSurfaceComponent`
- `RopeNode` enum belongs in `display_list.rs` (`mesh-core-render`)
- `ProfilingStage::LayoutRetained` belongs in `mesh-core-debug`
- `rpds 1.2.0` workspace dep needed for rope phase (if `rpds::Vector` is used); `profiling 1.0.17` for profiling phase
- `_mesh_key` (not `TaffyNodeId`) is the stable retained-map key — critical design constraint
- `remove_taffy_subtree` must post-order walk descendants; Taffy does not recursively remove
- Profiling timer acquisition (`Instant::now()`) must be gated behind `profiling_enabled` flag — not just the recording step

### Blockers/Concerns

None.

## Deferred Items

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| debug | phase31-live-uat-diagnosis | updated | v1.5 |
| todo | 2026-05-15-define-module-install-requirement-resolution.md | pending | v1.8 |
| todo | 2026-05-13-phase31-audio-popover-transition-delay.md | pending | v1.5 |
| todo | mesh.debug.scheduler structured payload (DIAG-02) | deferred | v1.19 |
| todo | widget-level opaque rect analysis (OPAQUE-02) | deferred | v1.19 |

## Session Continuity

Last session: 2026-06-18
Stopped at: Roadmap created for v1.21; Phase 104 ready for planning
Resume file: None
