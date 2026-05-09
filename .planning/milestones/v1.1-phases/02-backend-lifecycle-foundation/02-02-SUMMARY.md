---
phase: 02-backend-lifecycle-foundation
plan: 02
subsystem: backend-runtime
tags: [rust, tokio, luau, lifecycle-events, polling]
requires:
  - phase: 01-plugin-package-manifest-foundation
    provides: backend provider module metadata
provides:
  - typed backend lifecycle events
  - init failure and poll failure runtime reporting
  - poll failure threshold stop behavior
affects: [backend lifecycle, shell runtime bridge, diagnostics]
tech-stack:
  added: []
  patterns: [typed lifecycle event enum for backend runtime outcomes]
key-files:
  created: []
  modified:
    - crates/core/runtime/backend/src/lib.rs
    - crates/core/runtime/scripting/src/backend.rs
key-decisions:
  - "Backend poll and command handler failures now return typed scripting errors instead of being swallowed as no payload."
  - "Three consecutive poll failures stop the runtime without automatic restart."
patterns-established:
  - "Runtime event channel: service payloads are wrapped as BackendServiceEvent::Update while lifecycle outcomes use distinct event variants."
requirements-completed: [BPLUG-03, BPLUG-04]
duration: 11 min
completed: 2026-05-03
---

# Phase 02 Plan 02: Backend Runtime Lifecycle Events Summary

**Backend Luau runtimes now emit typed lifecycle events and stop after repeated poll failures**

## Performance

- **Duration:** 11 min
- **Started:** 2026-05-03T17:32:40Z
- **Completed:** 2026-05-03T17:44:05Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- Added `BackendServiceEvent` with `Started`, `Update`, `InitFailed`, `PollFailed`, `Failed`, and `Stopped` variants.
- Converted `BackendScriptContext::run_poll()` and `run_command()` to return typed `Result<Option<JsonValue>, BackendScriptError>` values.
- Added `MAX_CONSECUTIVE_POLL_FAILURES: u32 = 3` and runtime stop behavior after repeated poll failures.
- Added tests proving init failure does not poll or dispatch commands and poll failures terminate the runtime cleanly.

## Task Commits

1. **Tasks 1-3: Runtime lifecycle events, scripting error propagation, and poll failure threshold** - `2bbf3a0` (feat)

**Plan metadata:** this SUMMARY commit

## Files Created/Modified

- `crates/core/runtime/backend/src/lib.rs` - Adds lifecycle event enum, event emission, poll failure threshold, and updated async runtime tests.
- `crates/core/runtime/scripting/src/backend.rs` - Returns typed poll/command handler errors and adds handler failure tests.

## Decisions Made

- Command handler failures emit a terminal `Failed { stage: "command" }` lifecycle event but do not add automatic restart behavior.
- Init failure emits `InitFailed` and exits before interval creation, preserving the gate that no polling or command dispatch happens after failed init.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- Initial init-failure test expected a timeout after `InitFailed`; the correct runtime behavior closes the event channel after exit. The test was corrected to assert no `Update` event is emitted.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Plan 03 can use `BackendServiceEvent` to clean up shell runtime slots on init failure, terminal failure, and stop events.

## Self-Check: PASSED

- `nix develop -c cargo test -p mesh-core-scripting backend` passed.
- `nix develop -c cargo test -p mesh-core-backend spawn_backend_service` passed.
- `grep -n "MAX_CONSECUTIVE_POLL_FAILURES\|InitFailed\|PollFailed\|Stopped" crates/core/runtime/backend/src/lib.rs` found expected lifecycle symbols.

---
*Phase: 02-backend-lifecycle-foundation*
*Completed: 2026-05-03*
