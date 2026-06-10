---
gsd_state_version: 1.0
milestone: v1.20
milestone_name: "Compositor Integration"
status: planning
last_updated: "2026-06-10T00:00:00.000Z"
last_activity: 2026-06-10 -- Milestone v1.20 started
progress:
  total_phases: 0
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# State: MESH v1.20

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-06-10)

**Core value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.
**Current focus:** v1.20 Compositor Integration — defining requirements

## Current Position

Phase: Not started (defining requirements)
Plan: —
Status: Defining requirements
Last activity: 2026-06-10 — Milestone v1.20 started

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.

- [v1.19]: Blocking `poll()` on Wayland fd replaces `std::thread::sleep` — zero new crate dependencies
- [v1.19]: eventfd wakes blocking poll on backend/IPC messages — sequential polling, not epoll
- [v1.19]: `supports_blocking_dispatch()` gate preserves dev-window sleep path
- [v1.19]: Conservative opaque region — root background only, alpha==255, no effects, no rounded corners

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

Last session: 2026-06-10T00:00:00.000Z
Stopped at: Milestone v1.20 initialized — requirements definition in progress
Resume file: None
