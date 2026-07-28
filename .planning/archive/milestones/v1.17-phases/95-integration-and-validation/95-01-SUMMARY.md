---
phase: 95-integration-and-validation
plan: 01
subsystem: runtime
tags: [luau, mlua, lazy-init, scripting]

# Dependency graph
requires:
  - phase: 94-env-isolation-lazy-init
    provides: "ScriptContext lazy-init pattern (ensure_initialized gate)"
provides:
  - "BackendScriptContext defers Lua::new() until first init()/poll invocation"
  - "ensure_lua() gate pattern for backend singleton Lua VMs"
affects: [backend-providers, scripting-runtime]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Lazy Lua allocation: Option<Lua> + ensure_lua() gate before every Lua access"
    - "ensure_lua stores Lua in self.lua before calling install_host_api to prevent recursion"

key-files:
  created: []
  modified:
    - crates/core/runtime/scripting/src/backend/runtime.rs
    - crates/core/runtime/scripting/src/backend/tests.rs

key-decisions:
  - "ensure_lua() stores Lua in self.lua before calling install_host_api to prevent recursion in methods that chain through ensure_lua themselves"
  - "Fields borrowed by install_host_api (storage, module_id) are cloned before ensure_lua() to satisfy borrow checker"
  - "take_service_state_snapshot, public_function_names, current_self_table changed from &self to &mut self since they now gate through ensure_lua"

patterns-established:
  - "ensure_lua pattern: check Option, create if None, install host API, populate builtin globals, store, return &Lua"

requirements-completed: [INIT-03]

# Metrics
duration: 5min
completed: 2026-06-07
---

# Phase 95 Plan 01: Lazy-init BackendScriptContext Summary

**BackendScriptContext defers Lua VM allocation to first init()/poll invocation, eliminating idle allocations for registered-but-unused backend providers**

## Performance

- **Duration:** 5 min
- **Started:** 2026-06-07T20:07:20+02:00
- **Completed:** 2026-06-07T20:12:22+02:00
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- `BackendScriptContext` struct changed from `lua: Lua` to `lua: Option<Lua>` with `None` at construction
- `ensure_lua()` gate method creates Lua VM on first access, installs host API, populates builtin_globals
- All 10 public entrypoints gated through `ensure_lua()` — no direct `self.lua.` access outside the gate
- Three method signatures updated (`take_service_state_snapshot`, `public_function_names`, `current_self_table` → `&mut self`)
- All 63 existing backend tests pass unchanged — zero assertion modifications needed

## Task Commits

1. **task 1: restructure BackendScriptContext struct and add ensure_lua gate** - `1333e7d` (refactor)
2. **task 2: gate all entrypoints through ensure_lua and update tests** - `9eaaebf` (refactor)

## Files Created/Modified
- `crates/core/runtime/scripting/src/backend/runtime.rs` - Struct field changed, constructor simplified, `ensure_lua()` added, all entrypoints gated
- `crates/core/runtime/scripting/src/backend/tests.rs` - All `ctx.lua.X()` → `ctx.ensure_lua().X()` (7 call sites)

## Decisions Made
- Lua is stored in `self.lua` *before* calling `install_host_api` to prevent infinite recursion (install_host_api methods chain through `ensure_lua` themselves)
- `run_command_with_result` restructured to clone `module_id` before `ensure_lua()` to satisfy Rust borrow checker (cannot hold `&Lua` and `&self.module_id` simultaneously)
- `current_self_table` clones `Arc<Mutex<ScopedStorage>>` before `ensure_lua()` for same borrow checker reason

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Stack overflow from recursive ensure_lua → install_host_api → ensure_lua**
- **Found during:** task 2 (initial test run)
- **Issue:** `ensure_lua()` called `install_host_api()` before storing Lua in `self.lua`. `install_host_api` uses `self.ensure_lua().create_table()`, which found `self.lua: None` and created another Lua VM, recursing infinitely.
- **Fix:** Store `self.lua = Some(lua)` before calling `install_host_api`, then access globals via `self.lua.as_ref().unwrap().globals()`.
- **Files modified:** crates/core/runtime/scripting/src/backend/runtime.rs
- **Committed in:** 9eaaebf (task 2 commit)

**2. [Rule 1 - Bug] Borrow checker conflict: &Lua from ensure_lua vs &self.module_id**
- **Found during:** task 2 (first build attempt)
- **Issue:** `command_result_from_lua(self.ensure_lua(), &self.module_id, returned)` — the `&Lua` from `ensure_lua()` and `&self.module_id` cannot coexist (Rust treats `ensure_lua(&mut self) -> &Lua` as holding a mutable borrow).
- **Fix:** Clone `module_id` before `ensure_lua()` call, pass clone to `command_result_from_lua`.
- **Files modified:** crates/core/runtime/scripting/src/backend/runtime.rs
- **Committed in:** 9eaaebf (task 2 commit)

**3. [Rule 3 - Blocking] Multiline `self.lua\n.create_function(` patterns not caught by single-line replace**
- **Found during:** task 2 (build error)
- **Issue:** Several `self.lua.create_function()` calls in `install_host_api` were split across lines and missed by the initial replaceAll for `self.lua.create_function(`. Same for one test file `ctx2.lua\n.globals()`.
- **Fix:** Targeted multiline edits for each remaining instance.
- **Files modified:** crates/core/runtime/scripting/src/backend/runtime.rs, tests.rs
- **Committed in:** 9eaaebf (task 2 commit)

---

**Total deviations:** 3 auto-fixed (2 bugs, 1 blocking)
**Impact on plan:** All auto-fixes within scope — no architectural changes needed.

## Issues Encountered
- Rust borrow checker required restructuring `run_command_with_result` and `current_self_table` to avoid holding `&Lua` and `&self.field` simultaneously. Resolved by cloning borrowed fields before `ensure_lua()`.
- `public_function_names` and `current_self_table` needed `&self` → `&mut self` signature changes, which cascaded to callers (all were already `&mut self` methods, so no caller changes needed).

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `BackendScriptContext` now lazily initializes Lua VMs per INIT-03
- All existing behavior preserved (63/63 tests pass)
- No caller-side changes needed — API is backward compatible for all public entrypoints

---
*Phase: 95-integration-and-validation*
*Completed: 2026-06-07*
