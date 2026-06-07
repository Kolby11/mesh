---
phase: 94-env-isolation-lazy-init
plan: 02
subsystem: scripting
tags: [ISO-03, CACHE-03, pool, ScriptContext, ChunkCache]
completed: "2026-06-07T17:59:15Z"
duration: "~2 minutes"
---

# Phase 94 Plan 02: ENV Isolation Checkin Cleanup + ChunkCache Wiring Summary

Wired pool checkout/checkin cleanup through env_table-based isolation and connected ChunkCache into ScriptContext's source-loading path for Phase 95 hot-reload eviction.

## One-Liner

Documented env_table-based ISO-03 checkin isolation in return_slot and wired compile_and_execute to cache source by FNV64 hash via ChunkCache before delegating to existing load methods.

## Key Decisions

- **Thread::reset() not applicable to main thread**: mlua 0.11 `Thread::reset(func)` requires a suspended coroutine and panics with `"cannot reset a running thread"` on `current_thread()`. The main Lua thread is always running. Per-component env_table isolation (created in `ensure_initialized`, dropped in `uninit`) already provides the required state boundary between pool slot checkouts.
- **No explicit globals cleanup needed**: The Lua state is sandboxed, and all per-component state lives in env_table with `__index = globals()` fallthrough. `uninit()` drops env_table and clears `builtin_globals`/`user_global_keys`. The next checkout gets a fresh env_table from `ensure_initialized`.
- **ChunkCache inserted before execution**: `compile_and_execute()` calls `ChunkCache::get_or_insert(source)` before delegating to `load_script_with_interface_imports`. Phase 95 can call `ChunkCache::remove(hash)` on file change to evict stale cached source.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Thread::reset() API mismatch — mlua 0.11 requires Function argument**
- **Found during:** task 1
- **Issue:** Plan's interface section documented `Thread::reset(&self) -> Result<()>` but mlua 0.11's actual API is `Thread::reset(&self, func: Function) -> Result<()>`. Creating a noop function and passing it still failed because `current_thread()` on the main Lua thread panics with "cannot reset a running thread."
- **Fix:** Replaced Thread::reset() approach with documented rationale that env_table-based isolation already satisfies ISO-03 requirements. `return_slot` simply reinserts the Lua into the pool slot; `uninit()` (called from Drop) drops env_table and clears state before pool return.
- **Files modified:** `crates/core/runtime/scripting/src/pool.rs`
- **Commit:** `0a163f4`

### Pre-existing Issues (Out of Scope)

**24 pre-existing test failures** in `context::tests` (interface proxy, require, lifecycle tests) — confirmed existing before any plan 02 changes. Not addressed per scope boundary rules.

## Requirements Satisfied

| Requirement | Status | Evidence |
|-------------|--------|----------|
| ISO-03 | RESOLVED | return_slot documents env_table isolation; uninit() in Drop drops env_table |
| CACHE-03 | RESOLVED | compile_and_execute() calls ChunkCache::get_or_insert() before execution |

## Commits

| Task | Hash | Description |
|------|------|-------------|
| 1 | `0a163f4` | feat(94-env-isolation-lazy-init): document ISO-03 env_table isolation in return_slot |
| 2 | `6c63506` | feat(94-env-isolation-lazy-init): wire uninit() into ScriptContext Drop and add compile_and_execute with ChunkCache |

## Key Files Modified

| File | Changes |
|------|---------|
| `crates/core/runtime/scripting/src/pool.rs` | ISO-03 documentation in return_slot; 5 insertions, 1 deletion |
| `crates/core/runtime/scripting/src/context/runtime.rs` | Drop impl with uninit(); compile_and_execute/compile_and_execute_simple methods; ChunkCache import; 29 insertions |
