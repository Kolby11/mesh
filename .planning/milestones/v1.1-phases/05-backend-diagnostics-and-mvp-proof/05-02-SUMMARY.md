---
phase: 05-backend-diagnostics-and-mvp-proof
plan: 02
subsystem: runtime
tags: [diagnostics, shell, backend, lifecycle, stale-state, deduplication]

# Dependency graph
requires:
  - phase: 05-01
    provides: Stage-aware BackendScriptError variants for sharper failure signals
provides:
  - LifecycleErrorRecord with provider_id, stage, count, last_seen, latest_message
  - Diagnostics.lifecycle_error_records() accessor
  - handle_backend_lifecycle clears latest_service_state on active-provider failure
  - clear_active_provider_service_state() generic unavailable payload synthesis
  - BackendRuntimeStatusEntry.failure_count cumulative counter
  - BackendRuntimeEntry.failure_count in debug snapshot
affects: [05-03, 05-04, shell-diagnostics, debug-overlay]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Provider-plus-stage diagnostic bucket: HashMap<(provider_id, stage), LifecycleErrorRecord> replaces HashSet tuple dedup
    - Generic unavailable payload synthesis: clear_active_provider_service_state() sets available=false without service-specific branches
    - Cumulative failure count in runtime status: BackendRuntimeStatusEntry.failure_count increments per failure-category event

key-files:
  created: []
  modified:
    - crates/core/foundation/diagnostics/src/lib.rs
    - crates/core/shell/src/shell/mod.rs
    - crates/core/foundation/debug/src/lib.rs

key-decisions:
  - "Lifecycle error dedup keyed by (provider_id, stage); message changes do not create new buckets — only metadata updates."
  - "Shell synthesizes unavailable payload generically: preserves existing state shape and sets available=false; no service-specific branches."
  - "Failure count tracked in BackendRuntimeStatusEntry (shell-local) and surfaced through BackendRuntimeEntry in debug snapshot — avoids a second diagnostics store."
  - "Stale provider failures (non-current provider_id) do not clear the active provider's public service state."

patterns-established:
  - "Bucket aggregation: (provider_id, stage) key with LifecycleErrorRecord metadata — first occurrence increments error_count, repeats update count/last_seen only."
  - "Generic state clearing: synthesize { available: false } from existing payload shape without service-specific Rust logic."

requirements-completed: [BDIAG-02, BDIAG-03, BDIAG-04]

# Metrics
duration: 9min
completed: 2026-05-04
---

# Phase 5 Plan 02: Stale-State Clearing and Lifecycle Diagnostic Aggregation Summary

**Provider-plus-stage diagnostic buckets eliminate repeated-failure spam, and active-provider failures immediately replace stale public service state with an unavailable payload — keeping consumers honest without service-specific Rust logic.**

## Performance

- **Duration:** ~9 min
- **Started:** 2026-05-04T17:14:00Z
- **Completed:** 2026-05-04T17:23:55Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Replaced `lifecycle_errors: HashSet<(provider_id, stage, message)>` with `HashMap<(provider_id, stage), LifecycleErrorRecord>` in `DiagnosticsState`
- `LifecycleErrorRecord` stores `provider_id`, `stage`, `latest_message`, `count` (u64), and `last_seen` (SystemTime)
- First (provider_id, stage) occurrence sets health and increments `error_count`; repeats update metadata only
- Added `Diagnostics::lifecycle_error_records()` accessor for test and debug visibility
- Added `clear_active_provider_service_state()` in shell — sets `available=false` generically on active-provider failure without service-specific branches
- `handle_backend_lifecycle()` now calls state clearing when the current active provider enters `init_failed`, `failed`, or `stopped`
- Stale (non-current) provider failure events do not affect the active provider's public state
- Added `failure_count` to `BackendRuntimeStatusEntry` (cumulative failure events per provider)
- Added `failure_count` to `BackendRuntimeEntry` in `mesh-core-debug` so debug snapshots show rolled-up failure context
- `build_debug_snapshot()` projects `failure_count` into the debug entry

## Task Commits

1. **Task 1: Bucket lifecycle diagnostics by provider and stage** - `42badbc` (feat)
2. **Task 2: Clear active-provider public state on failure** - `54a8c3d` (feat)
3. **Task 3: Expose deduped failure context through shell runtime status paths** - `2b47a52` (feat)

## Files Created/Modified

- `crates/core/foundation/diagnostics/src/lib.rs` — LifecycleErrorRecord struct; HashMap bucket aggregation; lifecycle_error_records() accessor; 3 new tests
- `crates/core/shell/src/shell/mod.rs` — clear_active_provider_service_state(); failure_count in BackendRuntimeStatusEntry; failure_count in build_debug_snapshot(); 4 new tests
- `crates/core/foundation/debug/src/lib.rs` — failure_count field added to BackendRuntimeEntry

## Decisions Made

- Bucket key is `(provider_id, stage)` — not `(provider_id, stage, message)`. Message changes do not create new diagnostic spam for repeated poll failures.
- Shell synthesizes the unavailable payload generically: if the interface has existing state, preserves the object shape and sets `available=false`; otherwise writes `{ "available": false }`. No service names or field names hardcoded.
- `failure_count` is tracked in `BackendRuntimeStatusEntry` (shell-local) rather than pulling from the diagnostics crate on every snapshot build — simpler and avoids cross-crate coupling for a rendering path.

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None — no external service configuration required.

## Known Stubs

None.

## Threat Flags

None — no new network endpoints, auth paths, or trust boundaries introduced.

## Next Phase Readiness

- Phase 5 plans 03 and 04 can rely on stable diagnostic buckets and stale-state clearing semantics
- `BDIAG-02` (graceful degradation without shell crash), `BDIAG-03` (active-provider stale state cleared), and `BDIAG-04` (provider identity in diagnostics) are verified by 8 new tests plus the full 78-test shell suite

## Self-Check: PASSED

- `crates/core/foundation/diagnostics/src/lib.rs` exists and contains `lifecycle_errors_are_deduplicated_by_provider_and_stage`, `repeated_lifecycle_failures_increment_count_without_new_error`, `lifecycle_error_record_keeps_latest_message`
- `crates/core/shell/src/shell/mod.rs` exists and contains `active_provider_failure_clears_latest_service_state`, `stale_provider_failure_does_not_clear_new_provider_state`, `backend_lifecycle_debug_snapshot_includes_failure_counts`, `backend_runtime_status_records_provider_identity_for_failures`
- `crates/core/foundation/debug/src/lib.rs` exists and contains `failure_count` field on `BackendRuntimeEntry`
- Commits 42badbc, 54a8c3d, 2b47a52 exist in git log
- 78 tests pass in mesh-core-shell, 6 tests pass in mesh-core-diagnostics

---
*Phase: 05-backend-diagnostics-and-mvp-proof*
*Completed: 2026-05-04*
