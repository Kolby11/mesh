---
phase: 01-backend-host-api-contract
plan: 02
subsystem: backend-runtime
tags: [rust, luau, tokio, backend, shell, runtime]
requires:
  - phase: 01-01
    provides: structured backend host APIs inside BackendScriptContext
provides:
  - backend plugin settings wired from shell spawn into mesh.config()
  - live poll interval updates inside the async backend runtime loop
  - bundled backend runtime compatibility coverage without external system commands
affects: [backend-runtime, shell-plugin-spawn, bundled-backend-fixtures]
tech-stack:
  added: []
  patterns: [settings-aware backend spawn, dynamic tokio interval refresh, bundled script compatibility fixtures]
key-files:
  created: [.planning/phases/01-backend-host-api-contract/01-02-SUMMARY.md]
  modified:
    - crates/core/runtime/backend/src/lib.rs
    - crates/core/runtime/scripting/src/backend.rs
    - crates/core/shell/src/shell/mod.rs
key-decisions:
  - "Backend plugin settings come from ShellConfig plugin values only, excluding shell bookkeeping like enabled."
  - "Runtime interval changes rebuild the Tokio interval from now instead of forcing an immediate extra poll tick."
patterns-established:
  - "Backend runtime tests can prove host API integration by spawning real Luau services over channels."
  - "Bundled backend scripts are treated as compatibility fixtures for the public mesh host API surface."
requirements-completed: [HOST-01, HOST-02, HOST-03, HOST-04, HOST-05, HOST-06]
duration: 11 min
completed: 2026-05-01
---

# Phase 01 Plan 02: Backend Host API Contract Summary

**Settings-aware backend spawning, live poll interval reconfiguration, and bundled backend runtime compatibility coverage**

## Performance

- **Duration:** 11 min
- **Started:** 2026-05-01T15:59:00Z
- **Completed:** 2026-05-01T16:09:48Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments
- Threaded per-plugin shell config values into `spawn_backend_service()` so real backend plugins can read them through `mesh.config()`.
- Updated the async backend loop to re-read `ctx.poll_interval_ms().max(50)` after runtime callbacks and rebuild the active Tokio interval when scripts change cadence.
- Added runtime and scripting compatibility coverage for bundled backend fixtures, including the shell-theme backend running through the real service loop.

## Task Commits

Each task was committed atomically:

1. **Task 1: Thread plugin settings from shell spawn to BackendScriptContext** - `744d8f1` (feat)
2. **Task 2: Make set_poll_interval effective while backend runtime is running** - `1634e14` (feat)
3. **Task 3: Add runtime compatibility coverage for bundled backend APIs** - `5e8523a` (test)

## Files Created/Modified
- `crates/core/runtime/backend/src/lib.rs` - Settings-aware backend spawn, dynamic interval refresh, and runtime integration tests.
- `crates/core/runtime/scripting/src/backend.rs` - Bundled backend API-surface compatibility assertions.
- `crates/core/shell/src/shell/mod.rs` - Shell-side plugin settings extraction passed into backend runtime spawn.
- `.planning/phases/01-backend-host-api-contract/01-02-SUMMARY.md` - Plan execution summary.

## Decisions Made
- Used `ShellConfig.plugins[plugin_id].values` as the backend settings source so `mesh.config()` receives only plugin-defined values.
- Recreated updated polling intervals with `interval_at(now + duration, duration)` so runtime cadence changes wait for the new interval instead of generating an immediate extra poll.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed backend runtime tests that closed the command channel before the first poll**
- **Found during:** Task 2
- **Issue:** The initial async backend tests dropped the command sender before the service emitted its first update, letting the runtime exit before assertions ran.
- **Fix:** Kept the command sender alive until after the expected update was observed, then closed it and verified clean task shutdown.
- **Files modified:** `crates/core/runtime/backend/src/lib.rs`
- **Verification:** `cargo test -p mesh-core-backend`
- **Committed in:** `1634e14`

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** The fix was required for the new runtime integration tests to validate the intended behavior. No scope creep.

## Issues Encountered
- `cargo test -p mesh-core-shell --lib` could not be completed in this environment because `smithay-client-toolkit`'s build script could not find the system `xkbcommon` pkg-config dependency.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Backend runtime plumbing now covers real settings propagation, update emission, and live poll cadence changes.
- Phase 2 can build on this contract for frontend service proxy delivery.
- Full shell crate verification in this environment still requires installing the system `xkbcommon` development package.

## Verification

- `cargo test -p mesh-core-scripting config_returns_backend_settings` — passed
- `cargo test -p mesh-core-backend` — passed
- `cargo test -p mesh-core-scripting` — passed
- `rg -n "set_poll_interval|poll_interval_ms|max\\(50\\)" crates/core/runtime/backend/src/lib.rs crates/core/runtime/scripting/src/backend.rs` — passed
- `rg -n "spawn_backend_service\\(" crates/core/shell/src/shell/mod.rs crates/core/runtime/backend/src/lib.rs` — passed
- `cargo test -p mesh-core-shell --lib` — failed due missing system `xkbcommon` pkg-config dependency

## Self-Check: PASSED

- Summary file exists: `.planning/phases/01-backend-host-api-contract/01-02-SUMMARY.md`
- Verified task commits: `744d8f1`, `1634e14`, `5e8523a`

---
*Phase: 01-backend-host-api-contract*
*Completed: 2026-05-01*
