---
phase: 02-backend-lifecycle-foundation
plan: 04
subsystem: backend-diagnostics
tags: [rust, diagnostics, debug-snapshot, lifecycle-status]
requires:
  - phase: 02-backend-lifecycle-foundation
    provides: graph validation statuses and lifecycle events
provides:
  - exact backend lifecycle status vocabulary
  - deduplicated lifecycle diagnostics
  - debug snapshot backend runtime entries
affects: [backend lifecycle, diagnostics, debug overlay]
tech-stack:
  added: []
  patterns: [deduplicated lifecycle diagnostics by provider stage and message]
key-files:
  created: []
  modified:
    - crates/core/shell/src/shell/mod.rs
    - crates/core/foundation/diagnostics/src/lib.rs
    - crates/core/foundation/debug/src/lib.rs
key-decisions:
  - "Lifecycle diagnostics dedupe by provider id, lifecycle stage, and message."
  - "Debug snapshots expose backend lifecycle state without reading logs."
patterns-established:
  - "Lifecycle status model: store status by interface/provider and serialize exact phase-contract status strings."
requirements-completed: [BPLUG-01, BPLUG-02, BPLUG-03, BPLUG-04, BPLUG-05]
duration: 16 min
completed: 2026-05-03
---

# Phase 02 Plan 04: Backend Lifecycle Diagnostics Summary

**Backend startup, failure, and cleanup status is now visible through diagnostics and debug snapshots**

## Performance

- **Duration:** 16 min
- **Started:** 2026-05-03T17:44:05Z
- **Completed:** 2026-05-03T18:00:10Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Added `BackendRuntimeStatus` with exact status strings: `no_active_provider`, `unmet_backend_requirement`, `invalid_manifest`, `missing_entrypoint`, `missing_binary`, `init_failed`, `running`, `poll_failed`, `failed`, and `stopped`.
- Stored lifecycle status by interface/provider and updated it during graph validation, runtime start, poll failure, terminal failure, and stop cleanup.
- Added `Diagnostics::record_lifecycle_error` and `DiagnosticsCollector::record_lifecycle_error`, deduped by provider/stage/message.
- Added `BackendRuntimeEntry` to `DebugSnapshot` and populated sorted backend runtime status entries from shell state.
- Added focused tests for status names, deduplicated lifecycle diagnostics, and debug snapshot lifecycle visibility.

## Task Commits

1. **Tasks 1-3: Status model, lifecycle diagnostics, and debug snapshot output** - `8d8f813` (feat)
2. **Review fix: Preserve stopped status after transient poll failures during replacement** - `8e212eb` (fix)

**Plan metadata:** this SUMMARY commit

## Files Created/Modified

- `crates/core/shell/src/shell/mod.rs` - Adds backend lifecycle status storage, status updates, diagnostics wiring, and debug snapshot population.
- `crates/core/foundation/diagnostics/src/lib.rs` - Adds deduplicated lifecycle error recording and tests.
- `crates/core/foundation/debug/src/lib.rs` - Adds `BackendRuntimeEntry` and `DebugSnapshot::backend_runtimes`.

## Decisions Made

- Recorded graph validation failures with provider id when available and `"<none>"` when no active provider exists.
- Treated invalid manifests, missing entrypoints, missing binaries, init failures, poll failures, and terminal failures as lifecycle diagnostic errors.
- Sorted backend runtime debug entries by interface then provider id for stable snapshot output.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- Code review found that a transient `poll_failed` status suppressed later replacement `stopped` status. Commit `8e212eb` fixed the status transition and added a regression test.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 02 is ready for final validation: all four plans have summaries, lifecycle behavior is covered by focused tests, and runtime state is observable in diagnostics/debug data.

## Self-Check: PASSED

- `nix develop -c cargo test -p mesh-core-diagnostics lifecycle` passed.
- `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` passed.
- `nix develop -c cargo test -p mesh-core-backend spawn_backend_service` passed.
- `nix develop -c cargo test -p mesh-core-scripting backend` passed.

---
*Phase: 02-backend-lifecycle-foundation*
*Completed: 2026-05-03*
