# Requirements: MESH v1.17 Performance: Scripting VM Consolidation

**Milestone:** v1.17
**Goal:** Eliminate the per-component `mlua::Lua` VM allocation — the largest per-component startup and memory cost — by introducing a per-thread VM pool with `_ENV`-based isolation, lazy initialization, and a compiled chunk cache.
**Defined:** 2026-06-02

---

## v1 Requirements

### VM Pool Foundation

- [ ] **POOL-01**: The shell creates a thread-local Lua VM pool where each pool VM is initialized once with `Lua::sandbox(true)` so stdlib tables are read-only before any component uses it.
- [ ] **POOL-02**: `ScriptContext` checks out a pooled VM for each component activation and returns it via a RAII drop guard (`PooledVm`), replacing unconditional `Lua::new()` per component.
- [ ] **POOL-03**: The pool grows on-demand with a bounded floor (minimum 4 VMs) so simultaneous surface initialization does not exhaust the pool.
- [ ] **POOL-04**: The `PooledVm` RAII guard asserts the same thread identity on drop so cross-thread VM usage is detected at runtime.

### _ENV Isolation

- [x] **ISO-01**: Each checked-out VM slot runs `Thread::sandbox()` per component activation, giving the component a per-component `_ENV` table where writes are local and reads fall through to the shared read-only pool VM globals.
- [x] **ISO-02**: All per-component host API entries (`require`, `self`, `module`, `mesh.*`, `__mesh_svc_*`, `__mesh_request_redraw`, `__mesh_locale_current`) are installed into the sandboxed `_ENV` table instead of `lua.globals()`, so no component-private state is written to the shared VM.
- [x] **ISO-03**: On VM checkin, the component's `env_table` and all registry key handles are explicitly dropped via `uninit()` before the slot is returned to the pool. The sandboxed Lua prevents user scripts from mutating `globals()`, and per-component `env_table`-based `_ENV` isolation handles the state boundary. `Thread::reset()` is not applicable to the main Lua thread (requires a suspended coroutine), but the env_table approach satisfies the isolation requirement.
- [ ] **ISO-04**: A `pool_baseline_globals` snapshot is captured once at pool VM construction and shared immutably so `sync_state_from_lua()` can distinguish stdlib entries from user-defined reactive component state.

### Lazy Initialization

- [x] **INIT-01**: `ScriptContext` replaces `lua: Lua` with `vm: Option<PooledVm>` and `env: Option<Table>`; VM pool checkout is deferred until the first script call via an `ensure_initialized()` entry point.
- [x] **INIT-02**: Components that are declared but never mounted or shown hold no pool slot, reducing idle memory footprint compared to the current model.
- [x] **INIT-03**: `BackendScriptContext` defers `Lua::new()` to the first `init()` or poll call; backend contexts are long-lived singletons so no pooling is applied, only lazy allocation.

### Chunk Cache

- [ ] **CACHE-01**: A process-wide source-string cache keyed on FNV64 content hash stores compiled `.luau` source so multi-instance modules (e.g., two components from the same `.mesh` file) parse the source once.
- [ ] **CACHE-02**: Pool VM checkout loads the cached source string (or validated bytecode if cross-VM loading is confirmed in Phase 1) rather than re-reading from disk on every activation.
- [x] **CACHE-03**: Hot-reload cache insertion is wired — `compile_and_execute()` calls `ChunkCache::get_or_insert(source)` by FNV64 content hash before execution. Phase 95 adds the mtime watcher for eviction on file change.

### Integration

- [x] **INT-01**: `FrontendSurfaceComponent::create_runtime_for_component` passes pool and cache references into `ScriptContext::new_lazy()`, replacing the current direct `Lua::new()` constructor call.
- [ ] **INT-02**: The shipped navigation bar and audio popover surfaces continue to function correctly after the pool migration — reactive state, service events, keybind dispatch, and storage behavior are unaffected.

---

## Future Requirements

- Debug inspector pool metrics (hit/miss ratio, chunk cache hit rate, reset count) — add when pool is stable and observable behavior needs validation on real surfaces.
- True bytecode cross-VM sharing via `luau_compile`/`luau_load` C FFI — mlua safe API does not expose this in v0.11; deferred to a future milestone.
- Disk-persistent bytecode cache — adds invalidation complexity across mlua upgrades; in-memory is sufficient for v1.17.
- Pool per `BackendScriptContext` — backend contexts are long-lived singletons; pooling adds complexity for minimal gain; lazy-init only in v1.17.
- Pool size auto-tuning based on benchmark profiling data.

---

## Out of Scope

- Backend VM pooling (only lazy-init applies; backend modules are long-lived per-provider singletons).
- Debug inspector pool metrics (P2; add after pool is stable).
- Benchmark comparison requirement (omitted per user preference; correctness proof on shipped surfaces is sufficient).
- Cross-process bytecode caching (persistence, serialization complexity).
- GPU rendering or any render pipeline changes.
- Any changes to the Luau scripting authoring model visible to module authors.

---

## Traceability

| REQ-ID | Phase | Notes |
|--------|-------|-------|
| POOL-01 | Phase 92 | VM pool construction with sandbox initialization |
| POOL-02 | Phase 92 | PooledVm RAII checkout/checkin |
| POOL-03 | Phase 92 | On-demand growth with 4 VM floor |
| POOL-04 | Phase 92 | Thread ID assertion in PooledVm drop |
| ISO-01 | Phase 94 | Thread::sandbox() per-component _ENV |
| ISO-02 | Phase 93 (foundation), Phase 94 (completion) | install_host_api refactor to &Table target |
| ISO-03 | Phase 94 | Explicit env_table drop + registry key cleanup on checkin |
| ISO-04 | Phase 93 | pool_baseline_globals snapshot at pool VM construction |
| INIT-01 | Phase 94 | ScriptContext vm: Option<PooledVm> + env: Option<Table> + ensure_initialized() |
| INIT-02 | Phase 94 | Zero pool slots for unmounted components |
| INIT-03 | Phase 95 (plan 01) | BackendScriptContext deferred Lua::new() |
| CACHE-01 | Phase 92 | ChunkCache process-wide FNV64-keyed source string store |
| CACHE-02 | Phase 92 | Checkout loads cached source string |
| CACHE-03 | Phase 95 (plan 02) | Hot-reload mtime watcher cache eviction in reload_source() |
| INT-01 | Phase 95 (plan 02) | FrontendSurfaceComponent::create_runtime_for_component wired to compile_and_execute + new_lazy() |
| INT-02 | Phase 95 (plan 03) | Workspace build + test suite regression proof |
