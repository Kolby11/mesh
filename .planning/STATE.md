---
gsd_state_version: 1.0
milestone: v1.21
milestone_name: Retained Layout & Display List
status: shell suite 374 pass / 9 fail; fixing regressions from inline-popover migration commit
stopped_at: context exhaustion at 76% (2026-06-29)
last_updated: "2026-06-29T21:20:00.000Z"
last_activity: 2026-06-29 -- fixed InstanceBinding validation + closed-popover placeholder; 9 nav/theme failures remain
progress:
  total_phases: 3
  completed_phases: 1
  total_plans: 3
  completed_plans: 3
  percent: 33
---

# State: MESH v1.21

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-06-18)

**Core value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.
**Current focus:** Phase 104 — Retained TaffyTree

## Current Position

Phase: 104 (Retained TaffyTree) — GAP CLOSURE (7 suspects remain)
Plan: 3 of 3
Status: shell suite 54->7 failing; remaining 7 documented as regression suspects
Last activity: 2026-06-22 -- gap-closure pass; fixture/harness drift from shipped-module rewrites resolved

```
Progress: [████████████░░░░░░░░] 33% impl; shell suite 347 pass / 7 fail (was 54 fail)
```

## Shell Gap-Closure Pass (2026-06-22)

Took `nix develop -c cargo test --package mesh-core-shell --lib` from 54 -> 7
failing. Fixed all fixture/harness drift from the shipped navigation-bar /
audio-popover rewrites (embed-handler keys, missing child-component + interface

+ i18n registration in the test harness, debug-inspector seed-flow, vertical

slider geometry, deprecated keybind migration, icons.toml completion, obsolete
test deletion). Also fixed a real product bug: audio popover slider stuck at 0
(`value={expr}` must be quoted `value="{var}"`).

The remaining 7 are behavior-level **regression suspects** (not fixtures),
documented with a triage table in `104-VERIFICATION.md`: retained narrow-diff
counts (2), service-observation repaint gating (1), live element refs metrics
(1), container-query restyle (1), profiling raster proof (1), settings-loader
display_transition default (1). Recommend a focused `gsd:debug` pass starting
with narrow-diff + service-observation.

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

- Phase 104 full shell verification is blocked by current dirty-tree navigation/module/service test failures. Focused retained-layout tests pass and `mesh-core-shell` builds under Nix; full `nix develop -c cargo test --package mesh-core-shell` fails 54 tests against changed shipped surfaces/module graph. See `.planning/phases/104-retained-taffytree/104-VERIFICATION.md`.

## Deferred Items

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| debug | phase31-live-uat-diagnosis | updated | v1.5 |
| todo | 2026-05-15-define-module-install-requirement-resolution.md | pending | v1.8 |
| todo | 2026-05-13-phase31-audio-popover-transition-delay.md | pending | v1.5 |
| todo | mesh.debug.scheduler structured payload (DIAG-02) | deferred | v1.19 |
| todo | widget-level opaque rect analysis (OPAQUE-02) | deferred | v1.19 |

## Session Continuity

Last session: 2026-06-29T20:42:17.055Z
Stopped at: context exhaustion at 75% (2026-06-29)
Resume file: None
