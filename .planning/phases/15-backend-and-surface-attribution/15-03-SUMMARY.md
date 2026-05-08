---
phase: 15-backend-and-surface-attribution
plan: 03
subsystem: backend
tags: [profiling, backend, surface, rust, shell, diagnostics]
requires:
  - phase: 15-backend-and-surface-attribution
    provides: provider-attributed backend poll/update and command profiling from plans 15-01 and 15-02
provides:
  - accepted backend state publish/delivery attribution at the service-event fanout seam
  - deterministic backend and per-surface profiling snapshot ordering in debug output
  - stable per-surface module attribution even when stage metadata is incomplete
affects: [phase-16-inspector, phase-17-benchmarks, backend-attribution, surface-attribution]
tech-stack:
  added: []
  patterns: [accepted-update-only backend attribution, deterministic profiling snapshot ordering, surface-id keyed rollup stabilization]
key-files:
  created:
    - .planning/phases/15-backend-and-surface-attribution/15-03-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/runtime/debug.rs
    - crates/core/shell/src/shell/runtime/profiling.rs
    - crates/core/shell/src/shell/runtime/render.rs
    - crates/core/shell/src/shell/runtime/service_state.rs
    - crates/core/shell/src/shell/tests.rs
key-decisions:
  - "StatePublishDelivery is recorded only after record_latest_service_state(...) accepts the update and component fanout completes."
  - "Debug snapshots sort profiling surfaces by surface_id and backends by (interface, provider_id) even if collector ordering changes."
  - "Empty module_id stage metadata is normalized away so later valid surface attribution can still populate the snapshot."
patterns-established:
  - "Backend publish/delivery attribution belongs at the accepted service-event seam, not on stale or terminal-provider paths."
  - "Per-surface snapshots must retain canonical surface_id keys and recover module attribution from later valid records."
requirements-completed: [TIME-02, BACK-02]
duration: 2min
completed: 2026-05-08
---

# Phase 15 Plan 03: Publish Delivery and Per-Surface Rollup Stability Summary

**Accepted backend service updates now record publish/delivery timing while debug snapshots keep deterministic backend ordering and stable per-surface module attribution**

## Performance

- **Duration:** 2 min
- **Started:** 2026-05-08T17:42:12Z
- **Completed:** 2026-05-08T17:44:26Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- Recorded backend `StatePublishDelivery` samples only for accepted service updates, covering latest-state validation and component fanout in one timing seam.
- Stabilized profiling snapshots so backend summaries sort deterministically and per-surface rollups retain canonical `surface_id` keys with recoverable `module_id` attribution.
- Added shell profiling regressions for accepted versus stale publish/delivery attribution, empty module-id recovery, and coexistence of shell, surface, and backend profiling views.

## Task Commits

Each task was committed atomically:

1. **Task 1: Record backend state publish and delivery timing around accepted service-event fanout** - `27c34cb` (feat)
2. **Task 2: Lock stable per-surface rollups and snapshot ordering beside backend attribution** - `3e32ab6` (feat)

## Files Created/Modified

- `crates/core/shell/src/shell/runtime/service_state.rs` - Records publish/delivery profiling only after a service update is accepted and fanout completes.
- `crates/core/shell/src/shell/runtime/profiling.rs` - Adds an explicit `StatePublishDelivery` helper and normalizes empty surface module ids before they enter the collector.
- `crates/core/shell/src/shell/runtime/render.rs` - Falls back from empty stage `module_id` values to the component id so surface attribution remains stable.
- `crates/core/shell/src/shell/runtime/debug.rs` - Applies explicit deterministic ordering to profiling surfaces and backends in debug snapshots.
- `crates/core/shell/src/shell/tests.rs` - Adds publish/delivery, module-id recovery, and stable snapshot ordering regressions.
- `.planning/phases/15-backend-and-surface-attribution/15-03-SUMMARY.md` - Captures execution results for the plan.

## Decisions Made

- Publish/delivery timing is measured once around the accepted update path instead of splitting validation and fanout into separate backend counters.
- Snapshot ordering is enforced at debug-snapshot assembly time so inspector consumers see deterministic backend and surface arrays regardless of collector internals.
- Empty stage-level module metadata is treated as absent data rather than authoritative attribution.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- A transient `.git/index.lock` blocked one staging attempt during Task 2. The lock disappeared before retry, and the commit proceeded without modifying unrelated files.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 16 can render backend publish/delivery timings beside shell and per-surface views from one deterministic profiling snapshot.
- Phase 17 benchmark work now has stable surface and backend attribution inputs without stale-update noise or missing module identifiers.

## Self-Check: PASSED

- Found `.planning/phases/15-backend-and-surface-attribution/15-03-SUMMARY.md`.
- Found task commit `27c34cb`.
- Found task commit `3e32ab6`.
