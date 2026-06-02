# Pitfalls Research

**Domain:** Lua VM pooling, `_ENV` environment isolation, lazy VM init, compiled chunk caching in a Rust/mlua Luau shell runtime
**Researched:** 2026-06-02
**Confidence:** HIGH (mlua source + official Luau docs + codebase analysis)

---

## Critical Pitfalls

### Pitfall 1: Chunk-as-Function is VM-Bound — Cannot Be Shared Across Pool VMs

**What goes wrong:**
`lua.load(source).into_function()` compiles the Luau source and returns an `mlua::Function`. That `Function` is a GC object that lives inside the specific `Lua` instance it was compiled on. If you store these compiled functions in a cross-VM cache and load them into a different pool VM, you get either a runtime error or silent undefined behavior — the function references the original VM's GC heap.

**Why it happens:**
The natural optimization is "compile once, run everywhere." Developers cache the `Function` value keyed by source hash and hand it to whichever pool VM is available. This works for standard bytecode files (load from raw bytes), but not for `mlua::Function` handles, which are VM-internal reference-counted objects.

**How to avoid:**
Cache Luau **bytecode bytes** (via `Function::dump()` or the Luau `Compiler` struct), not `mlua::Function` handles. Each VM in the pool loads its own `Function` from the cached bytes via `lua.load(&bytecode).set_mode(ChunkMode::Binary).into_function()`. Key the cache on `(source_path, content_hash)`. Each VM then holds its own compiled function in its own registry if needed.

Note: Luau's bytecode format is version-stamped and changes with Luau VM upgrades. The cache must store the Luau version alongside the bytes and evict entries whose version does not match the running VM.

**Warning signs:**
- "attempt to call a nil value" or "invalid function" errors after pool checkout
- Works in tests (single VM) but fails in production (multi-VM pool)
- Errors appear non-deterministically depending on which pool VM is checked out

**Phase to address:**
Chunk cache implementation phase. Define cache key as `(source_content_hash, luau_version_string)`, store raw bytes, load fresh `Function` per VM.

---

### Pitfall 2: `_ENV` Isolation Leaves the String Metatable Shared Across All Environments

**What goes wrong:**
When multiple scripts share a single `Lua` VM with `_ENV`-based isolation (each script has its own globals table), the Lua string metatable is global to the VM — it lives on the string type itself, not on any environment table. A script that does `getmetatable("").__index.mymethod = function() ... end` injects into the string metatable visible to every other script in the pool VM. This is a contamination vector that `_ENV` alone does not block.

**Why it happens:**
`_ENV` replacement redirects global variable lookups to a per-script table, but it does not sandbox the string type's shared metatable. The Luau sandbox documentation explicitly warns that "the string metatable" is a cross-environment leak path.

**How to avoid:**
After assigning a per-component `_ENV`, also sandbox the string library proxy inside that environment:

```lua
-- inside environment setup, not global
local string_proxy = setmetatable({}, {__index = string})
env.string = string_proxy
```

Enable Luau's `Lua::sandbox()` mode if available, which freezes the builtin table hierarchy. For the `send`-enabled MESH build, verify that `lua.sandbox(true)` is called on pool VMs before any user code runs. This makes the builtin tables read-only at the VM level, preventing injection via `rawset` as well.

Additionally, mark the shared `_ENV` prototype (the table holding stdlib references) as read-only with `lua_setreadonly` (exposed via `mlua` in Luau mode) so scripts cannot mutate what they inherit.

**Warning signs:**
- String method behavior changes between components that share a VM
- A crash or wrong output only in components loaded after a specific module
- `getmetatable("")` returns unexpected fields

**Phase to address:**
VM pool + `_ENV` isolation phase. Include a test that proves one script cannot mutate the string metatable visible to another script on the same VM.

---

### Pitfall 3: Host API Closures Capture the Pre-Isolation Global State

**What goes wrong:**
The current `install_host_api()` creates `mlua::Function` closures (for `require`, `mesh.events.publish`, etc.) that capture `Arc` clones and are set as Lua globals. When moving to a pooled VM with `_ENV` isolation, these closures are re-installed per checkout — but if any closure internally calls `lua.globals()` to read or write state, it bypasses the per-script `_ENV` and writes directly to the VM's shared global table. This re-introduces cross-component contamination.

**Why it happens:**
The current code calls `self.lua.globals().set(...)` for every host API entry. In an isolated environment model, `globals()` is the shared VM table, not the component's `_ENV`. Closures that do `lua.globals().set("__mesh_request_redraw", true)` will set a flag visible to all scripts on that VM.

**How to avoid:**
In the pooled model, all per-component state (service payloads like `__mesh_svc_audio`, redraw flags, locale current) must live in the per-component `_ENV` table, not the VM's global table. Host API closures that need to write "component state" must receive the component's `_ENV` table as an argument or close over a `RegistryKey` pointing to the component's env table. Specifically:

- Replace `lua.globals().set("__mesh_request_redraw", ...)` with `env_table.set("__mesh_request_redraw", ...)`
- Replace `lua.globals().get::<Option<String>>("__mesh_locale_current")` with a read from the component's env
- The `require` closure must resolve interfaces against the component's capability set, not a shared global

**Warning signs:**
- `__mesh_request_redraw` set by one component causes another to repaint
- Service payloads from one backend appear in a different component's script
- Locale changes from one component affect another's `mesh.locale.current()`

**Phase to address:**
`_ENV` isolation phase. Every `lua.globals()` read/write in `install_host_api()` and all closure callbacks must be audited and ported to operate on the component's env table.

---

### Pitfall 4: Pool VM Checkout with Stale GC Objects Causes Subtle State Bleed

**What goes wrong:**
After returning a VM to the pool (checking it back in), the previous component's Lua tables, closures, and userdata still exist in the VM's heap until the GC collects them. If the next component checks out the VM before a GC cycle runs, callbacks registered by the previous component (particularly event channel callbacks stored in registry values or upvalues) can still fire during the next component's execution. The prior component's `__mesh_self_event_channels` table, if still alive in the GC graph, can cause events from the previous session to surface in the new one.

**Why it happens:**
Lua GC is incremental and does not run immediately on checkout. Registry values are not cleared by resetting `_ENV`. The checkout-reset step must explicitly clear all per-component entries, not rely on GC timing.

**How to avoid:**
On VM checkin (before returning to pool), perform an explicit reset sequence:
1. Remove all keys from the per-component `_ENV` table (or replace the table entirely).
2. Remove any registry keys registered for this component session (use `RegistryKey` handles tracked by the component, not string keys).
3. Call `lua.gc_collect()` (or equivalent) to run a full collection pass before the VM re-enters the pool. This is acceptable cost on checkin because checkin happens at component teardown, not at every frame.
4. Wipe `__mesh_self_event_channels`, `__mesh_svc_*`, and all `__mesh_*` sentinel globals.

**Warning signs:**
- Events from a previous component instance firing in a freshly initialized component
- `has_handler("render")` returns true on a checked-out VM that has never had a script loaded
- Storage write callbacks from a prior component triggering rerender on the new one

**Phase to address:**
VM pool checkin/checkout phase. Treat the reset sequence as a formal contract with a test that proves isolation between two sequential component loads on the same pool VM.

---

### Pitfall 5: Lazy-Init Creates a Window Where `call_render_lifecycle()` Executes Against an Uninitialized VM

**What goes wrong:**
The current `call_render_lifecycle()` checks `has_handler("render")` before calling. With lazy-init, an uninitialized VM has no script loaded, so `has_handler()` returns false and the render hook is silently skipped. This is correct behavior. The danger is at the *transition* point: a component becomes visible (triggering render) before `load_script()` completes (because lazy-init defers it until first use). If the lazy-init trigger and the render call happen in the same synchronous shell tick (which they do today — `render()` then `paint()`), there is no race; but if init is deferred to an async task or a background thread, the render call arrives before the VM is ready.

**Why it happens:**
The shell's event loop calls `call_render_hooks()` on every dirty component. If lazy-init is implemented as "init when first event arrives" rather than "init synchronously on first paint request", the render hook fires before the init completes.

**How to avoid:**
Keep lazy-init synchronous and blocking: init the VM on the first call that requires it (first render, first event handler, first service update), not in a background task. The init cost is paid once per component and is amortized across the component lifetime. Do not make the init async. Log a diagnostic warning if init is attempted from a context where it cannot block (e.g., from within a Lua callback). Provide a `is_initialized()` guard that all call sites check before delegating to the VM.

**Warning signs:**
- Components that never appear to respond to service updates until the second or third event
- Silent `HandlerNotFound` errors on first-ever activation of a component
- Debug inspector shows a component with no public state fields despite the script assigning them

**Phase to address:**
Lazy-init phase. The `EmbeddedFrontendRuntime` must track an explicit `Initialized` / `Uninitialized` state flag, and all call sites must check it before any VM dispatch.

---

### Pitfall 6: Hot-Reload Does Not Invalidate the Chunk Cache

**What goes wrong:**
The shell already watches source paths via `source_paths: Vec<(PathBuf, Option<SystemTime>)>` and triggers a recompile when mtime changes. If a compiled chunk cache (keyed on the source path) is not also notified of the mtime change, the old bytecode continues to run after the developer edits a `.mesh` file. The edit appears to have no effect. The developer has no diagnostic indicating a stale cache is being used.

**Why it happens:**
The chunk cache is a performance optimization that lives outside the normal `compile_frontend_module()` path. Hot-reload clears the compiled module tree but does not know about the bytecode cache unless the cache invalidation is explicitly wired into the reload path.

**How to avoid:**
Key the bytecode cache on `(source_path, content_hash)` rather than just `source_path`. When the shell detects a source change and calls `compile_frontend_module()`, the new compilation produces a new content hash. The old cache entry remains but is never requested again (the new hash is used). Add a cache eviction pass (e.g., LRU with a small capacity, or evict on any reload for affected paths) to prevent unbounded growth. Log a debug trace when a new chunk replaces an old cache entry for a path, to make the invalidation observable.

**Warning signs:**
- Source edits during development appear to have no effect until shell restart
- Debug inspector shows script state that does not match the current `.mesh` source
- Different components with the same source path pick up each other's stale scripts

**Phase to address:**
Chunk cache phase. Define the cache key structure before building the cache, and wire cache invalidation to the same path-mtime notification already used by the hot-reload watcher.

---

### Pitfall 7: Mutable Upvalues in Cached Chunks Cause Cross-Component State Bleed

**What goes wrong:**
Luau closures capture upvalues by reference when the upvalue is mutable. If a cached compiled function is loaded into a new component's VM session but the function's upvalue slots still reference state from a previous load (this is only possible if the same `Function` handle is reused across sessions, not if bytes are re-loaded), those upvalue cells can carry state between component instances. This is a variant of Pitfall 1, but specifically about upvalue cells rather than the function pointer itself.

**Why it happens:**
If the bytecode-to-function compilation step is skipped and an existing `Function` handle is reused (which the correct approach forbids), the function's upvalue slots retain whatever was written to them during the prior execution. Top-level Luau scripts typically have `_ENV` as their sole upvalue; if `_ENV` is properly swapped per load, mutable top-level upvalues are not an issue. But nested closures (event handlers, callbacks) that capture module-level mutable locals can carry stale state if the function handle is reused.

**How to avoid:**
Enforce the rule from Pitfall 1: always compile from bytes, never reuse `Function` handles across component sessions. This eliminates upvalue reuse. Document this constraint in the cache API: the cache stores `Vec<u8>` (bytes), not `Function` handles.

**Warning signs:**
- A module-level counter increments across component reload (showing state survived reload)
- Event handler closures reference the previous component's `module_id` string

**Phase to address:**
Chunk cache phase. The cache type signature should make it impossible to store `Function` values — it must be `HashMap<CacheKey, Vec<u8>>`.

---

### Pitfall 8: `send` Feature + Per-Thread VM Pool Creates Implicit Threading Requirement

**What goes wrong:**
The `send` feature is already enabled in `mesh-core-scripting` Cargo.toml (`mlua = { features = ["luau", "serialize", "send"] }`). This makes `Lua: Send + Sync`. A per-thread VM pool implemented with `thread_local!` would store the `Lua` in a non-`Send` container on the thread. If a future refactor moves shell rendering to a Tokio thread pool (currently it is single-threaded), a VM checked out on thread A and stored in a `Rc<RefCell<...>>` local would panic or fail to compile when accessed from thread B.

More immediately: the pool must be implemented as `thread_local!{ static POOL: RefCell<Vec<Lua>> }` — one pool per OS thread. If the shell's render loop is on a single dedicated thread (which is true today), this is fine. If any async task or background thread ever tries to call into a pooled VM, it will either fail to compile (because `RefCell` is not `Send`) or silently access the wrong thread's pool.

**Why it happens:**
The `send` feature removes the `!Send` marker from `Lua` but the actual VM is not safe to call from multiple threads concurrently. The reentrant mutex inside mlua serializes calls on a single `Lua` instance but still does not make it safe to check out on thread A and execute on thread B without synchronization.

**How to avoid:**
Use `thread_local!` for the pool. Make pool checkout return a RAII guard that panics on drop if it is dropped on a different thread than it was checked out on (asserting same `thread::current().id()`). Document that pool VMs must only be used on the shell render thread. Keep backend scripting (`BackendScriptContext`) using its own dedicated `Lua` instances, not the pool — backends run on Tokio task threads and mixing them with the frontend pool would be an immediate bug.

**Warning signs:**
- Intermittent crashes or UB in Luau execution during async-heavy load
- Pool checkout functions called from Tokio async contexts
- Tests pass with `#[tokio::test]` single-threaded but fail with multi-thread executor

**Phase to address:**
VM pool implementation phase. Annotate pool checkout/checkin with `#[track_caller]` and thread ID assertions from day one.

---

### Pitfall 9: Pool Exhaustion During Bulk Component Init at Shell Startup

**What goes wrong:**
At shell startup, all visible frontend components initialize simultaneously. If the pool has fewer VMs than the number of components, and initialization holds a checked-out VM while waiting for something (e.g., an interface catalog resolution that blocks), the pool empties and all remaining components block waiting for a VM that is never returned. This is a deadlock.

**Why it happens:**
Component initialization is currently synchronous and non-blocking, so this risk is low today. The risk grows if lazy-init is combined with any form of async init or if the pool is used for both initialization and per-frame render hooks (holding a VM across a frame boundary).

**How to avoid:**
Size the pool to at least `max(num_visible_components, 4)` on startup. Never hold a checked-out VM across a yield point (async `.await`, blocking I/O). VM checkout must be "take one, execute, return" within a single synchronous call — never keep a VM checked out across multiple shell ticks. If a component needs to do async work, it should do so outside the VM, collect the result, and then check out a VM only for the pure Lua execution step. Consider an upper bound (e.g., 16 VMs) and a growth-on-demand policy rather than a fixed size.

**Warning signs:**
- Shell hangs at startup when more than N components load
- `pool.checkout()` call never returns in profiling snapshots
- Frontend surfaces never appear after adding more modules

**Phase to address:**
VM pool sizing and lifetime policy phase. Add a pool utilization metric to the debug inspector.

---

### Pitfall 10: Luau Bytecode Version Mismatch After mlua Upgrade

**What goes wrong:**
Luau bytecode is not a stable cross-version format. If the project upgrades `mlua` (which vendors a specific Luau release), any cached bytecode compiled against the old Luau version will fail to load against the new runtime. This manifests as a load error ("invalid bytecode" or "unsupported bytecode version") at startup or during hot-reload, often without a clear message pointing to the cache as the source.

**Why it happens:**
Persistent bytecode caches (stored on disk or in a database between shell runs) do not automatically know which Luau version produced them. In-memory caches (valid only for the current shell session) are not affected, but any cache that survives a shell restart (including one backed by a file cache under `~/.cache/mesh/`) will have stale entries.

**How to avoid:**
For in-memory caches (the v1.17 target): version mismatch is not a problem because the cache is built fresh each shell run.

If disk caching is added in a future milestone, embed the Luau bytecode version number (readable from the bytecode header) in the cache key or the cache file name. On cache miss or version mismatch, re-compile from source.

For v1.17 specifically: use an in-memory-only cache and explicitly document that disk persistence of compiled chunks is out of scope.

**Warning signs:**
- Startup errors that disappear after clearing a cache directory
- Components fail to load only after a Cargo dependency update
- Version numbers in error messages referencing a different mlua release

**Phase to address:**
Chunk cache phase. If disk caching is never planned, add a comment in the cache struct explicitly ruling it out to prevent future contributors from accidentally adding it.

---

### Pitfall 11: `sync_state_from_lua()` Full Globals Scan Interacts Badly With Pool VM Globals

**What goes wrong:**
`sync_state_from_lua()` performs a full scan of `lua.globals()` on the first call after `load_script()` to discover user-defined keys. In a pool VM, the globals table contains stdlib entries plus host API entries plus any leftovers from previous component sessions (if cleanup was incomplete). The `builtin_globals` snapshot taken just before `lua.load(source).exec()` must accurately reflect everything installed by host API setup; any keys injected during VM pool setup that are not present at `install_host_api()` time will be misclassified as user-defined reactive state.

**Why it happens:**
The current code takes a `builtin_globals` snapshot after `install_host_api()` and before `lua.load(source).exec()`. In the pool model, if the VM enters the pool with pre-installed stdlib entries that were not there during `install_host_api()`, those entries will not be in the snapshot and will be treated as user state during the full scan.

**How to avoid:**
Capture the `builtin_globals` snapshot immediately after the VM is initialized (VM creation + stdlib install + pool setup), before any per-component host API installation. Store this as an immutable `pool_baseline_globals` in the pool entry. On checkout, initialize `builtin_globals` from `pool_baseline_globals` and then extend it with the keys added by `install_host_api()` for this component. This ensures the scan correctly excludes everything that was pre-loaded.

**Warning signs:**
- Debug inspector shows unexpected reactive state variables like `"string"`, `"math"`, or `"require"` appearing as component public fields
- Template binding expressions `{string}` or `{math}` accidentally resolve to the stdlib table

**Phase to address:**
`_ENV` isolation + globals scan phase. The `pool_baseline_globals` snapshot must be computed once at pool VM creation and must be immutable thereafter.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Cache `Function` handles instead of bytecode bytes | Slightly faster checkout (no re-parse from bytes) | Cross-VM contamination, forces single-VM "pool" | Never — always cache bytes |
| One global pool (not per-thread) with `Mutex<Vec<Lua>>` | Simpler code, shares VMs across threads | Contention on lock; VM execute-from-wrong-thread risk | Never — use `thread_local!` |
| Skip VM reset on checkin (rely on GC) | Faster checkin | Event channel and state bleed between components | Never — always reset explicitly |
| Eager-init (allocate all VMs at shell startup) | No lazy-init complexity | Wastes memory for hidden surfaces that never show | Acceptable for initial v1.17 if memory budget allows |
| In-memory chunk cache without disk persistence | No cache invalidation problem across upgrades | Cache is rebuilt every shell run (cold start cost) | Acceptable for v1.17 — disk caching is future work |
| Share stdlib tables across `_ENV` environments without read-only freeze | Simpler env setup | String metatable and stdlib mutation contaminates all envs | Never — freeze stdlib before exposing to scripts |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Retained rendering + lazy-init | Skipping render hook calls for uninit components silently | Track `is_initialized` state; distinguish "uninit" (skip and trigger init) from "has no render handler" (skip permanently) |
| Hot-reload + chunk cache | Clearing the compiled module tree without clearing the bytecode cache | Wire cache eviction to the same path-change notification as `source_paths` mtime tracking |
| Per-frame `call_render_lifecycle()` + pool checkout | Holding a pool VM across the full render + paint cycle | Return the VM to the pool immediately after Lua execution; paint uses the `ScriptState` snapshot, not the live VM |
| Backend scripting + frontend pool | Using the pool for backend VMs | Backend VMs live on separate Tokio task threads and must not share the frontend thread-local pool |
| `self.storage` closures + `_ENV` isolation | Storage callbacks write to the old VM-global `__mesh_*` keys | Storage dirty flags and tracking state must live in per-component `_ENV`, not VM globals |
| `sync_state_from_lua()` + shared VM | Globals scan picks up keys from previous component | Snapshot `pool_baseline_globals` at pool VM creation, extend per checkout |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| `lua.gc_collect()` on every checkin | Checkin takes 5-50ms blocking the render thread | Run full GC only on low-activity idle ticks; use incremental GC step on normal checkin | Every component unmount in heavy UI |
| Bytecode cache with no eviction | In-memory cache grows unboundedly during long shell sessions with many unique modules | Cap cache to `num_loaded_modules * 2`; LRU eviction | Sessions loading many dynamic modules |
| Re-installing full host API on every checkout | Checkout cost approaches original `Lua::new()` cost | Install stdlib-level APIs once at pool VM creation; install per-component APIs (capabilities, module_id closures) on checkout | High-frequency surface open/close |
| `Function::dump()` called on every cache miss | Repeated full compilation of source text | `dump()` once per source hash, cache the bytes | Hot-reload loop with rapid edits |
| `pairs()` globals walk on every `sync_state_from_lua()` | O(n_globals) per frame | Fast path (known user keys list) already exists; ensure it activates immediately after first load, not just after a second call | More than ~10 reactive globals per component |

---

## "Looks Done But Isn't" Checklist

- [ ] **VM isolation:** `_ENV` is set per component, but verify the string metatable is frozen and stdlib tables are read-only before any script executes
- [ ] **Pool reset:** VM checkin clears `_ENV` contents, but verify registry keys registered for event channels are also explicitly removed
- [ ] **Lazy-init guard:** `call_render_lifecycle()` silently skips uninitialized components, but verify it also schedules the init to run before the next render rather than waiting for the next event
- [ ] **Chunk cache:** bytecode bytes are cached, but verify the cache key includes a Luau version identifier to survive a future `mlua` upgrade
- [ ] **Hot-reload invalidation:** source mtime change triggers recompile, but verify the bytecode cache entry for that path is evicted before the recompile runs (not after)
- [ ] **Thread assertion:** pool checkout returns a VM, but verify checkout panics if called from any thread other than the shell render thread
- [ ] **Backend isolation:** `BackendScriptContext` does not use the pool, but verify no code path accidentally calls `pool.checkout()` from a Tokio async context
- [ ] **`builtin_globals` snapshot:** snapshot is taken before user script runs, but verify it includes pool-baseline keys (stdlib entries installed at VM creation time, before checkout)

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Cross-VM `Function` handle sharing | HIGH | Audit all `Function` values stored outside a `Lua` instance; replace with `Vec<u8>` bytecode; add a compile-time newtype that prevents `Function` from being stored in the cache struct |
| String metatable contamination | HIGH | Add per-environment string proxy; enable Luau sandbox mode; write a test that mutates string metatable in one component and proves the change is not visible in another |
| Host API closure writing to VM globals | MEDIUM | Audit every `lua.globals().set(...)` in `install_host_api()`; port each to env-table writes; add integration test proving redraw flag isolation |
| Stale chunk cache after hot-reload | LOW | Add content hash to cache key; the old entry becomes unreachable, not wrong |
| Pool deadlock at startup | MEDIUM | Add pool exhaustion diagnostic with pool size and waiters count; increase pool floor to `num_modules + 4` |
| `builtin_globals` scan pollution | LOW | Add a test that loads two components sequentially on one pool VM and verifies the second component's reactive state does not include stdlib names |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Chunk-as-Function is VM-bound | Chunk cache implementation | Test: compile bytes on VM-A, load on VM-B, assert no error and correct output |
| String metatable shared across envs | `_ENV` isolation phase | Test: mutate string metatable in component-1, assert component-2 on same VM is unaffected |
| Host API closures bypass `_ENV` | `_ENV` isolation phase | Test: two components on same VM, assert `__mesh_request_redraw` is independent |
| Stale GC objects on VM checkin | Pool checkin/checkout phase | Test: load component-1, check in, load component-2, assert no handlers/state from component-1 visible |
| Lazy-init and render hook ordering | Lazy-init phase | Test: call `call_render_lifecycle()` on uninit component, assert init is triggered before next render |
| Hot-reload does not invalidate cache | Chunk cache + hot-reload integration phase | Test: load chunk, mutate source, reload, assert new behavior not old |
| Mutable upvalue reuse | Chunk cache phase | Enforce by type: cache stores `Vec<u8>` not `Function` |
| `send` + thread pool mixing | VM pool design phase | Assert with `thread_id()` check in pool RAII guard |
| Pool exhaustion | VM pool sizing phase | Test: init N+1 components with pool size N, assert all complete without deadlock |
| Luau bytecode version mismatch | Chunk cache phase | Document in-memory-only scope; add eviction test |
| `sync_state_from_lua()` globals pollution | `_ENV` isolation + globals scan phase | Test: verify no stdlib key appears in reactive state after component load on pool VM |

---

## Sources

- mlua 0.11 Cargo.toml at `crates/core/runtime/scripting/Cargo.toml` — `send` feature confirmed enabled
- `crates/core/runtime/scripting/src/context/runtime.rs` — `ScriptContext` implementation, `install_host_api`, `sync_state_from_lua`, `load_script_with_interface_imports`
- `crates/core/shell/src/shell/component.rs` — `EmbeddedFrontendRuntime`, per-component `HashMap<String, ScriptContext>` storage
- `crates/core/shell/src/shell/types.rs` — `source_paths` mtime hot-reload tracking
- [mlua `send` feature and `!Send` rationale — Discussion #398](https://github.com/mlua-rs/mlua/discussions/398)
- [mlua pre-compiling chunks — Discussion #137](https://github.com/mlua-rs/mlua/discussions/137)
- [Luau sandbox model and string metatable shared risk](https://luau.org/sandbox/)
- [mlua `Chunk::set_environment` docs — `_ENV` upvalue behavior](https://docs.rs/mlua/latest/mlua/struct.Chunk.html)
- [Luau bytecode versioning — `Bytecode.h`](https://github.com/luau-lang/luau/blob/master/Common/include/Luau/Bytecode.h)
- [mlua concurrent processing discussion — Discussion #494](https://github.com/mlua-rs/mlua/discussions/494)
- Luau performance notes on upvalue immutability and closure caching: https://luau.org/performance/

---
*Pitfalls research for: mlua Luau VM pooling, `_ENV` isolation, lazy-init, chunk caching in MESH shell runtime*
*Researched: 2026-06-02*
