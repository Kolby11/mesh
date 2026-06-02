# Project Research Summary

**Project:** MESH v1.17 Performance: Scripting VM Consolidation
**Domain:** Luau VM pooling, _ENV isolation, lazy-init, compiled chunk caching in a Rust/mlua shell runtime
**Researched:** 2026-06-02
**Confidence:** HIGH

## Executive Summary

MESH v1.17 eliminates the primary scripting startup cost: each `ScriptContext::new()` currently calls `mlua::Lua::new()`, which initializes a full Luau VM — stdlib tables, allocator state, metatables — for every mounted component, regardless of whether that component ever executes a script. For a shell with a navigation bar, audio popover, and a handful of embedded component instances, this means 5–10 or more full VMs allocated on startup. The v1.17 goal is to replace this one-VM-per-component model with a thread-local pool of shared VMs, per-component `_ENV` table isolation, lazy initialization, and a compiled chunk cache. All required APIs (`Chunk::set_environment`, `Compiler::compile`, `Lua::sandbox`, `ChunkMode::Binary`) are present in the already-vendored `mlua 0.11` crate — no dependency changes are needed.

The recommended implementation order builds from a stable foundation: construct the `LuaVmPool` and `PooledVm` types first, add the `ChunkCache` second, then refactor `install_host_api` to write into a per-component env table instead of `lua.globals()`, and finally wire lazy-init and `_ENV` isolation into `ScriptContext`. The single highest-risk change is the `install_host_api` refactor: every `lua.globals().set(...)` call for per-component state (`__mesh_svc_*`, `__mesh_request_redraw`, `__mesh_locale_current`, `require`, `self`, `module`, `mesh`) must move to the component's isolated `env_table` before the pool produces correct behavior. There is one significant nuance from the architecture research: mlua's safe API does not expose raw Luau bytecode bytes via `Compiler::compile()` in the expected cross-VM-sharing sense — the practical v1.17 chunk cache stores source strings keyed by content hash, not compiled bytecode. True bytecode sharing via `luau_compile`/`luau_load` C FFI is deferred to a future milestone.

The key risks are all implementation-level, not design-level. VM state contamination across pool slots is the highest-severity risk, but it is fully preventable by enforcing explicit reset on VM checkin and by never writing component state to `lua.globals()`. The thread-locality invariant must be enforced from day one: the shell's render loop is single-threaded today, but `mesh-core-scripting` already has the `send` feature enabled, which means a `thread_local!` pool will coexist safely while future-proofing against backend scripting paths that run on Tokio task threads.

## Key Findings

### Recommended Stack

No new crate dependencies are required. `mlua 0.11` already provides every needed primitive: `Chunk::set_environment()` for per-component `_ENV` redirection, `Lua::sandbox(true)` to freeze stdlib tables on pool VMs, `Lua::new_with(StdLib::BASE | STRING | TABLE | MATH, ...)` to create stripped-down pool VMs, and the `mlua::Compiler` struct for source-to-bytecode compilation. The Rust standard library supplies `thread_local!` + `RefCell<Vec<Lua>>` for the zero-overhead per-thread pool and `OnceLock<Mutex<HashMap<...>>>` for the global chunk cache.

**Core technologies:**
- `mlua 0.11` (already in workspace): All four VM consolidation APIs — `Chunk::set_environment`, `Lua::sandbox`, `Compiler::compile`, `ChunkMode::Binary`; no version bump needed
- `std::thread_local!` + `RefCell<Vec<Lua>>`: Zero-cost per-thread pool ownership; no lock contention on the single render thread; avoids the `Arc<Mutex<>>` overhead that the `send` feature would otherwise require
- `std::sync::OnceLock<Mutex<HashMap<u64, String>>>`: Process-wide chunk cache keyed on FNV64 content hash; source strings in v1.17, upgradeable to bytecode bytes when C FFI path is taken

**Critical API caveat (ARCHITECTURE.md, LOW confidence):** mlua's safe Rust API does not directly expose `luau_compile` output as extractable `Vec<u8>` bytes that can be re-loaded into a different VM via `set_mode(ChunkMode::Binary)`. STACK.md treats this as available; ARCHITECTURE.md's direct source analysis rates it LOW confidence. The safe fallback — caching source strings and re-parsing per VM — is fully sufficient for v1.17 and avoids this uncertainty.

### Expected Features

All six items below are P1 (must-have) for the milestone to be considered correct and complete.

**Must have (table stakes):**
- Thread-local VM pool replacing `Lua::new()` per component — eliminates the largest startup cost; all other improvements build on this
- `_ENV` table isolation per component checkout — correctness requirement; without it, two components sharing a VM see each other's reactive globals
- Host API re-injection into `env_table` on every checkout — correctness requirement; `require`, `self`, `module`, `mesh.*`, `__mesh_svc_*` must target the component's private table, not `lua.globals()`
- Lazy-init (`Option<PooledVm>` in `ScriptContext`) — removes allocation cost for hidden surfaces; straightforward Rust ownership change
- Compiled chunk cache keyed on content hash — amortizes parse cost for multi-instance components (e.g., two notification rows from the same `.mesh` source)
- Explicit VM reset on checkin (`env_table` drop + registry key cleanup) — prevents stale GC objects and event channel callbacks from bleeding into the next component on the same VM

**Should have (correctness/quality):**
- Pool size growth-on-demand with a reasonable floor (4–16 VMs) — prevents deadlock at startup when all visible surfaces initialize simultaneously
- `pool_baseline_globals` snapshot captured at pool VM creation and shared immutably — prevents `sync_state_from_lua()` from misclassifying stdlib entries as reactive component state
- Thread ID assertion in `PooledVm::drop` — detects accidental cross-thread pool usage early

**Defer (v2+):**
- True bytecode sharing via `luau_compile`/`luau_load` C FFI — safe mlua API does not expose this; not needed for v1.17
- Backend VM pooling — backend modules are long-lived singletons; apply lazy-init only
- Disk-persistent bytecode cache — adds invalidation complexity across mlua upgrades; in-memory is sufficient
- Debug inspector pool metrics (hit rate, reset count) — add when pool is stable

### Architecture Approach

The change surface is narrow: `ScriptContext` in `crates/core/runtime/scripting/src/context/runtime.rs` is the only file with structural changes. Two new files are added to `mesh-core-scripting` (`pool.rs` and `chunk_cache.rs`). `FrontendSurfaceComponent` in `mesh-core-shell` gets a one-line change to pass pool and cache refs into `ScriptContext::new_lazy()`. `BackendScriptContext` gets lazy-init only, no pooling. The pool is recommended as a `thread_local!` in `mesh-core-scripting`, avoiding the need to thread ownership through every constructor call.

**Major components:**
1. `LuaVmPool` + `PooledVm` (`pool.rs`, new) — holds `Vec<Lua>` with checkout/checkin RAII; VMs are sandbox-initialized once at construction; per-component setup happens via `env_table` on every checkout
2. `ChunkCache` (`chunk_cache.rs`, new) — `Arc<Mutex<HashMap<u64, String>>>` keyed on FNV64 source hash; stores source strings in v1.17; avoids repeated file reads and enables future bytecode elevation
3. `ScriptContext` (modified) — `lua: Lua` replaced with `vm: Option<PooledVm>` + `env: Option<Table>` + `initialized: bool`; new `ensure_initialized()` entry point; `install_host_api` refactored to accept `&Table` target

**Build order:**
1. `LuaVmPool` + `PooledVm` — no external deps, testable in isolation
2. `ChunkCache` — depends only on `std`, source-string cache first
3. `install_host_api` refactor — change target from `lua.globals()` to `&Table`; pass `lua.globals()` temporarily to preserve existing behavior
4. `_ENV` isolation + lazy-init — highest-risk step; replace `lua: Lua`, wire `ensure_initialized()`
5. `BackendScriptContext` lazy-init — simpler, no pooling
6. Wire pool/cache into `FrontendSurfaceComponent` — update `create_runtime_for_component` call site

### Critical Pitfalls

1. **VM state contamination via `lua.globals()` writes** — every `lua.globals().set(...)` call in `install_host_api()` and all host API closure callbacks must be audited and redirected to `env_table`; writing `__mesh_svc_audio`, `__mesh_request_redraw`, or `__mesh_locale_current` to the pool VM's shared `_G` corrupts all other components on that VM; recovery cost is HIGH if discovered late

2. **Stale GC objects on VM checkin causing cross-component state bleed** — do not rely on GC timing; the checkin sequence must explicitly drop `env_table`, remove tracked `RegistryKey` handles, and wipe all `__mesh_*` sentinel entries; otherwise event channel callbacks from the previous component fire during the next component's execution on the same VM

3. **Chunk-as-Function is VM-bound** — caching `mlua::Function` handles is undefined behavior when loaded into a different `Lua` instance; the cache must store `Vec<u8>` bytecode bytes (or source strings) with no VM affinity; enforce this via the cache type signature so it is impossible to store a `Function` in it

4. **Hot-reload does not invalidate the chunk cache by default** — the existing source-path mtime watcher clears the compiled module tree but knows nothing about the bytecode/source cache; wire cache eviction to the same path-change notification; use content hash as cache key so the old entry becomes unreachable on source change

5. **`sync_state_from_lua()` globals scan pollution from pool VM baseline** — the `builtin_globals` snapshot must be captured at pool VM creation time (before any per-component host API installation) and shared immutably; a snapshot taken too late will misclassify stdlib entries (`"string"`, `"math"`, `"require"`) as user-defined reactive state

## Implications for Roadmap

Based on combined research, the milestone maps to four sequential phases that respect the build-order dependencies identified in ARCHITECTURE.md. Each phase is independently testable and ships a verifiable correctness improvement.

### Phase 1: VM Pool Foundation
**Rationale:** `LuaVmPool` and `ChunkCache` are pure new types with no changes to existing behavior. They must exist before any `ScriptContext` changes are made. Building and testing them in isolation eliminates uncertainty before the high-risk migration step.
**Delivers:** `LuaVmPool` (thread-local `Vec<Lua>` with checkout/checkin RAII), `PooledVm` (newtype with `Drop`-based return), `ChunkCache` (`Arc<Mutex<HashMap<u64, String>>>`), thread ID assertion in pool RAII guard, pool growth-on-demand policy.
**Addresses:** Foundation for all table-stakes features; pool size policy (Pitfall 9).
**Avoids:** Pitfall 8 (thread pool mixing) via thread ID assertion from day one.

### Phase 2: Host API Re-targeting
**Rationale:** Refactoring `install_host_api` to accept a `&Table` target is the highest-risk single change but is best done as a standalone step before `_ENV` isolation is wired in. At this stage, existing behavior is preserved by passing `lua.globals()` as the target — the refactor is mechanical and validates cleanly against existing tests.
**Delivers:** `install_host_api(lua, &Table)` signature; all per-component host API writes are target-parameterized; existing tests pass without modification.
**Addresses:** Host API re-injection correctness; sets up the correct write target for Phase 3.
**Avoids:** Pitfall 3 (host API closures bypassing `_ENV`) — the target switch happens here, not retrofitted later.

### Phase 3: _ENV Isolation + Lazy-Init
**Rationale:** The core migration: replace `lua: Lua` with `vm: Option<PooledVm>` + `env: Option<Table>`, add `ensure_initialized()`, redirect all globals reads/writes to `env_table`, and wire the pool checkout/checkin lifecycle. Gated on Phase 1 (pool exists) and Phase 2 (host API is retargetable). Highest-risk phase.
**Delivers:** Full per-component `_ENV` isolation; lazy-init defers VM allocation until first script call; `sync_state_from_lua()` reads from `env_table.pairs()`; `pool_baseline_globals` immutable snapshot; string metatable frozen via `Lua::sandbox(true)` on pool VMs; explicit env drop + registry key cleanup on VM checkin.
**Addresses:** Per-component state isolation, lazy-init, explicit reset on checkin.
**Avoids:** Pitfall 2 (string metatable contamination) via `Lua::sandbox(true)` at VM construction; Pitfall 4 (stale GC objects) via explicit cleanup on checkin; Pitfall 5 (lazy-init ordering) by keeping init synchronous; Pitfall 11 (`sync_state_from_lua` pollution) via `pool_baseline_globals` snapshot.

### Phase 4: Integration + Validation
**Rationale:** Wire the pool and cache into `FrontendSurfaceComponent`, apply lazy-init to `BackendScriptContext`, validate hot-reload cache invalidation, and run benchmark comparison. Intentionally thin on new logic — this is an integration and measurement phase.
**Delivers:** `ScriptContext::new_lazy(pool, cache, ...)` call site in `create_runtime_for_component`; `BackendScriptContext` defers `Lua::new()` to first poll/init; hot-reload cache eviction wired to source-path mtime notification; `cargo bench` showing per-component mount cost before vs. after.
**Addresses:** Chunk caching (hot-reload invalidation), backend lazy-init.
**Avoids:** Pitfall 6 (hot-reload cache staleness) by wiring eviction to the existing mtime watcher before integration is considered done.

### Phase Ordering Rationale

- Phases 1 and 2 carry no behavioral risk to the running shell; they can be reviewed and merged independently without destabilizing any existing surface.
- Phase 3 is the only phase where an implementation mistake produces silent correctness bugs (component state bleed) rather than compile errors; doing it after Phases 1 and 2 minimizes the change surface and ensures the new types are already tested.
- Phase 4 must be last because it exercises the full integration path; benchmarks here confirm the optimization reduces VM allocation cost and catch any regression the unit tests missed.
- Backend lazy-init belongs in Phase 4 rather than Phase 3 because it is a simpler isolated change and backend modules are long-lived singletons where the timing matters less.

### Research Flags

Phases needing careful implementation attention:
- **Phase 3:** `_ENV` isolation + lazy-init — highest-risk; add a dedicated integration test matrix (two components on one pool VM, sequential load/unload cycles, service payload isolation) before merging; if the `pool_baseline_globals` approach is unclear from ARCHITECTURE.md, consider a targeted research pass on `sync_state_from_lua` before implementation
- **Phase 2:** `install_host_api` refactor — medium risk; grep `globals()` across `context/runtime.rs` and all closure bodies before writing the patch to ensure no call site is missed

Phases with standard patterns (skip additional research):
- **Phase 1:** `LuaVmPool` + `ChunkCache` — standard `thread_local!` + `Arc<Mutex<HashMap>>` patterns; fully specified in STACK.md and ARCHITECTURE.md
- **Phase 4:** Integration wiring — one call-site change and one lazy-init guard; well-specified in ARCHITECTURE.md

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All key APIs verified against mlua 0.11 docs via Context7 and official docs; one caveat: `Compiler::compile()` cross-VM bytecode sharing is HIGH in STACK.md but LOW in ARCHITECTURE.md's direct source analysis — source-string fallback resolves safely for v1.17 |
| Features | HIGH | Feature set is infrastructure optimization with a clear correctness baseline; all table-stakes features are derived from first principles (isolation, lifecycle, host API continuity) rather than user surveys |
| Architecture | HIGH | Based on direct source reads of `crates/core/runtime/scripting/src/context/runtime.rs` and `crates/core/shell/src/shell/component/runtime.rs`; component boundaries, build order, and ownership model are grounded in actual code |
| Pitfalls | HIGH | All 11 pitfalls are grounded in codebase analysis (current `install_host_api` behavior, `send` feature in Cargo.toml) or official Luau documentation (string metatable, sandbox model); no speculative pitfalls |

**Overall confidence:** HIGH

### Gaps to Address

- **`Compiler::compile()` bytecode cross-VM loading**: STACK.md asserts this works via `set_mode(ChunkMode::Binary)`; ARCHITECTURE.md rates it LOW confidence. Validate at the start of Phase 1 with a minimal test: compile source on one `Lua` instance, load the bytes into a second. If it works, upgrade the chunk cache to store bytecode bytes instead of source strings. If it fails, the source-string cache is correct for v1.17 and no design change is needed.
- **Pool VM construction cost**: The assumption is that `Lua::new_with(BASE | STRING | TABLE | MATH, ...)` + `sandbox(true)` is cheap enough to run lazily on first pool expansion without blocking a render frame. Verify with a `cargo bench` measurement before finalizing the growth policy.
- **`BackendScriptContext` thread isolation**: Confirm in Phase 4 that `BackendScriptContext::new_*()` lazy-init does not reference the frontend `thread_local!` pool. Enforce by type — `BackendScriptContext` must have no `LuaVmPool` field.

## Sources

### Primary (HIGH confidence)
- `https://docs.rs/mlua/0.11.0/mlua/struct.Chunk.html` — `set_environment()`, `into_function()`, `set_mode()` signatures
- `https://docs.rs/mlua/0.11.0/mlua/struct.Lua.html` — `new_with()`, `StdLib`, `sandbox()`, `set_globals()` caveats
- `https://luau.org/sandbox/` — `__index`-based global table delegation, string metatable shared risk, read-only builtin strategy
- `crates/core/runtime/scripting/src/context/runtime.rs` (direct source read) — `ScriptContext` fields, `install_host_api`, `sync_state_from_lua`, `load_script_with_interface_imports`
- `crates/core/shell/src/shell/component/runtime.rs` (direct source read) — `EmbeddedFrontendRuntime`, `create_runtime_for_component`

### Secondary (MEDIUM confidence)
- `https://github.com/mlua-rs/mlua/discussions/494` — per-thread VM pattern; maintainer recommendation
- `https://github.com/mlua-rs/mlua/discussions/137` — bytecode `Vec<u8>` as cross-VM sharing unit; `ChunkMode::Binary` loading
- `https://sleitnick.github.io/luau-api/guides/sandboxing.html` — Luau C API `luaL_sandbox` / `luaL_sandboxthread` pattern
- `https://docs.rs/mlua/latest/mlua/struct.Thread.html` — Thread sandbox API
- `https://docs.rs/mlua/latest/mlua/struct.Compiler.html` — Compiler struct API

### Tertiary (LOW confidence)
- Architecture research assessment of `Compiler::compile()` bytecode extractability — contradicts STACK.md; resolve with a targeted test in Phase 1

---
*Research completed: 2026-06-02*
*Ready for roadmap: yes*
