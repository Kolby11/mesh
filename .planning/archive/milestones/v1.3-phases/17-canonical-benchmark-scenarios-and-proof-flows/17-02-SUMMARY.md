---
phase: 17-canonical-benchmark-scenarios-and-proof-flows
plan: 02
subsystem: debug
tags: [rust, mesh.debug, profiling, benchmarks, shell, ipc]

requires:
  - phase: 17-canonical-benchmark-scenarios-and-proof-flows
    provides: typed benchmark scenario contract and mesh.debug benchmark rows from Plan 17-01
provides:
  - explicit RunDebugBenchmark core request
  - shell.run-debug-benchmark event routing
  - shell:debug_benchmark IPC routing
  - fixed-id benchmark request validation and non-fatal diagnostics
  - debug-owned latest requested benchmark run state
affects: [phase-17-benchmark-ui, phase-17-backend-correlation, phase-18-optimization-proof]

tech-stack:
  added: []
  patterns:
    - debug-scoped benchmark launch requests with fixed scenario id validation
    - benchmark requests reuse normal shell request semantics instead of adding a profiler entrypoint

key-files:
  created:
    - .planning/phases/17-canonical-benchmark-scenarios-and-proof-flows/17-02-SUMMARY.md
  modified:
    - crates/core/frontend/host/src/lib.rs
    - crates/core/foundation/debug/src/lib.rs
    - crates/core/shell/src/shell/service.rs
    - crates/core/shell/src/shell/ipc.rs
    - crates/core/shell/src/shell/runtime/request.rs
    - crates/core/shell/src/shell/runtime/debug.rs
    - crates/core/shell/src/shell/tests.rs

key-decisions:
  - "Benchmark launch requests accept only the five canonical scenario ids and report unknown ids through non-fatal diagnostics."
  - "Surface open/close benchmark launch reuses normal ShowSurface and HideSurface requests for @mesh/audio-popover."
  - "Benchmark launch requests record session-local debug state but never toggle debug profiling."

patterns-established:
  - "RunDebugBenchmark routes through CoreRequest and the existing shell request drain like other debug actions."
  - "DebugOverlayState owns latest_benchmark_run so benchmark launch state remains live/session-scoped."

requirements-completed: [BENCH-01, BENCH-02, BENCH-03, BENCH-04]

duration: 6min
completed: 2026-05-09
---

# Phase 17 Plan 02: Benchmark Launch Requests and Scenario Execution Hooks Summary

**Debug-scoped benchmark run requests with fixed scenario validation and shell-native surface execution hooks**

## Performance

- **Duration:** 6 min
- **Started:** 2026-05-09T08:00:13Z
- **Completed:** 2026-05-09T08:05:40Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments

- Added `CoreRequest::RunDebugBenchmark` and mapped both `shell.run-debug-benchmark` frontend events and `shell:debug_benchmark:<scenario_id>` IPC commands into it.
- Implemented accepted scenario handling for `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`, with unknown ids returning non-fatal diagnostics.
- Added session-local latest benchmark run state to `DebugOverlayState` and threaded it into benchmark snapshot status/hints.
- Reused existing shell surface behavior for `surface_open_close` by emitting `ShowSurface` then `HideSurface` for `@mesh/audio-popover`.
- Proved request routing, unknown-scenario rejection, and that benchmark requests do not enable profiling.

## Task Commits

1. **Task 17-02-01: Add debug benchmark request type and event mapping** - `3ff4af8` (feat)
2. **Task 17-02-02: Handle benchmark run requests without starting profiling** - `c9f1bb5` (feat)
3. **Task 17-02-03: Prove benchmark request routing** - `5104f07` (test)

## Files Created/Modified

- `crates/core/frontend/host/src/lib.rs` - Adds `RunDebugBenchmark` to the shell/frontend request contract.
- `crates/core/foundation/debug/src/lib.rs` - Adds benchmark run state and `Running` benchmark status label support.
- `crates/core/shell/src/shell/service.rs` - Maps `shell.run-debug-benchmark` events and reports missing `scenario_id`.
- `crates/core/shell/src/shell/ipc.rs` - Parses `shell:debug_benchmark:<scenario_id>` IPC commands.
- `crates/core/shell/src/shell/runtime/request.rs` - Validates scenario ids, records latest run state, emits surface open/close requests, and diagnoses unknown ids.
- `crates/core/shell/src/shell/runtime/debug.rs` - Reflects latest requested benchmark state in `mesh.debug.benchmarks.scenarios`.
- `crates/core/shell/src/shell/tests.rs` - Adds benchmark request routing and profiling independence tests.

## Decisions Made

- Fixed-id validation stays in the shell request handler so event and IPC inputs share one allowlist.
- `surface_open_close` uses existing `ShowSurface` and `HideSurface` request semantics rather than direct benchmark-only surface mutation.
- Latest run state is stored only in `DebugOverlayState`, preserving live/session-scoped behavior and avoiding persistence.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added minimal runtime handling during request routing**
- **Found during:** Task 17-02-01 (Add debug benchmark request type and event mapping)
- **Issue:** Adding `RunDebugBenchmark` made `CoreRequest` matches in `request.rs` non-exhaustive, so the focused benchmark test command could not compile before Task 17-02-02.
- **Fix:** Added initial non-fatal runtime handling and profiling trigger labeling for the new request, then expanded it in Task 17-02-02.
- **Files modified:** `crates/core/shell/src/shell/runtime/request.rs`
- **Verification:** `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark`
- **Committed in:** `3ff4af8`

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Compile-only ordering fix required by Rust exhaustiveness. Final behavior matches the planned Task 17-02-02 request handling.

## Issues Encountered

- Initial Task 17-02-01 verification failed because the new `CoreRequest` variant was not yet handled in `request.rs`; resolved as the documented Rule 3 deviation.

## Verification

- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt` - passed
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` - passed, 7 tests

## Known Stubs

None.

## Threat Flags

None - the new debug event and IPC request surface was planned in the threat model and is constrained to fixed scenario ids.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Plan 17-03 can wire inspector controls to `shell.run-debug-benchmark` and render latest requested scenario states from `mesh.debug.benchmarks.scenarios`. Plan 17-04 can build on the same request ids for backend/frontend correlation proof.

## Self-Check: PASSED

- Found summary file.
- Found task commits `3ff4af8`, `c9f1bb5`, and `5104f07`.
- Confirmed no accidental file deletions in task commits.

---
*Phase: 17-canonical-benchmark-scenarios-and-proof-flows*
*Completed: 2026-05-09*
