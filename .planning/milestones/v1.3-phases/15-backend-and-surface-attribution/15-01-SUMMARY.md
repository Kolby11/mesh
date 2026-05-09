---
phase: 15-backend-and-surface-attribution
plan: 01
subsystem: api
tags: [profiling, debug, backend, rust, shell]
requires:
  - phase: 14-profiling-data-model-and-timing-hooks
    provides: rolling shell and per-surface profiling snapshots plus bounded collector storage
provides:
  - typed backend profiling snapshot contracts with explicit backend stages
  - bounded backend accumulators keyed by interface and provider identity
  - shell regression coverage for disabled, reset, and bounded backend profiling behavior
affects: [phase-16-inspector, phase-17-benchmarks, backend-attribution]
tech-stack:
  added: []
  patterns: [typed debug snapshot contracts, bounded profiling collectors, shell-level profiling regression tests]
key-files:
  created:
    - .planning/phases/15-backend-and-surface-attribution/15-01-SUMMARY.md
  modified:
    - crates/core/foundation/debug/src/lib.rs
    - crates/core/shell/src/shell/runtime/profiling.rs
    - crates/core/shell/src/shell/tests.rs
key-decisions:
  - "Backend profiling data lives in ProfilingSnapshot as typed backend summaries rather than overloading backend_runtimes lifecycle entries."
  - "Backend attribution reuses the existing bounded profiling collector with a deterministic (interface, provider_id) key."
patterns-established:
  - "Profiling backend summaries mirror the existing stage-summary shape: stage, counts, totals, max, bounded recent samples."
  - "Backend profiling helpers must stay inert when debug profiling is disabled and reset cleanly on new sessions."
requirements-completed: [TIME-02, BACK-01]
duration: 4min
completed: 2026-05-08
---

# Phase 15 Plan 01: Backend Profiling Contract and Collector Extension Summary

**Typed backend profiling summaries for interface/provider attribution with explicit poll, command, and publish stages in the existing bounded debug profiler**

## Performance

- **Duration:** 4 min
- **Started:** 2026-05-08T17:22:02Z
- **Completed:** 2026-05-08T17:26:25Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- Extended `mesh-core-debug` with typed backend profiling snapshots and explicit backend stage identifiers.
- Added bounded backend accumulator storage keyed by `(interface, provider_id)` inside the existing shell profiling collector.
- Added shell profiling regression tests proving backend payloads are disabled when profiling is off, cleared on session reset, and bounded when active.

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend the debug profiling contract with explicit backend provider and stage snapshots** - `c658471` (feat)
2. **Task 2: Extend the bounded profiling collector and debug snapshot assembly for backend summaries** - `e37cad6` (feat)

## Files Created/Modified

- `crates/core/foundation/debug/src/lib.rs` - Added typed backend profiling snapshot, stage summary, sample, and stage enum definitions.
- `crates/core/shell/src/shell/runtime/profiling.rs` - Added bounded backend accumulators and shell helper methods for backend profiling samples.
- `crates/core/shell/src/shell/tests.rs` - Added backend profiling regression tests for disabled, reset, keyed, and bounded behavior.
- `.planning/phases/15-backend-and-surface-attribution/15-01-SUMMARY.md` - Captures execution results for the plan.

## Decisions Made

- Backend profiling data is emitted through `DebugSnapshot.profiling` only; lifecycle-oriented `backend_runtimes` remains separate.
- Backend samples use backend-specific typed stages while preserving the existing shell and per-surface profiling contract unchanged.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 16 can consume the new `ProfilingSnapshot.backends` payload without inventing a second backend diagnostics transport.
- Backend timing attribution is now typed, bounded, and keyed by concrete provider identity for future inspector rendering.

## Self-Check: PASSED

- Found `.planning/phases/15-backend-and-surface-attribution/15-01-SUMMARY.md`.
- Found task commit `c658471`.
- Found task commit `e37cad6`.
