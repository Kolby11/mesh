---
phase: 93-host-api-retargeting
plan: 02
subsystem: runtime
tags: [mlua, luau, scripting, backend, host-api, iso-02]

requires:
  - phase: 93-host-api-retargeting
    provides: "plan 93-01 retargeted ScriptContext::install_host_api to &Table"
provides:
  - "BackendScriptContext::install_host_api accepts &mlua::Table target parameter"
  - "ISO-02 foundation complete: both frontend and backend host API installers target caller-supplied table"
affects: [94-per-component-env, scripting, backend-runtime]

tech-stack:
  added: []
  patterns:
    - "Host API installer accepts &Table target — caller controls which table receives the API keys"

key-files:
  created: []
  modified:
    - crates/core/runtime/scripting/src/backend/runtime.rs

key-decisions:
  - "Pure signature + variable rename refactor — no logic changes, call site passes &lua.globals() preserving identical behavior"

patterns-established:
  - "install_host_api(&Table): both frontend ScriptContext and backend BackendScriptContext now accept a &mlua::Table — Phase 94 can pass per-component _ENV tables to both"

requirements-completed: [ISO-02]

duration: 5min
completed: 2026-06-07
---

# Phase 93 Plan 02: Host API Re-targeting (Backend) Summary

**BackendScriptContext::install_host_api refactored to accept &mlua::Table, completing ISO-02 foundation alongside the matching frontend change in plan 93-01**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-06-07T00:00:00Z
- **Completed:** 2026-06-07T00:05:00Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments

- Changed `BackendScriptContext::install_host_api` signature from `&mut self` to `&mut self, target: &mlua::Table`
- Replaced `let globals = self.lua.globals()` with `let globals = target` — single rename propagates to all `globals.set(...)` calls throughout the function body
- Updated call site in `new()` to split the borrow: `let globals = ctx.lua.globals(); ctx.install_host_api(&globals)`
- `Table` was already imported in the `use mlua::{...}` line — no import change needed
- `cargo build -p mesh-core-scripting` passes with zero errors

## Task Commits

1. **Task 1: Refactor BackendScriptContext::install_host_api to accept &Table (ISO-02)** - `0b5b0ac` (feat)

## Files Created/Modified

- `crates/core/runtime/scripting/src/backend/runtime.rs` - Changed install_host_api signature and call site

## Decisions Made

None - followed plan as specified. Pure signature + variable rename refactor with no logic changes.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

Pre-existing test failures in `pool::tests` and `context::tests` were observed (5 failures). These are caused by other in-progress work already staged in `pool.rs` and unrelated context test files — not introduced by this plan's change. `cargo build -p mesh-core-scripting` exits clean, confirming no compile errors from this change.

## Next Phase Readiness

- ISO-02 foundation is now complete: both `ScriptContext::install_host_api` (plan 93-01) and `BackendScriptContext::install_host_api` (plan 93-02) accept `&mlua::Table`
- Phase 94 can now pass per-component `_ENV` tables to both installers

---
*Phase: 93-host-api-retargeting*
*Completed: 2026-06-07*
