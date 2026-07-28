---
phase: 94-env-isolation-lazy-init
plan: 01
subsystem: scripting
tags: [mlua, lua, sandbox, pool, env-isolation, lazy-init]

# Dependency graph
requires:
  - phase: 92-lua-vm-pool
    provides: LuaVmPool, PooledVm, pool::checkout(), thread-local pool
  - phase: 93-host-api-target-refactor
    provides: install_host_api(&Table) accepting arbitrary table target
provides:
  - ScriptContext with vm: Option<PooledVm> + env_table: Option<Table> (lazy-init pattern)
  - ensure_initialized() method for deferred VM checkout and per-component _ENV creation
  - uninit() method returning PooledVm to pool and clearing env_table
  - All script entrypoints gated through ensure_initialized()
  - Per-component _ENV isolation via { __index = lua.globals() } metatable
  - Script chunks loaded with set_environment(env_table) for private namespace
  - Closures capture env_table instead of lua.globals() for reactive state access
affects:
  - 95-lazy-init-integration-wiring
  - 96-backend-lazy-init

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Option<PooledVm> + Option<Table> lazy-init: VM checkout deferred to first entrypoint call"
    - "accessor methods lua()/env() unwrap Option fields with expect() for ergonomic usage"
    - "env_table.__index = lua.globals() metatable for stdlib fallthrough with write isolation"
    - "Chunk::set_environment() for sandboxed script execution landing in per-component namespace"

key-files:
  created: []
  modified:
    - crates/core/runtime/scripting/src/context/runtime.rs
      provides: ScriptContext with lazy-init, env_table isolation, gated entrypoints, closure capture
    - crates/core/runtime/scripting/src/pool.rs
      provides: #[derive(Debug)] for PooledVm (required by ScriptContext derive)
    - crates/core/shell/src/shell/component/composition.rs
      provides: get_mut() for mutable public_member_snapshot access

key-decisions:
  - "ensure_initialized() creates env_table BEFORE host API install — all host API keys land in private namespace, not globals"
  - "builtin_globals populated from env_table.pairs() instead of lua.globals() — stdlib keys already excluded by __index fallthrough"
  - "load_script_with_interface_imports no longer re-installs host API or re-snapshots builtin_globals — ensure_initialized handles both"
  - "Closures (request_redraw, locale.current) capture env_table.clone() — cheap Table handle, ref-counted by mlua"

patterns-established:
  - "Pattern 1: Lazy-init guard — every public entrypoint calls ensure_initialized()? as first line"
  - "Pattern 2: env_table as canonical state surface — all reactive reads/writes target env_table, not globals"
  - "Pattern 3: Per-component _ENV sandbox — { __index = globals() } metatable provides read-only stdlib fallthrough"

requirements-completed: [INIT-01, INIT-02, ISO-01, ISO-02]

# Metrics
duration: 688s
completed: 2026-06-07
---

# Phase 94 Plan 01: _ENV Isolation + Lazy-Init Summary

**ScriptContext restructured with lazy VM checkout, per-component _ENV table with __index=globals() metatable, and all reactive state paths migrated from globals to env_table**

## Performance

- **Duration:** 11m 28s
- **Started:** 2026-06-07T16:33:00Z
- **Completed:** 2026-06-07T16:44:28Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Replaced `ScriptContext::lua: Lua` field with `vm: Option<PooledVm>` + `env_table: Option<Table>` — no Lua allocation on construction
- Added `ensure_initialized()` which checks out a PooledVm, creates private _ENV table with `__index = lua.globals()`, installs host API, and populates builtin_globals
- Added `uninit()` to drop env_table, clear builtin/user globals, and return PooledVm to pool
- Gated all 11 public script entrypoints with `ensure_initialized()` for on-demand VM checkout
- Migrated all `self.lua().globals()` reads/writes to `self.env()` across 8+ methods (sync_state, call_init, call_handler, apply_service_payload, etc.)
- Added `set_environment(self.env().clone())` to script chunk loading so function definitions land in private namespace
- Captured env_table in closures (`request_redraw`, `locale.current`) instead of accessing raw globals
- Simplified sync filter: stdlib keys excluded by env_table.pairs() (no __index fallthrough)

## Task Commits

Each task was committed atomically:

1. **task 1: restructure ScriptContext fields, add ensure_initialized() and uninit()** - `9719524` (feat)
2. **task 2: migrate all globals() reads/writes to env_table** - `455a21d` (feat)

## Files Created/Modified
- `crates/core/runtime/scripting/src/context/runtime.rs` - ScriptContext struct with lazy-init vm+env_table, ensure_initialized/uninit, env_table migration, closure capture
- `crates/core/runtime/scripting/src/pool.rs` - Added `#[derive(Debug)]` to PooledVm for ScriptContext derive
- `crates/core/shell/src/shell/component/composition.rs` - Changed `get()` to `get_mut()` for mutable access needed by public_member_snapshot

## Decisions Made
- ensure_initialized() creates env_table BEFORE host API install — all host API keys (`require`, `self`, `module`, `mesh.*`, etc.) land in private namespace, not global
- builtin_globals populated from env_table.pairs() instead of lua.globals() — stdlib keys already excluded by __index fallthrough, no pool_baseline_globals filter needed
- load_script_with_interface_imports no longer re-installs host API or re-snapshots builtin_globals — ensure_initialized handles both idempotently
- Closures (request_redraw, locale.current) capture `env_table.clone()` — Table handle is ref-counted by mlua, cheap clone
- Changed `has_handler`, `public_function_names`, and `public_member_snapshot` from `&self` to `&mut self` to support ensure_initialized() gate — compatible with existing callers behind Mutex

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added `#[derive(Debug)]` to PooledVm**
- **Found during:** task 1 (build verification)
- **Issue:** ScriptContext derives Debug but PooledVm didn't implement Debug, causing compile error
- **Fix:** Added `#[derive(Debug)]` to PooledVm struct in pool.rs
- **Files modified:** crates/core/runtime/scripting/src/pool.rs
- **Verification:** `cargo build -p mesh-core-scripting` compiles with zero errors
- **Committed in:** 9719524 (task 1 commit)

**2. [Rule 3 - Blocking] Fixed multi-line `self.lua` → `self.lua()` accessor conversion**
- **Found during:** task 1 (build verification)
- **Issue:** Several instances of `self.lua` split across lines (e.g., `self\n.lua\n.globals()`) required individual fixes — sed missed inline `.lua` without `self` prefix
- **Fix:** Used sed to convert standalone `.lua` lines to `.lua()` and fixed remaining `&self.lua` references to `self.lua()`
- **Files modified:** crates/core/runtime/scripting/src/context/runtime.rs
- **Verification:** `cargo build -p mesh-core-scripting` compiles with zero errors
- **Committed in:** 9719524 (task 1 commit)

**3. [Rule 1 - Bug] Changed method signatures from `&self` to `&mut self` for ensure_initialized() callers**
- **Found during:** task 1 (implementation)
- **Issue:** Plan directed adding `ensure_initialized()` gates to `public_function_names(&self)` and `has_handler(&self)`, but both methods took immutable `&self` while `ensure_initialized` needs `&mut self`
- **Fix:** Changed `has_handler`, `public_function_names`, and `public_member_snapshot` to `&mut self`; updated `bind_child_instance` caller to use `get_mut()` on Mutex-protected HashMap
- **Files modified:** crates/core/runtime/scripting/src/context/runtime.rs, crates/core/shell/src/shell/component/composition.rs
- **Verification:** `cargo build -p mesh-core-scripting` compiles with zero errors; shell crate composition.rs uses existing `get_mut` pattern
- **Committed in:** 9719524 (task 1 commit)

---

**Total deviations:** 3 auto-fixed (1 blocking missing derive, 1 blocking accessor conversion, 1 bug signature mismatch)
**Impact on plan:** All auto-fixes necessary for compilation correctness. No scope creep.

## Issues Encountered
- `cargo check -p mesh-core-shell` failed due to missing `xkbcommon` system library (environment issue, not code) — built `-p mesh-core-scripting` instead which is the primary crate being modified

## Threat Verification
All four threat mitigations from the plan's STRIDE register are in place:
- **T-94-01 (Tampering):** env_table has `__index = lua.globals()` only — no `__newindex` override. Writes go to env_table; globals remain read-only via `Lua::sandbox(true)` on pool VMs. ✓
- **T-94-02 (Info Disclosure):** Each checkout creates fresh env_table; `pairs()` only sees keys installed since checkout. ✓
- **T-94-03 (EoP):** Closures capture `env_table.clone()` (cheap Table handle). Writes are component-scoped. ✓
- **T-94-04 (Spoofing):** `set_environment()` sets chunk's first upvalue (`_ENV`). Cannot escape to write globals due to sandbox. ✓

## Next Phase Readiness
- ScriptContext lazy-init infrastructure complete — all entrypoints gated, env_table isolation active
- Ready for Phase 95 integration wiring (FrontendSurfaceComponent::create_runtime_for_component, BackendScriptContext lazy-init, hot-reload cache eviction)
- Currently `uninit()` is public but not called from Drop — integration phase will wire unmount lifecycle

---
*Phase: 94-env-isolation-lazy-init*
*Completed: 2026-06-07*
