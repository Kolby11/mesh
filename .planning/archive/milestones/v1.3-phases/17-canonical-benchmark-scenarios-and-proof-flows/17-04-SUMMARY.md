---
phase: 17-canonical-benchmark-scenarios-and-proof-flows
plan: 04
subsystem: debug
tags: [rust, mesh.debug, profiling, benchmarks, backend-correlation, testing]

requires:
  - phase: 17-canonical-benchmark-scenarios-and-proof-flows
    provides: benchmark debug payload contract, launch requests, and inspector rows from Plans 17-01 through 17-03
  - phase: 15-backend-and-surface-attribution
    provides: provider-attributed backend profiling stages and comparable per-surface render snapshots
provides:
  - backend-driven benchmark correlation between provider stage timing and frontend surface render cost
  - complete and missing-data proof for backend_update benchmark rows
  - final focused Phase 17 benchmark, inspector, and profiling proof suite
affects: [phase-18-optimization-proof, benchmark-proof, backend-attribution]

tech-stack:
  added: []
  patterns:
    - backend_update completion requires both generic backend stage timing and frontend total_surface_render timing
    - missing backend or frontend timing remains visible as Waiting for samples or Unavailable without service-specific payload parsing

key-files:
  created:
    - .planning/phases/17-canonical-benchmark-scenarios-and-proof-flows/17-04-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/runtime/debug.rs
    - crates/core/shell/src/shell/tests.rs

key-decisions:
  - "Backend-driven benchmark completion requires both provider-stage timing and frontend surface render timing."
  - "Backend benchmark target text is derived from existing profiling/runtime identities while preserving the canonical mesh.audio -> @mesh/pipewire-audio fallback."
  - "Task 17-04-03 is recorded with an empty verification commit because the final proof suite passed without code changes."

patterns-established:
  - "backend_update benchmark rows use ProfilingBackendSnapshot and ProfilingSurfaceSnapshot only; no audio payload fields are parsed in Rust."
  - "Focused shell tests prove complete, missing-surface, and missing-backend benchmark states."

requirements-completed: [BACK-03, BENCH-05]

duration: 7min
completed: 2026-05-09
---

# Phase 17 Plan 04: Backend Correlation and Final Benchmark Proof Summary

**Backend-driven benchmark rows now require correlated provider-stage timing and frontend surface render cost**

## Performance

- **Duration:** 7 min
- **Started:** 2026-05-09T08:21:14Z
- **Completed:** 2026-05-09T08:28:03Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- Tightened `backend_update` benchmark result assembly so `Complete` requires both backend stage timing and frontend `total_surface_render` timing.
- Derived backend target text from existing backend profiling/runtime identities, while keeping the canonical `mesh.audio -> @mesh/pipewire-audio` fallback and avoiding audio payload parsing.
- Added shell proof for complete correlation plus missing-surface and missing-backend states.
- Ran the final focused Phase 17 proof suite across benchmark, debug inspector, and profiling selectors.

## Task Commits

1. **Task 17-04-01: Populate backend-driven benchmark correlation result** - `ae0b717` (feat)
2. **Task 17-04-02: Add shell proof for backend-to-frontend correlation** - `37ca314` (test)
3. **Task 17-04-03: Run final Phase 17 focused proof suite** - `a8d5592` (test, empty verification commit)

## Files Created/Modified

- `crates/core/shell/src/shell/runtime/debug.rs` - Correlates backend_update rows using generic backend and surface profiling snapshots, with explicit complete, waiting, and unavailable states.
- `crates/core/shell/src/shell/tests.rs` - Adds backend_update proof tests for complete correlation and both missing-data paths.
- `.planning/phases/17-canonical-benchmark-scenarios-and-proof-flows/17-04-SUMMARY.md` - Execution summary and verification record.

## Decisions Made

- Backend-driven benchmark completion now requires both a backend stage sample and a non-zero frontend surface render total.
- Missing backend timing can still display already-captured frontend render timing, but the row stays `Waiting for samples`.
- The final proof-only task uses an empty commit because no files changed after the required commands passed.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Preserved frontend render timing while waiting for backend samples**
- **Found during:** Task 17-04-02 (Add shell proof for backend-to-frontend correlation)
- **Issue:** The first missing-backend implementation returned a generic waiting message even when frontend surface render timing was already present.
- **Fix:** Threaded the optional `ProfilingSurfaceSnapshot` into the backend-waiting state so the row can show `frontend total_surface_render` while still refusing to mark the scenario complete.
- **Files modified:** `crates/core/shell/src/shell/runtime/debug.rs`, `crates/core/shell/src/shell/tests.rs`
- **Verification:** `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark_backend`
- **Committed in:** `37ca314`

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** The fix strengthened the planned missing-data proof without adding persistence, new profiling surfaces, or service-specific Rust behavior.

## Issues Encountered

- Parallel Nix test commands briefly waited on package-cache and artifact locks. All focused commands completed successfully after the locks cleared.

## Verification

- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt` - passed
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` - passed, 14 tests
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark_backend` - passed, 3 tests
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_inspector` - passed, 10 tests
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` - passed, 25 tests

## Known Stubs

None.

## Threat Flags

None - no new network endpoints, auth paths, file access patterns, schema changes, persistence, or service-specific Rust payload parsing were introduced.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 17 now has automated proof that the canonical backend-driven scenario correlates backend provider/stage timing with visible frontend render cost. Phase 18 can use the same fixed benchmark ids and live debug profiling rows for bounded before/after optimization proof.

## Self-Check: PASSED

- Found summary file.
- Found modified source files.
- Found task commits `ae0b717`, `37ca314`, and `a8d5592`.
- Confirmed no accidental file deletions in task commits.

---
*Phase: 17-canonical-benchmark-scenarios-and-proof-flows*
*Completed: 2026-05-09*
