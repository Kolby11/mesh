---
phase: 15-backend-and-surface-attribution
plan: 02
subsystem: backend
tags: [profiling, backend, rust, shell, diagnostics]
requires:
  - phase: 15-backend-and-surface-attribution
    provides: typed backend profiling snapshots and bounded per-provider collectors from plan 15-01
provides:
  - backend poll/update attribution wired through shell message delivery with interface and provider identity
  - backend command-handling attribution keyed to the active provider at service command dispatch
  - shell regression tests covering accepted update attribution, stale-update silence, command attribution, and disabled profiling inertness
affects: [phase-16-inspector, phase-17-benchmarks, backend-attribution]
tech-stack:
  added: []
  patterns: [provider-attributed shell message handling, profiling-gated backend command dispatch]
key-files:
  created:
    - .planning/phases/15-backend-and-surface-attribution/15-02-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/backend/spawn.rs
    - crates/core/shell/src/shell/runtime/mod.rs
    - crates/core/shell/src/shell/runtime/request.rs
    - crates/core/shell/src/shell/runtime/service_state.rs
    - crates/core/shell/src/shell/tests.rs
    - crates/core/shell/src/shell/types.rs
key-decisions:
  - "Accepted backend update attribution is recorded in the shell message drain after stale-provider filtering, so rejected updates stay silent."
  - "Command-handling attribution resolves provider identity from the active runtime slot instead of inferring it from the caller or contract registry."
patterns-established:
  - "Backend-originated service updates should carry explicit interface/provider metadata through ShellMessage before profiling or delivery."
  - "Backend command attribution should reuse the existing dispatch path and profiling gate rather than creating a parallel backend API."
requirements-completed: [BACK-01, BACK-02]
duration: 5min
completed: 2026-05-08
---

# Phase 15 Plan 02: Backend Update and Command Attribution Hooks Summary

**Provider-attributed backend poll/update and command-handling timings now flow through the shell profiler without adding work when debug profiling is off**

## Performance

- **Duration:** 5 min
- **Started:** 2026-05-08T17:31:59Z
- **Completed:** 2026-05-08T17:36:59Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- Routed backend-originated service updates through a provider-aware shell message so accepted update work records `PollUpdate` against concrete `(interface, provider_id)` pairs.
- Added a reusable shell message handler and separated accepted service-event delivery from stale-update rejection so profiling only captures accepted backend work.
- Extended service command dispatch to record `CommandHandling` samples for the active provider and proved both enabled and disabled profiling behavior with shell tests.

## Task Commits

Each task was committed atomically:

1. **Task 1: Record backend poll and update attribution at the backend event bridge and shell message drain** - `95f0968` (feat)
2. **Task 2: Record backend command-handling attribution at service command dispatch** - `044728d` (feat)

## Files Created/Modified

- `crates/core/shell/src/shell/backend/spawn.rs` - Preserves backend provider metadata when service updates cross into the shell.
- `crates/core/shell/src/shell/runtime/mod.rs` - Centralizes shell message handling and records accepted backend `PollUpdate` samples at the message drain.
- `crates/core/shell/src/shell/runtime/service_state.rs` - Separates latest-state acceptance from component delivery so accepted backend work can be profiled once.
- `crates/core/shell/src/shell/runtime/request.rs` - Records backend `CommandHandling` samples on both direct and throttled service command dispatch.
- `crates/core/shell/src/shell/tests.rs` - Adds backend update and command attribution regressions, including disabled-mode silence.
- `crates/core/shell/src/shell/types.rs` - Adds the provider-aware backend service update shell message variant.
- `.planning/phases/15-backend-and-surface-attribution/15-02-SUMMARY.md` - Captures execution results for the plan.

## Decisions Made

- Poll/update attribution is attached after `record_latest_service_state(...)` accepts the event, so stale or terminal-provider updates do not pollute backend summaries.
- Command attribution records against the active runtime slot provider and reuses the existing bounded profiling collector, keeping disabled profiling fully inert.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Backend update and command stages now appear with concrete provider identity in profiling snapshots, so later inspector work can render actionable backend buckets directly.
- The shell now has a reusable provider-aware backend message seam for any remaining backend stage attribution work in subsequent Phase 15 plans.

## Self-Check: PASSED

- Found `.planning/phases/15-backend-and-surface-attribution/15-02-SUMMARY.md`.
- Found task commit `95f0968`.
- Found task commit `044728d`.
