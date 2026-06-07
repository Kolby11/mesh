# Phase 94: _ENV Isolation + Lazy-Init - Context

**Gathered:** 2026-06-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Each component gets a private `_ENV` table on VM pool checkout so writes from one component are invisible to any other component sharing the same pool VM; components that are never mounted hold no pool slot.

Replace `ScriptContext.lua: Lua` with `vm: Option<PooledVm>` + `env: Option<Table>`, deferring VM checkout until `ensure_initialized()` at first script entry. Build on Phase 93's `install_host_api(&Table)` refactor to target the per-component `_ENV` table. Wire checkin cleanup (env drop + thread reset) and ChunkCache integration at source-load time. Hot-reload cache eviction called from the existing shell mtime watcher, not inside ChunkCache.

Covers requirements: ISO-01, ISO-02 (completion), ISO-03, INIT-01, INIT-02, CACHE-03.

</domain>

<decisions>
## Implementation Decisions

### ScriptContext Lazy-Init Pattern
- Replace `lua: Lua` with `vm: Option<PooledVm>` + `env: Option<Table>` — both `None` until `ensure_initialized()` is called.
- `ensure_initialized()` is called at the top of every public script entrypoint (`sync_state_to_lua`, `compile_and_execute`, `call_lifecycle_handler`, etc.) — every path touching the Lua VM checks first.
- `PooledVm` checkout happens inside `ensure_initialized()` via `pool::checkout()`, paired with sandbox + host API install.
- `sync_state_from_lua` filter combines `pool_baseline_globals` (Phase 93) + `builtin_globals` (existing) — skip keys present in either set to distinguish reactive state from stdlib.

### _ENV Isolation Implementation
- Create per-component `_ENV` by: `lua.create_table()` → set metatable with `__index = lua.globals()` → call `Thread::sandbox(env_table)` to sandbox the thread with the component's `_ENV`.
- Env table uses `{ __index = lua.globals() }` metatable so stdlib reads fall through to shared read-only globals while writes stay local.
- `install_host_api` targets `&env_table` (per Phase 93 refactor) so all host API keys (`require`, `self`, `module`, `mesh.*`, etc.) are installed in the component's private namespace.
- Closure bodies that read `lua.globals()` (e.g., `mesh_ui_api.request_redraw`, `mesh_locale.current`) must change to `env_table` reads — they need per-component `_ENV` to find reactive fields.

### Checkin Cleanup Protocol
- In `LuaVmPool::return_slot`: call `Thread::reset()` to clear the main thread's state, drop the env table, then reinsert the Lua into the pool slot.
- `env_table` is stored in `ScriptContext` as `Option<Table>` — explicit `uninit()` method drops env + drops PooledVm (which returns to pool). Backend: called on `stop()`.
- `builtin_globals` populated during `ensure_initialized()` (post host-api install via a globals walk), not at `ScriptContext::new` since the Lua doesn't exist yet.

### Cache & Source Loading Integration
- `ChunkCache::get_or_insert(source)` called during `compile_and_execute` — cache-first lookup: if hit → use cached source; if miss → read from disk, `get_or_insert`, then use.
- CACHE-03 hot-reload eviction: existing shell mtime watcher calls `ChunkCache::remove(hash)` when source file changes; next `compile_and_execute` does fresh `get_or_insert`.
- Eviction called from the shell's existing mtime watcher or `FrontendSurfaceComponent` reload path — no ChunkCache-internal filesystem awareness.

### OpenCode's Discretion
- Exact Thread::reset() API mapping in mlua v0.11 (may be `Thread::reset` or equivalent).
- Whether `uninit()` is called from `Drop` or an explicit method — implementer's choice based on ownership patterns.
- Format of `builtin_globals` population at `ensure_initialized()` time — follow existing Phase 93 D-06 convention.

</decisions>

<code_context>
## Existing Code Insights

### Integration Points
- `crates/core/runtime/scripting/src/context/runtime.rs` — `ScriptContext` struct, `install_host_api`, `ensure_initialized`, `sync_state_from_lua`, `compile_and_execute`
- `crates/core/runtime/scripting/src/backend/runtime.rs` — `BackendScriptContext` struct, `install_host_api`, lazy-init
- `crates/core/runtime/scripting/src/pool.rs` — `LuaVmPool`, `PooledVm`, `return_slot` (needs `Thread::reset()` addition)
- `crates/core/runtime/scripting/src/chunk_cache.rs` — `ChunkCache::get_or_insert`, `ChunkCache::get`, `ChunkCache::remove` (all exist from Phase 92)

### Reusable Assets
- `install_host_api(&Table)` — Phase 93 already refactored both frontend and backend to accept a `&Table` target; now just pass `&env_table` instead of `&lua.globals()`
- `LuaVmPool::baseline_globals()` — Arc<HashSet<String>> already captured at pool construction (Phase 93)
- `pool::checkout()` — thread-local PooledVm acquisition (Phase 92)
- `ChunkCache::get_or_insert`, `get`, `remove` — process-wide source cache (Phase 92)

### Established Patterns
- `builtin_globals: HashSet<String>` in `ScriptContext` — populated post-host-api-install via globals walk; same pattern at `ensure_initialized()` time
- RAII guard pattern in `PooledVm` — Drop returns Lua to pool; same pattern for `ScriptContext` Drop calling `uninit()`
- Option-based lazy-init: `vm: Option<PooledVm>`, `env: Option<Table>`

</code_context>

<specifics>
## Specific Ideas

- The env table should be created BEFORE host API install — set up the `__index = globals()` metatable, sandbox the thread, then call `install_host_api(&env_table)` so all keys land in the per-component namespace.
- `sync_state_from_lua` currently walks `lua.globals()` — with `_ENV`, it should walk `env_table` since that's where reactive state is rooted. The filter still uses `pool_baseline_globals` + `builtin_globals`.
- `PooledVm` changes: the `PooledVm` type may need to expose `lua_mut()` or equivalent for the thread reset operation in `return_slot`.
- Backend `ScriptContext` gets `vm: Option<PooledVm>` + `env: Option<Table>` + `ensure_initialized()` — same pattern as frontend. Backend contexts are long-lived but still use the pool for frontend parity; INIT-03 (backend lazy-init) is Phase 95.

</specifics>

<deferred>
## Deferred Ideas

- `BackendScriptContext` deferring `Lua::new()` → Phase 95 (INIT-03)
- Live integration wiring into `FrontendSurfaceComponent::create_runtime_for_component` → Phase 95 (INT-01)
- Shipped surface regression proof → Phase 95 (INT-02)
- Hot-reload mtime watcher CACHE-03 wiring → Phase 95 (the remove() call site lives in shell code)
</deferred>

---

*Phase: 94-_ENV Isolation + Lazy-Init*
*Context gathered: 2026-06-07*
