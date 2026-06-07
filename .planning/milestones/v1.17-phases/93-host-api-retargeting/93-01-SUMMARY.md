---
phase: 93-host-api-retargeting
plan: 01
subsystem: scripting
tags: [mlua, lua-vm-pool, host-api, sandbox, hashset, arc]

# Dependency graph
requires:
  - phase: 92-vm-pool-foundation
    provides: "LuaVmPool struct with sandboxed slots"
provides:
  - "LuaVmPool::baseline_globals() returning Arc<HashSet<String>> of stdlib key names captured before sandbox(true)"
  - "ScriptContext::install_host_api(&mlua::Table) with caller-supplied target table parameter"
affects:
  - 94-per-component-env
  - scripting isolation

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Capture Lua globals before sandbox(true) — Luau luaL_sandbox replaces the globals table with a read-only proxy, making raw iteration return nothing after the call"
    - "Pass &Table to install_host_api so Phase 94 can target per-component _ENV tables instead of globals"

key-files:
  created: []
  modified:
    - crates/core/runtime/scripting/src/pool.rs
    - crates/core/runtime/scripting/src/context/runtime.rs

key-decisions:
  - "Collect baseline globals BEFORE sandbox(true): luaL_sandbox installs a read-only metatable proxy over the globals table, making pairs() return an empty iterator afterward"
  - "install_host_api takes &mlua::Table not Table to avoid consuming globals; call site splits borrow with let globals = self.lua.globals() before the &mut self call"

patterns-established:
  - "Pre-sandbox globals walk: create Lua::new(), walk globals(), THEN call sandbox(true)"
  - "Table-targeted host API: install_host_api(&target) installs all API keys into caller-supplied table for reuse with per-component _ENV in Phase 94"

requirements-completed: [ISO-02, ISO-04]

# Metrics
duration: 15min
completed: 2026-06-07
---

# Phase 93 Plan 01: Host API Re-targeting Summary

**LuaVmPool captures pre-sandbox stdlib key snapshot and ScriptContext::install_host_api retargeted to accept a caller-supplied &mlua::Table**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-06-07T15:40:00Z
- **Completed:** 2026-06-07T15:55:00Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Added `baseline_globals: Arc<HashSet<String>>` to `LuaVmPool` with 3 new tests
- Discovered that Luau's `luaL_sandbox` replaces the globals table with a read-only proxy that makes `pairs()` return nothing — globals must be collected BEFORE calling `sandbox(true)`
- Refactored `ScriptContext::install_host_api` to accept `&mlua::Table` parameter instead of always installing into `self.lua.globals()`; call site unchanged in behavior

## Task Commits

1. **Task 1+2: Add baseline_globals and retarget install_host_api** — `f0a6cc0` (feat)

Note: pool.rs Task 1 changes were previously bundled into commit `daa335a` by a prior agent execution; this execution committed the remaining Task 2 (context/runtime.rs) changes.

## Files Created/Modified

- `crates/core/runtime/scripting/src/pool.rs` — Added `baseline_globals` field, pre-sandbox globals capture, accessor, and 3 tests
- `crates/core/runtime/scripting/src/context/runtime.rs` — Changed `install_host_api` signature to `(&mut self, target: &mlua::Table)`, updated call site

## Decisions Made

- Collect globals before `sandbox(true)` not after: Luau's sandbox mechanism installs a frozen metatable proxy over the globals table, making subsequent `pairs()` calls return nothing
- Closure bodies that call `lua.globals()` at invocation time (request_redraw, locale.current) are intentionally left unchanged — they read from globals at call time, not at install time

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Collect baseline globals before sandbox(true)**
- **Found during:** Task 1 (baseline_globals_is_non_empty test RED phase)
- **Issue:** Plan specified creating baseline VM and calling `sandbox(true)` before walking globals. Luau's `luaL_sandbox` installs a read-only metatable proxy over the globals table making `pairs()` return an empty iterator
- **Fix:** Moved the `globals().pairs()` collection to run before `sandbox(true)` call
- **Files modified:** crates/core/runtime/scripting/src/pool.rs
- **Verification:** `baseline_globals_is_non_empty` and `baseline_globals_contains_stdlib` tests pass
- **Committed in:** f0a6cc0

---

**Total deviations:** 1 auto-fixed (Rule 1 - bug in plan's prescribed order)
**Impact on plan:** Essential correctness fix. No scope creep.

## Issues Encountered

- Prior agent execution bundled pool.rs (Task 1) into the 93-02 docs commit; this execution committed only the remaining context/runtime.rs changes

## Next Phase Readiness

- Phase 94 (per-component _ENV) can now use `pool.baseline_globals()` to distinguish stdlib from user globals in `sync_state_from_lua`
- Phase 94 can call `install_host_api(&env_table)` to populate per-component environment tables

---
*Phase: 93-host-api-retargeting*
*Completed: 2026-06-07*
