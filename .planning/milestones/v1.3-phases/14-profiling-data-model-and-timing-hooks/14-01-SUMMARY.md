---
phase: 14-profiling-data-model-and-timing-hooks
plan: 01
subsystem: debug-profiling-contract
tags: [debug, profiling, ipc, cli]

requires: []

provides:
  - Typed profiling snapshot and stage contracts in `mesh-core-debug`
  - Shell-owned profiling toggle request path through runtime request handling, IPC, and CLI
  - Initial profiling-disabled regression coverage

affects:
  - phase-14-collector
  - phase-14-snapshot-rollups

tech-stack:
  added: []
  patterns:
    - "Debug-only runtime capabilities extend the shared debug snapshot contract instead of creating a parallel subsystem"
    - "Shell-owned request, IPC, and CLI seams remain the control path for developer-only instrumentation features"

key-files:
  created:
    - .planning/phases/14-profiling-data-model-and-timing-hooks/14-01-SUMMARY.md
  modified:
    - crates/core/foundation/debug/src/lib.rs
    - crates/core/shell/src/shell/types.rs
    - crates/core/shell/src/shell/runtime/request.rs
    - crates/core/shell/src/shell/ipc.rs
    - crates/tools/cli/src/main.rs
    - crates/core/shell/src/shell/runtime/debug.rs
    - crates/core/shell/src/shell/tests.rs

key-decisions:
  - "Profiling state lives on the existing debug contract via `DebugSnapshot` and `DebugOverlayState`."
  - "Profiling control is exposed as an explicit shell request and IPC command instead of an end-user config setting."
  - "The CLI now treats profiling as a debug subcommand path (`mesh-shell debug profiling`)."

requirements-completed: [PROF-02, TIME-03]

duration: 1 session
completed: 2026-05-08
---

# Phase 14 Plan 01: Debug Profiling Contract and Control Path

**Phase 14 now has a typed profiling contract and an explicit debug-only control path, with profiling still absent from live snapshots when disabled.**

## Accomplishments

- Extended `mesh-core-debug` with typed profiling snapshot, stage, sample, and scope structures.
- Added shell-owned profiling toggle support through `CoreRequest`, request handling, IPC parsing, and the `mesh-shell debug profiling` CLI path.
- Added regression tests proving profiling toggle/session independence from overlay visibility and proving disabled snapshots omit profiling payloads.

## Task Commits

Pending commit in this workspace run. The implementation is validated and committed immediately after this summary.

## Verification

- `grep -n 'Profiling\|profiling_' crates/core/foundation/debug/src/lib.rs crates/core/shell/src/shell/types.rs crates/core/shell/src/shell/runtime/request.rs crates/core/shell/src/shell/ipc.rs crates/tools/cli/src/main.rs crates/core/shell/src/shell/tests.rs`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_`

## Deviations From Plan

- `crates/core/shell/src/shell/runtime/debug.rs` needed a small compatibility update to populate the new `DebugSnapshot.profiling` field with `None` while the collector/runtime rollup work remains in later plans. This was required to keep Wave 1 buildable and does not change Phase 14 scope.

## Self-Check: PASSED

- Summary file exists.
- Profiling contract types and toggle seams are present in the planned files.
- Disabled-mode profiling snapshot coverage passed in `mesh-core-shell` tests.

---
*Phase: 14-profiling-data-model-and-timing-hooks*
*Completed: 2026-05-08*
