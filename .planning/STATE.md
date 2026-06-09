---
gsd_state_version: 1.0
milestone: v1.19
milestone_name: "Performance: Event-Driven Frame Scheduler"
status: roadmap_created
completed_at: —
last_updated: "2026-06-09T19:00:00.000Z"
last_activity: 2026-06-09 -- ROADMAP.md created for v1.19
progress:
  total_phases: 2
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# State: MESH v1.19

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-06-09)

**Core value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.
**Current focus:** v1.19 Performance: Event-Driven Frame Scheduler

## Current Position

 Phase: 100 of 100 (Opaque Region Hints — second of two phases, Phase 99 planned, Phase 100 planned)
Plan: —
Status: Phases 99-100 planned, ready to execute
Last activity: 2026-06-09 — Phase 100 plans (2 plans) created; 6/6 v1.19 requirements planned

Progress: [░░░░░░░░░░] 0%

## Performance Metrics

**Velocity:**
- Total plans completed: 0
- Average duration: —
- Total execution time: —

**By Phase:**
| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

*No plans executed yet for v1.19.*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.

No v1.19-specific decisions yet. Research (`research/SUMMARY.md`) confirmed:
- Zero new crate dependencies needed
- `dispatch_available()` already implements prepare_read/poll/read/dispatch with 0ms timeout — needs deadline parameterization
- eventfd for IPC wakeup to prevent stale-frame latency
- Dev-window backend must preserve its existing sleep path
- Opaque region computation must walk retained display list for background-fill alpha

### Pending Todos

- `/todos/pending/2026-05-15-define-module-install-requirement-resolution.md` (open)

### Blockers/Concerns

None.

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| debug | phase31-live-uat-diagnosis | updated | v1.5 |
| todo | 2026-05-15-define-module-install-requirement-resolution.md | pending | v1.8 |
| todo | 2026-05-13-phase31-audio-popover-transition-delay.md | pending | v1.5 |

## Session Continuity

Last session: 2026-06-09 — roadmap creation
Stopped at: ROADMAP.md, STATE.md, REQUIREMENTS.md traceability written for v1.19
Resume file: None
