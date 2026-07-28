---
phase: 14-profiling-data-model-and-timing-hooks
plan: 02
subsystem: profiling-collector
tags: [profiling, runtime, snapshots]

requires:
  - phase: 14-01
    provides: Shared profiling types and debug-only control path

provides:
  - Shell-owned profiling collector state with fixed recent-sample retention
  - Clean enable-time session reset semantics
  - Snapshot assembly backed by runtime collector state

affects:
  - phase-14-stage-instrumentation
  - phase-14-snapshot-regressions

tech-stack:
  added: []
  patterns:
    - "Shell-wide and per-surface profiling data live in one shell-owned runtime collector"
    - "Enable-time session resets create clean measurement runs instead of appending to stale state"

key-files:
  created:
    - .planning/phases/14-profiling-data-model-and-timing-hooks/14-02-SUMMARY.md
    - crates/core/shell/src/shell/runtime/profiling.rs
  modified:
    - crates/core/shell/src/shell/mod.rs
    - crates/core/shell/src/shell/discovery.rs
    - crates/core/shell/src/shell/runtime/mod.rs
    - crates/core/shell/src/shell/runtime/request.rs
    - crates/core/shell/src/shell/runtime/debug.rs
    - crates/core/shell/src/shell/tests.rs

key-decisions:
  - "The collector uses fixed recent-sample capacity per stage accumulator rather than time-window retention."
  - "Profiling session reset happens when profiling is enabled, keyed by the incremented debug profiling session id."
  - "Snapshot assembly now reads profiling data from the runtime collector but still omits it when profiling is disabled."

requirements-completed: [PROF-03, TIME-03]

duration: 1 session
completed: 2026-05-08
---

# Phase 14 Plan 02: Bounded Profiling Collector and Session Storage

**Phase 14 now has a bounded runtime collector with clean session-reset semantics, and debug snapshots can serialize collector state once instrumentation starts feeding it.**

## Accomplishments

- Added `crates/core/shell/src/shell/runtime/profiling.rs` with shell-wide and per-surface accumulators, fixed recent-sample retention, and profiling sample ordering.
- Wired `Shell` ownership and initialization for profiling runtime state.
- Connected profiling enablement to collector session reset and updated `build_debug_snapshot()` to emit collector-backed profiling payloads only when profiling is enabled.
- Added regression tests covering session reset, bounded surface snapshots, redraw counts, and disabled-mode omission.

## Task Commits

Pending commit in this workspace run. The implementation is validated and committed immediately after this summary.

## Verification

- `grep -n 'profil' crates/core/shell/src/shell/mod.rs crates/core/shell/src/shell/runtime/profiling.rs crates/core/shell/src/shell/runtime/request.rs crates/core/shell/src/shell/runtime/debug.rs crates/core/shell/src/shell/tests.rs`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_`

## Deviations From Plan

None. The collector/session-reset work stayed inside the planned shell runtime and snapshot seams.

## Self-Check: PASSED

- Summary file exists.
- Collector ownership and reset logic are present in the planned runtime files.
- Profiling reset and snapshot retention tests passed.

---
*Phase: 14-profiling-data-model-and-timing-hooks*
*Completed: 2026-05-08*
