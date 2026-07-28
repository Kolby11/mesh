---
phase: 16-debug-only-profiling-mode-and-live-inspector
plan: 01
subsystem: debug
tags: [profiling, inspector, mesh.debug, shell, service-contract]
requires:
  - phase: 14-profiling-data-model-and-timing-hooks
    provides: debug-only profiling sessions and typed profiling snapshots
  - phase: 15-backend-and-surface-attribution
    provides: deterministic per-surface and per-backend profiling attribution
provides:
  - canonical `mesh.debug` interface contract for `.mesh` consumers
  - stable inspector-facing debug view identifiers and shell-owned `mesh.debug` service state
  - explicit frontend debug overlay and profiling control event mappings
affects: [phase-16-inspector-module, phase-17-benchmarks, debug-overlay]
tech-stack:
  added: []
  patterns: [shell-owned service state backfill, inspector-oriented debug view ids]
key-files:
  created: [modules/interfaces/debug.toml]
  modified:
    [
      crates/core/foundation/debug/src/lib.rs,
      crates/core/shell/src/shell/runtime/debug.rs,
      crates/core/shell/src/shell/service.rs,
      crates/core/shell/src/shell/tests.rs
    ]
key-decisions:
  - "Published `mesh.debug` through `latest_service_state` with `@mesh/core-debug` as the shell-owned provider id."
  - "Kept legacy `DebugTab` compatibility for the native renderer while adding stable inspector-facing `overview`, `surfaces`, `backend_services`, and `benchmark` identifiers."
patterns-established:
  - "Debug inspector consumers should read shell-owned state from `mesh.debug` instead of reaching into private native debug paths."
  - "Overlay visibility and profiling collection stay independent in both requests and published service payloads."
requirements-completed: [PROF-01, INSP-01]
duration: 20min
completed: 2026-05-08
---

# Phase 16 Plan 01: Debug Inspector State Contract and Frontend API Surface Summary

**Read-only `mesh.debug` service state, inspector-facing view ids, and explicit shell debug control events for the Phase 16 inspector path**

## Performance

- **Duration:** 20 min
- **Started:** 2026-05-08T18:07:24Z
- **Completed:** 2026-05-08T18:27:24Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- Added the canonical `modules/interfaces/debug.toml` contract with `service.debug.read` and the required shell debug state fields.
- Extended shared debug types with stable inspector-oriented view identifiers while preserving legacy native debug tab compatibility.
- Backfilled shell-owned `mesh.debug` state from `build_debug_snapshot()`, exposed explicit frontend debug control events, and locked the behavior with focused shell regressions.

## Task Commits

1. **Task 1: Add the canonical read-only `mesh.debug` interface contract and inspector-oriented debug view identifiers** - `54db9fc` (`feat`)
2. **Task 2: Publish shell-owned `mesh.debug` state and explicit frontend debug-control events** - `fc15352` (`feat`)

## Files Created/Modified

- `modules/interfaces/debug.toml` - Canonical read-only `mesh.debug` contract for `.mesh` inspector consumers.
- `crates/core/foundation/debug/src/lib.rs` - Shell debug constants plus stable inspector view identifiers.
- `crates/core/shell/src/shell/runtime/debug.rs` - `mesh.debug` payload backfill from the shell snapshot path.
- `crates/core/shell/src/shell/service.rs` - Frontend event mappings for overlay and profiling debug controls.
- `crates/core/shell/src/shell/tests.rs` - Regression coverage for `mesh.debug` payload shape and overlay/profiling independence.

## Decisions Made

- Published the inspector-facing debug payload through the existing `latest_service_state` mechanism so built-in and future `.mesh` consumers share one shell-owned path.
- Added `DebugInspectorView` alongside the legacy tab enum instead of replacing the legacy native renderer model in this plan, which keeps Phase 16-01 compatible with the current runtime while preparing the inspector API surface.

## Deviations from Plan

### Auto-fixed Issues

None.

### Ownership Constraint

- **Issue:** Standard GSD execution flow would also update `.planning/STATE.md`, `.planning/ROADMAP.md`, and `.planning/REQUIREMENTS.md`.
- **Resolution:** Left those files untouched because the task explicitly limited ownership to the five source files and this summary file.
- **Impact on plan:** No impact on the delivered code or verification; only execution metadata updates were deferred.

## Issues Encountered

- The first focused shell test run failed because the new JSON helpers referenced profiling snapshot types without fully qualifying them from `mesh_core_debug`.
- Qualifying those types in `runtime/debug.rs` resolved the build failure, and the rerun passed.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `.mesh` inspector work can now consume `@mesh/debug@>=1.0` through the shell-owned `mesh.debug` state surface.
- Phase 16-02 can build the shipped inspector module against stable view ids and explicit overlay/profiling control events.

## Self-Check: PASSED
