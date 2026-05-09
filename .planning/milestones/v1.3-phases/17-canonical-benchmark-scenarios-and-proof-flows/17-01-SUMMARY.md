---
phase: 17-canonical-benchmark-scenarios-and-proof-flows
plan: 01
subsystem: debug
tags: [rust, mesh.debug, profiling, benchmarks, shell]

requires:
  - phase: 16-debug-only-profiling-mode-and-live-inspector
    provides: debug-only profiling snapshots and mesh.debug inspector payloads
provides:
  - typed canonical benchmark scenario contract
  - mesh.debug benchmark scenario payload with five stable rows
  - disabled-mode benchmark inertness proof
affects: [phase-17-benchmark-ui, phase-17-benchmark-launch, phase-18-optimization-proof]

tech-stack:
  added: []
  patterns:
    - shell-owned benchmark rows derived from live rolling profiling snapshots
    - benchmark JSON serialized beside existing mesh.debug profiling payload

key-files:
  created:
    - .planning/phases/17-canonical-benchmark-scenarios-and-proof-flows/17-01-SUMMARY.md
  modified:
    - crates/core/foundation/debug/src/lib.rs
    - crates/core/shell/src/shell/runtime/debug.rs
    - crates/core/shell/src/shell/tests.rs

key-decisions:
  - "Benchmark rows derive from live rolling profiling snapshots only; no history, trace export, or persistence was added."
  - "Profiling-disabled benchmark rows remain visible but inert with Profiling off status and profiling payload stays null."

patterns-established:
  - "DebugBenchmarkSnapshot holds fixed scenario rows in the typed debug contract."
  - "benchmark_snapshot derives stable benchmark rows inside Shell::build_debug_snapshot before mesh.debug JSON serialization."

requirements-completed: [BENCH-01, BENCH-02, BENCH-03, BENCH-04, BENCH-05]

duration: 6min
completed: 2026-05-09
---

# Phase 17 Plan 01: Benchmark Contract and Debug Snapshot Shape Summary

**Typed mesh.debug benchmark contract with five canonical scenario rows derived from live rolling profiling snapshots**

## Performance

- **Duration:** 6 min
- **Started:** 2026-05-09T07:50:46Z
- **Completed:** 2026-05-09T07:56:38Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Added `DebugBenchmarkSnapshot`, `BenchmarkScenarioSnapshot`, `BenchmarkScenarioId`, and `BenchmarkScenarioStatus` to the core debug contract.
- Populated `DebugSnapshot.benchmarks` from `Shell::build_debug_snapshot()` with exactly five fixed scenarios: hover, surface open/close, pointer update, keyboard traversal, and backend update.
- Serialized `mesh.debug.benchmarks.scenarios` with stable ids, labels, targets, statuses, metrics, and hints while preserving `profiling: null` when profiling is disabled.
- Added focused shell tests proving stable scenario ordering, disabled-mode inertness, and payload JSON shape.

## Task Commits

1. **Task 17-01-01: Add typed benchmark snapshot structures** - `0329a2c` (feat)
2. **Task 17-01-02: Derive default benchmark scenarios in debug snapshots** - `f117118` (feat)
3. **Task 17-01-03: Prove benchmark debug payload shape and disabled-mode behavior** - `ae3f4a9` (test)

## Files Created/Modified

- `crates/core/foundation/debug/src/lib.rs` - Adds benchmark snapshot structs, fixed scenario ids, and status labels.
- `crates/core/shell/src/shell/runtime/debug.rs` - Derives benchmark rows from current profiling summaries and serializes them into `mesh.debug`.
- `crates/core/shell/src/shell/tests.rs` - Adds `benchmark_` tests for scenario ids, disabled behavior, and serialized payload shape.
- `.planning/phases/17-canonical-benchmark-scenarios-and-proof-flows/17-01-SUMMARY.md` - Execution summary and verification record.

## Decisions Made

- Benchmark rows are shell-owned and derived from existing live rolling profiling buckets, not from a new benchmark history store.
- Profiling remains independently toggled; building a debug snapshot does not enable profiling and disabled benchmark rows use `Profiling off`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added explicit active surface collection type**
- **Found during:** Task 17-01-02 (Derive default benchmark scenarios in debug snapshots)
- **Issue:** Borrowing `active_surfaces` into the new helper left Rust unable to infer the collection type.
- **Fix:** Annotated the collection as `Vec<String>`.
- **Files modified:** `crates/core/shell/src/shell/runtime/debug.rs`
- **Verification:** `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark`
- **Committed in:** `f117118`

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Compile-only fix required for the planned implementation. No scope expansion.

## Issues Encountered

- Parallel plan-level verification briefly waited on Nix/package cache locks. Both commands completed successfully after the lock cleared.

## Verification

- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt` - passed
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` - passed, 3 tests
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` - passed, 23 tests

## Known Stubs

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

The benchmark data contract is available in `mesh.debug`. Plan 17-02 can add explicit benchmark launch requests against the same stable scenario ids, and Plan 17-03 can render these rows in the inspector.

## Self-Check: PASSED

- Found summary file.
- Found modified source files.
- Found task commits `0329a2c`, `f117118`, and `ae3f4a9`.

---
*Phase: 17-canonical-benchmark-scenarios-and-proof-flows*
*Completed: 2026-05-09*
