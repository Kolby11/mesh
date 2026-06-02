# Feature Research

**Domain:** Lua VM pooling, _ENV-based isolation, lazy-init, and compiled chunk sharing for a shell UI runtime (MESH v1.17)
**Researched:** 2026-06-02
**Confidence:** HIGH (mlua API verified via Context7 + official docs; Luau sandboxing verified via luau.org official guide)

---

## Feature Landscape

### Table Stakes (Users Expect These)

These are behaviors the system must exhibit for the milestone to be considered correct. Missing any of these makes the optimization incomplete or unsafe.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Per-component state isolation | Components must not see each other's Lua globals; the existing authoring model (bare assignments = public state) depends on each component having its own `_ENV` namespace | MEDIUM | mlua `Thread::sandbox()` (Luau feature only) replaces global env table with a proxy that writes locally and reads through to parent VM globals. This is the correct mechanism — not a separate `Lua::new()` per component. |
| Pool checkout/checkin lifecycle | A component that checks out a VM slot must return it before another component can use it; use-after-checkin must be impossible | MEDIUM | Thread-local `RefCell<Vec<LuaThread>>` is the standard Rust pattern. Because Luau's `Lua` is `!Send` by default, thread-local is mandatory; a shared `Mutex<Vec<Lua>>` would require the `send` feature and adds lock contention on the render thread. |
| Host API re-injection on checkout | Per-component host closures (published_events Arc, tracked_service_fields Arc, module_id) must be installed into the checked-out thread environment before script execution | HIGH | The current `install_host_api` builds closures capturing `Arc<Mutex<...>>` clones. With pooling, the thread's sandbox `_ENV` is fresh on each checkout; host APIs must be re-injected into that env, not into the VM's shared parent globals. |
| Lazy-init triggers at first script call | A component that is declared but never mounted (e.g., a surface that hasn't been opened) must not allocate a VM slot. The slot is acquired when `load_script` or `call_handler` is first called. | LOW | This is purely a Rust ownership change: `ScriptContext` holds `Option<PooledThread>` instead of `Lua`. `None` means not yet initialized. The observable effect is reduced startup memory. |
| Compiled chunk sharing scope: per-module-source | Two instances of the same frontend module (e.g., two notification rows instantiated from the same `.mesh` file) compile the Luau source once and load the resulting bytecode into each thread slot | MEDIUM | `Compiler::compile()` (mlua Luau feature) takes `&[u8]` source and returns `Vec<u8>` bytecode. A `HashMap<Arc<str>, Arc<Vec<u8>>>` keyed on module source path or content hash holds the cache. Each pool checkout loads the cached bytecode via `lua.load(&bytecode).set_mode(ChunkMode::Binary)` into the sandboxed thread. |
| Deterministic cleanup on component unmount | When a component is unmounted, its pooled thread slot must be reset (via `Thread::reset()`) and returned to the pool, not leaked | LOW | `Thread::reset(function)` on Luau resets to newly-created state. The returned-to-pool thread must have its sandbox env replaced on next checkout, not carry state from the previous occupant. |
| Storage and side-channel continuity | `self.storage`, `published_events`, `tracked_service_fields`, and `shared_bound_instance_calls` must work identically with pooled threads as with the current per-component `Lua` instances | MEDIUM | These are Rust-side Arcs threaded through host API closures. They survive pool checkin/checkout as long as `ScriptContext` keeps ownership of the Arcs. Only the Luau-side host API table references must be rebuilt per checkout. |

### Differentiators (Competitive Advantage)

These behaviors go beyond correctness into observable performance improvement and maintainability improvement.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Pool size policy: bounded thread-local pool | Prevents unbounded memory growth when many surfaces are open simultaneously. A fixed-capacity pool (e.g., 8–16 slots per thread) combined with lazy-init means idle components hold no pool slot and active ones share a small pool. | MEDIUM | Pool size should be empirically tuned against the benchmark scenarios from v1.3. If all active surfaces need concurrent script calls (unlikely on a single render thread), pool can be sized to `max_concurrent_script_calls + 1` headroom. |
| Bytecode cache: process-wide, keyed on source hash | Two different frontend modules that happen to share a component definition (e.g., a shared helper `.mesh`) compile that source once for the whole process, not once per surface. | MEDIUM | Cache key should be a fast hash of source bytes (e.g., `xxhash` or `FxHashMap`) stored in a `OnceLock<Mutex<HashMap<u64, Arc<Vec<u8>>>>>`. Content-addressed avoids stale bytecode after hot-reload. |
| Sandbox once, inherit per-thread | Call `lua.sandbox(true)` once on the shared pool VM after stdlib load. Per-component isolation then costs only `Thread::sandbox()` per checkout — one Luau C API call — rather than re-initializing all stdlib tables per component. | LOW | This matches Roblox's published pattern: sandbox the state once, sandboxthread per script. Avoids per-component stdlib initialization cost entirely. |
| Observable pool metrics in debug inspector | Pool hit/miss ratio, thread reset count, chunk cache hit/miss, and lazy-init deferral count surfaced through the existing `mesh.debug` debug overlay. | LOW | Reuses the existing profiling infrastructure from v1.3. No new user-facing surface needed. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Shared `Mutex<Vec<Lua>>` cross-thread pool | Seems like it allows pools to be reused across Tokio worker threads | `mlua::Lua` is `!Send` by default. Enabling the `send` feature adds `Send` bounds to all Rust closures registered as Lua functions, which breaks the existing host API closures that capture non-Send types (Arcs are Send, but `mlua::Function` and `mlua::Table` are `!Send` by default). Migration cost is very high. | Thread-local pool. MESH's render loop is single-threaded; all script calls happen on the same thread. Thread-local is sufficient and zero-overhead. |
| Global-sharing between component _ENVs | Seems efficient to share parsed constants or computed tables | Breaks the public/private member model. If two components write to the same global name through their proxied `_ENV`, the proxy chain produces confusing behavior. Also breaks the `builtin_globals` snapshot used to filter reactive state. | Components that genuinely need shared data should use the backend service/interface layer or `self.storage`. |
| One `_ENV` table per module-type instead of per-instance | Seems like it reduces table allocations | Two instances of the same module would share state. Any global assignment from one instance (e.g., `icon_name = "audio-volume-high"`) would be visible to the other. Violates the core authoring contract. | Per-checkout thread sandbox. The table allocation cost is one `lua.create_table()` per script execution, which is trivial next to the per-VM stdlib init cost being eliminated. |
| Bytecode cache keyed on file path only | Simpler than content hashing | Stale bytecode after hot-reload. If a `.mesh` file changes on disk, the cache returns the old bytecode until the process restarts. Shell surfaces show outdated behavior. | Content hash key (fast non-cryptographic hash of source bytes). Invalidate on source change during hot-reload. |
| Pre-allocating a VM slot on `ScriptContext::new()` | Seems to avoid the first-call latency spike | Defeats lazy-init. Surfaces that haven't been shown yet allocate pool slots, reducing available capacity for visible surfaces. Under memory pressure this is worse than the current model, not better. | Acquire the slot on first `load_script()` call. The first render is already paid with source compilation; slot acquisition adds only thread sandbox setup. |
| Reusing thread slots across different module sources without reset | Avoids `Thread::reset()` cost | Bytecode loading into a non-reset thread appends to the existing execution context, potentially inheriting globals from the previous occupant. The sandbox proxy table accumulates old writes through its `__index` chain. | Always call `Thread::reset()` before reloading a different module's bytecode into a returned slot. Reset is documented as cheap in Luau — O(call-stack depth) cleanup. |

---

## Feature Dependencies

```
[VM Pool (thread-local LuaThread pool)]
    └──requires──> [Lua::sandbox() on shared VM at startup]
                       └──requires──> [mlua `luau` feature enabled]

[Per-component _ENV isolation (Thread::sandbox() per checkout)]
    └──requires──> [VM Pool]
    └──requires──> [Host API re-injection into sandboxed env]
                       └──requires──> [install_host_api refactored to target env table not globals()]

[Compiled chunk sharing (Compiler::compile() bytecode cache)]
    └──requires──> [mlua `luau` feature enabled]
    └──enhances──> [VM Pool] (pool checkout loads bytecode not source)

[Lazy-init (Option<PooledThread> in ScriptContext)]
    └──requires──> [VM Pool]
    └──enhances──> [startup memory] (no slot until first script call)

[Thread slot reset on unmount]
    └──requires──> [VM Pool]
    └──requires──> [Thread::reset() Luau API]

[Debug pool metrics]
    └──requires──> [VM Pool]
    └──enhances──> [existing v1.3 debug inspector]
```

### Dependency Notes

- **VM Pool requires `Lua::sandbox()`**: The pool pattern only pays off if the shared VM has read-only globals so per-thread sandboxing via `Thread::sandbox()` stays shallow (one-level `__index` proxy). Without VM-level sandbox, each thread sandbox creates a mutable proxy over a mutable parent, which is semantically wrong.
- **Host API re-injection requires refactoring `install_host_api`**: Currently `install_host_api` writes into `self.lua.globals()`. With pooling, it must write into the checked-out thread's local env table. The Arcs themselves do not change; only the target of the Lua table writes changes.
- **Chunk sharing enhances the pool**: Without chunk sharing the pool still works but each checkout re-compiles source. With chunk sharing, checkout is: reset thread, inject host API, load cached bytecode.
- **Lazy-init conflicts with pre-allocated pools**: Pool slots are borrowed on demand (lazy), not pre-warmed at startup. Pre-warming is an anti-feature for this use case (see Anti-Features above).

---

## MVP Definition

### Launch With (v1.17)

Minimum set that delivers the milestone goal: eliminate per-component `Lua::new()` cost.

- [ ] Thread-local VM pool with `Lua::sandbox()` on the shared instance at startup — the core mechanism; everything else builds on it
- [ ] `Thread::sandbox()` per checkout for `_ENV` isolation — correctness requirement; the public/private member model breaks without it
- [ ] Host API re-injection into the sandboxed thread env on every checkout — correctness requirement; scripts break without require, mesh.log, mesh.popover, etc.
- [ ] `ScriptContext` holds `Option<PooledThread>` for lazy-init — removes startup cost for hidden surfaces; straightforward Rust ownership change
- [ ] `Compiler::compile()` bytecode cache keyed on content hash — amortizes parse+compile cost across multi-instance modules

### Add After Validation (v1.x)

- [ ] Pool size tuning based on benchmark data from v1.3 scenarios — trigger: profiling shows pool exhaustion causing fallback allocations
- [ ] Debug inspector pool metrics (hit rate, reset count, cache hit rate) — trigger: need to validate pool behavior on real shell surfaces
- [ ] Thread slot reuse across component unmount/remount cycles — trigger: profiling shows `Thread::reset()` is measurable in hot paths

### Future Consideration (v2+)

- [ ] Cross-process bytecode caching (persist compiled bytecode to disk) — adds serialization complexity, only worthwhile if cold-start time becomes user-visible
- [ ] Pool per backend ScriptContext — backend contexts are long-lived (one per provider, not per component), so the per-VM cost is already amortized; pooling adds complexity for minimal gain

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Thread-local VM pool (replace Lua::new()) | HIGH — eliminates largest per-component startup cost | MEDIUM | P1 |
| Thread::sandbox() per checkout (_ENV isolation) | HIGH — correctness requirement | MEDIUM | P1 |
| Host API re-injection into sandboxed env | HIGH — correctness requirement | HIGH | P1 |
| Lazy-init (Option<PooledThread>) | MEDIUM — reduces startup memory for hidden surfaces | LOW | P1 |
| Compiler::compile() bytecode cache | MEDIUM — amortizes parse cost for multi-instance modules | MEDIUM | P1 |
| Thread::reset() on unmount for slot return | MEDIUM — prevents pool slot leak on surface close | LOW | P1 |
| Debug pool metrics | LOW — developer-facing only | LOW | P2 |
| Pool size tuning | LOW — correctness is fine with a generous default | LOW | P2 |

**Priority key:**
- P1: Must have for launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

---

## Reference Pattern Analysis

This is an infrastructure optimization milestone with no user-facing competitors. The relevant comparison is against prior art in embedded Lua runtimes:

| Pattern | Reference | Their Approach | MESH Approach |
|---------|-----------|----------------|---------------|
| Per-script isolation | Roblox / Luau official sandbox guide | `luaL_sandbox(state)` once, `luaL_sandboxthread(thread)` per script | Equivalent via mlua `lua.sandbox(true)` + `thread.sandbox()` |
| Chunk caching | Yazi (terminal file manager) | `Loader` caches compiled plugin chunks in `package.loaded`, prevents re-execution | Process-wide `HashMap<content_hash, Arc<Vec<u8>>>` keyed on source bytes |
| VM pool | Game engines (Unreal LuaMachine plugin) | LuaState loaded on-demand, not at registration time | `Option<PooledThread>` in ScriptContext, acquired on first `load_script` |
| Thread-local isolation | Standard Lua embedding practice | One `lua_State*` per OS thread | Thread-local `RefCell<Vec<Thread>>` — MESH render is single-threaded so one pool |
| Backend vs frontend separation | Yazi slim/standard env | Slim env for background tasks restricts async capabilities | MESH already has `BackendScriptContext` separate from `ScriptContext`; pool applies to both independently |

---

## Sources

- mlua Chunk API: https://docs.rs/mlua/latest/mlua/struct.Chunk.html
- mlua Thread API: https://docs.rs/mlua/latest/mlua/struct.Thread.html
- mlua Lua struct API: https://docs.rs/mlua/latest/mlua/struct.Lua.html
- mlua Compiler struct: https://docs.rs/mlua/latest/mlua/struct.Compiler.html
- Luau sandboxing official guide: https://luau.org/sandbox/
- Luau C API sandboxing guide: https://sleitnick.github.io/luau-api/guides/sandboxing.html
- Pre-compiling Lua discussion (mlua): https://github.com/mlua-rs/mlua/discussions/137
- Lua environments tutorial: http://lua-users.org/wiki/EnvironmentsTutorial
- Yazi plugin/Lua architecture: https://deepwiki.com/sxyazi/yazi/4.1-application-configuration

---
*Feature research for: MESH v1.17 Scripting VM Consolidation*
*Researched: 2026-06-02*
