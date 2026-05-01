---
phase: 01-backend-host-api-contract
plan: 01
subsystem: api
tags: [rust, luau, mlua, backend, host-api]
requires: []
provides:
  - structured backend command execution without shell argument injection
  - backend plugin settings exposed through mesh.config()
  - callable backend logging and emit failure coverage
affects: [backend-runtime, plugin-authors, host-api-docs]
tech-stack:
  added: []
  patterns: [callable Lua tables for host APIs, structured exec result tables, per-context backend settings]
key-files:
  created: [.planning/phases/01-backend-host-api-contract/01-01-SUMMARY.md]
  modified:
    - crates/core/runtime/scripting/src/backend.rs
    - crates/core/runtime/scripting/src/host_api.rs
key-decisions:
  - "Preserved single-string mesh.exec compatibility while making (program, args) the safe structured form."
  - "Implemented mesh.log as a callable Lua table so mesh.log(level, msg) and alias methods share one dispatch path."
patterns-established:
  - "Backend host APIs return structured Lua tables instead of throwing for ordinary command failures."
  - "Backend-specific API comments in host_api.rs must track the implemented Luau surface."
requirements-completed: [HOST-01, HOST-02, HOST-03, HOST-04, HOST-05]
duration: 38 min
completed: 2026-05-01
---

# Phase 01 Plan 01: Backend Host API Contract Summary

**Structured backend exec/config/log host APIs in `BackendScriptContext` with explicit emit failure coverage**

## Performance

- **Duration:** 38 min
- **Started:** 2026-05-01T15:20:00Z
- **Completed:** 2026-05-01T15:58:26Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments
- Added safe structured `mesh.exec(program, args)` while preserving the existing single-string compatibility form and `mesh.exec_shell(command)` behavior.
- Stored backend plugin settings per `BackendScriptContext` and exposed them to Luau via `mesh.config()`.
- Added callable `mesh.log(level, msg)` with aliases plus regression coverage for invalid `mesh.service.emit()` payloads.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add structured backend exec while preserving existing exec_shell behavior** - `16d9c7e` (feat)
2. **Task 2: Add backend settings and mesh.config()** - `ca2ae2a` (feat)
3. **Task 3: Add mesh.log(level, msg), aliases, and emit failure coverage** - `2348a19` (feat)

Plan metadata was captured in follow-up `docs(01-01)` commits for the summary file.

## Files Created/Modified
- `crates/core/runtime/scripting/src/backend.rs` - Backend Luau host API implementation and unit coverage for exec, config, logging, and emit failure behavior.
- `crates/core/runtime/scripting/src/host_api.rs` - Host API comment updates for backend `mesh.config()` and `mesh.log(level, msg)`.
- `.planning/phases/01-backend-host-api-contract/01-01-SUMMARY.md` - Plan execution summary.

## Decisions Made
- Preserved the old whitespace-splitting `mesh.exec("cmd args")` path for compatibility fixtures, but only the structured `(program, args)` path uses direct `StdCommand::new(program).args(args)`.
- Kept backend config local to `BackendScriptContext` so `mesh.config()` can only return the settings passed to that plugin instance.
- Used a callable Lua table for `mesh.log` so the direct `mesh.log(level, msg)` form and alias methods share the same normalized dispatch behavior.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed callable `mesh.log` metamethod argument handling**
- **Found during:** Task 3
- **Issue:** The initial callable-table implementation for `mesh.log(level, msg)` did not account for Lua passing the table itself as the first `__call` metamethod argument, which caused the logging test to return no payload.
- **Fix:** Updated the `__call` handler to accept and ignore the table receiver, then re-ran the logging acceptance test.
- **Files modified:** `crates/core/runtime/scripting/src/backend.rs`
- **Verification:** `cargo test -p mesh-core-scripting backend::tests::log_level_function_and_aliases_are_callable`
- **Committed in:** `2348a19`

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** The fix was required for the documented callable logging API to work. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Backend host API requirements HOST-01 through HOST-05 are implemented and covered in `mesh-core-scripting`.
- Phase 01 Plan 02 can now focus on runtime-loop integration for settings/poll interval behavior without revisiting the Luau API surface.

## Verification

- `cargo test -p mesh-core-scripting backend` — passed
- `cargo test -p mesh-core-scripting exec_returns_structured_result` — passed
- `cargo test -p mesh-core-scripting backend::tests::exec_accepts_program_and_args` — passed
- `cargo test -p mesh-core-scripting backend::tests::config_returns_backend_settings` — passed
- `cargo test -p mesh-core-scripting backend::tests::log_level_function_and_aliases_are_callable` — passed
- `cargo test -p mesh-core-scripting backend::tests::bad_emit_payload_does_not_emit_stale_state` — passed
- `rg -n "mesh\\.config\\(\\)|mesh\\.log\\(level" crates/core/runtime/scripting/src/host_api.rs crates/core/runtime/scripting/src/backend.rs` — matched documented API entries
- `rg -n "mesh\\.exec_shell|mesh\\.service\\.emit|mesh\\.config|mesh\\.log" packages/plugins/backend/core/pipewire-audio/src/main.luau packages/plugins/backend/core/pulseaudio-audio/src/main.luau` — confirmed bundled scripts still reference supported API names

## Self-Check: PASSED

- Summary file exists: `.planning/phases/01-backend-host-api-contract/01-01-SUMMARY.md`
- Verified task commits: `16d9c7e`, `ca2ae2a`, `2348a19`

---
*Phase: 01-backend-host-api-contract*
*Completed: 2026-05-01*
