---
phase: 05-backend-diagnostics-and-mvp-proof
plan: 01
subsystem: runtime
tags: [luau, mlua, backend, diagnostics, lifecycle, error-handling]

# Dependency graph
requires:
  - phase: 04-service-provider-contract
    provides: BackendScriptContext, spawn_backend_service, BackendServiceEvent lifecycle events
provides:
  - Stage-aware BackendScriptError variants (SnapshotFailed, CommandResultConversionFailed)
  - spawn_backend_service emits stage="snapshot" for state serialization failures
  - spawn_backend_service emits stage="command-result" for handler return value conversion failures
  - Runtime proofs for missing-entrypoint and unsupported-command diagnostics
affects: [05-02, 05-03, 05-04, shell-diagnostics, health-reporting]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Stage-aware error variants pattern: BackendScriptError carries SnapshotFailed and CommandResultConversionFailed as distinct variants for clean diagnostic bucketing
    - Lifecycle stage dispatch: backend runtime inspects error variant to pick stage string for Failed events

key-files:
  created: []
  modified:
    - crates/core/runtime/scripting/src/backend.rs
    - crates/core/runtime/backend/src/lib.rs

key-decisions:
  - "SnapshotFailed and CommandResultConversionFailed are distinct BackendScriptError variants (not Runtime sub-fields) so pattern matching at the runtime level is clean and exhaustive."
  - "spawn_backend_service maps SnapshotFailed to stage='snapshot' and CommandResultConversionFailed to stage='command-result' without service-name branches."
  - "Unsupported commands produce a generic ok=false CommandResult without emitting a Failed lifecycle event; handler Lua errors produce both a CommandResult and a Failed event."

patterns-established:
  - "Stage string dispatch: match on BackendScriptError variant to select stage string before emitting Failed events — keeps Rust core generic and avoids service-specific branches."

requirements-completed: [BDIAG-01, BDIAG-02]

# Metrics
duration: 3min
completed: 2026-05-04
---

# Phase 5 Plan 01: Backend Diagnostics Failure Boundary Summary

**Stage-aware BackendScriptError variants and targeted runtime proofs make snapshot, command-result, and missing-entrypoint failures distinguishable in the backend lifecycle without service-specific Rust logic.**

## Performance

- **Duration:** ~3 min
- **Started:** 2026-05-04T17:12:00Z
- **Completed:** 2026-05-04T17:15:30Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments
- Added `SnapshotFailed` and `CommandResultConversionFailed` variants to `BackendScriptError` so diagnostic consumers can bucket errors cleanly
- Updated `take_service_state_snapshot()` and `command_result_from_lua()` to use stage-specific error variants
- Updated `spawn_backend_service()` to dispatch `stage="snapshot"` and `stage="command-result"` in `Failed` events by matching on error variant
- Added 5 new targeted tests covering all required diagnostic paths (snapshot failure, command result conversion, runtime error outcome, missing entrypoint, unsupported command)

## Task Commits

1. **Task 1: Tighten backend snapshot and command failure boundaries** - `13bb062` (feat)
2. **Task 2: Preserve generic backend lifecycle events for every failure path** - `e86a764` (feat)
3. **Task 3: Add targeted runtime proof for missing entrypoint and unsupported command diagnostics** - `e7cf402` (feat)

## Files Created/Modified
- `crates/core/runtime/scripting/src/backend.rs` - Added SnapshotFailed and CommandResultConversionFailed error variants; updated take_service_state_snapshot and command_result_from_lua; added 4 new tests
- `crates/core/runtime/backend/src/lib.rs` - Imported BackendScriptError; added stage-aware dispatch in Err arm; added 3 new tests

## Decisions Made
- Used distinct error variants rather than a generic `Runtime { stage }` field so pattern-match exhaustiveness checking catches future gaps.
- Unsupported commands stay non-fatal (no `Failed` event) to match the existing `run_command_with_result` design where `Ok(BackendCommandOutcome)` is returned for missing handlers.
- `SnapshotFailed` message prefix `"failed to export state snapshot"` is preserved in the `Failed` event message so log scanners can grep for it without parsing stage strings.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 5 plans 02-04 can rely on stage-aware `Failed` events to implement stale-state clearing and health degradation without revisiting the scripting or backend crates.
- `BDIAG-01` (clear diagnostics for all failure paths) and `BDIAG-02` (graceful degradation without shell crash) are proven by the 5 new tests plus the existing 83 passing tests across both crates.

## Self-Check: PASSED
- `crates/core/runtime/scripting/src/backend.rs` exists and contains all required tests
- `crates/core/runtime/backend/src/lib.rs` exists and contains all required tests
- Commits 13bb062, e86a764, e7cf402 exist in git log
- 73 tests pass in mesh-core-scripting, 15 tests pass in mesh-core-backend

---
*Phase: 05-backend-diagnostics-and-mvp-proof*
*Completed: 2026-05-04*
