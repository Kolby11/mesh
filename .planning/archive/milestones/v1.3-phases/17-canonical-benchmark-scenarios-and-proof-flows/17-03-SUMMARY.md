---
phase: 17-canonical-benchmark-scenarios-and-proof-flows
plan: 03
subsystem: ui
tags: [mesh, debug-inspector, benchmarks, profiling, component-tests]

requires:
  - phase: 17-canonical-benchmark-scenarios-and-proof-flows
    provides: benchmark debug payload contract and benchmark launch requests from Plans 17-01 and 17-02
provides:
  - compact debug-inspector benchmark rows for all five canonical scenarios
  - safe parent-side benchmark payload normalization with sparse and malformed fallbacks
  - explicit benchmark run controls publishing canonical scenario ids
  - real-surface component proof for profiling-off, waiting, populated, and action states
affects: [phase-17-backend-correlation, phase-18-optimization-proof]

tech-stack:
  added: []
  patterns:
    - debug-inspector parent normalizes mesh.debug payloads into primitive child props
    - benchmark UI stays fixed-row and 320px-panel friendly using semantic theme tokens

key-files:
  created:
    - .planning/phases/17-canonical-benchmark-scenarios-and-proof-flows/17-03-SUMMARY.md
  modified:
    - modules/frontend/debug-inspector/src/main.mesh
    - modules/frontend/debug-inspector/src/components/benchmark-view.mesh
    - crates/core/shell/src/shell/component/tests.rs

key-decisions:
  - "Benchmark view rows consume primitive normalized props from the inspector parent rather than reading mesh.debug directly."
  - "Benchmark action buttons always publish shell.run-debug-benchmark with fixed canonical scenario ids."

patterns-established:
  - "Five benchmark row slots remain visible for profiling-off, waiting, running, complete, unavailable, skipped, and sparse-payload states."
  - "Real-surface debug-inspector tests feed mesh.debug benchmark payloads and assert rendered row text plus run request publication."

requirements-completed: [BENCH-01, BENCH-02, BENCH-03, BENCH-04, BENCH-05]

duration: 9min
completed: 2026-05-09
---

# Phase 17 Plan 03: Inspector Benchmark UI Rows Summary

**Debug inspector benchmark rows with safe mesh.debug normalization, canonical run controls, and real-surface UI proof**

## Performance

- **Duration:** 9 min
- **Started:** 2026-05-09T08:09:14Z
- **Completed:** 2026-05-09T08:17:57Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Replaced scaffold-only benchmark cards with exactly five compact benchmark rows in the existing debug inspector benchmark view.
- Added parent-side normalization for `mesh.debug.benchmarks.scenarios`, including profiling-off and live-without-results fallbacks.
- Wired row actions to `shell.run-debug-benchmark` with fixed ids: `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`.
- Added real-surface component tests for profiling-off rows, waiting rows, populated result rows, and run-action request publication.

## Task Commits

1. **Task 17-03-01: Normalize benchmark payload in the inspector parent** - `6bd8640` (feat)
2. **Task 17-03-02: Render compact benchmark rows** - `291d12f` (feat)
3. **Task 17-03-03: Prove benchmark UI states on real inspector surface** - `973e963` (test)

## Files Created/Modified

- `modules/frontend/debug-inspector/src/main.mesh` - Adds fixed benchmark row state, safe scenario lookup/fallbacks, and benchmark run handlers.
- `modules/frontend/debug-inspector/src/components/benchmark-view.mesh` - Renders five compact scenario rows with status, action, metrics, target, and hint slots.
- `crates/core/shell/src/shell/component/tests.rs` - Adds real-surface benchmark row and action tests.
- `.planning/phases/17-canonical-benchmark-scenarios-and-proof-flows/17-03-SUMMARY.md` - Execution summary and verification record.

## Decisions Made

- Benchmark row data is normalized in the inspector parent and passed as primitive props, matching existing debug inspector view patterns.
- Benchmark actions stay debug-scoped and publish only `shell.run-debug-benchmark` with the established canonical scenario ids.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated scaffold assertion after replacing scaffold UI**
- **Found during:** Task 17-03-02 (Render compact benchmark rows)
- **Issue:** The existing real-surface test still asserted the old scaffold copy after the task intentionally replaced that scaffold.
- **Fix:** Updated the assertion to the new UI-SPEC benchmark copy.
- **Files modified:** `crates/core/shell/src/shell/component/tests.rs`
- **Verification:** `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_inspector`
- **Committed in:** `291d12f`

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** The fix aligned the existing test with the planned UI replacement. No behavior scope was added.

## Issues Encountered

- Parallel plan-level verification briefly waited on Nix package/artifact cache locks. Both commands completed successfully after the locks cleared.

## Verification

- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt` - passed, no changes
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_inspector` - passed, 10 tests
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` - passed, 11 tests

## Known Stubs

None.

## Threat Flags

None - benchmark actions and payload consumption were planned surfaces, remain debug-scoped, and use fixed scenario ids.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

The debug inspector now renders and launches the five canonical benchmark scenarios from `mesh.debug`. Plan 17-04 can focus on backend/frontend correlation proof using the populated benchmark result rows and existing scenario ids.

## Self-Check: PASSED

- Found summary file.
- Found modified source files.
- Found task commits `6bd8640`, `291d12f`, and `973e963`.
- Confirmed no accidental file deletions in task commits.

---
*Phase: 17-canonical-benchmark-scenarios-and-proof-flows*
*Completed: 2026-05-09*
