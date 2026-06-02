# Architecture Research: v1.17 VM Pool + _ENV Isolation + Chunk Caching

**Domain:** Luau scripting runtime consolidation — per-thread VM pool, _ENV isolation, lazy-init, shared compiled chunks
**Researched:** 2026-06-02
**Confidence:** HIGH (based on direct source reading of crates/core/runtime/scripting and crates/core/shell)

---

## System Overview

### Current State (before v1.17)

```
Shell thread
  FrontendSurfaceComponent
    runtimes: Arc<Mutex<HashMap<instance_key, EmbeddedFrontendRuntime>>>
      EmbeddedFrontendRuntime { module_id, script_ctx: ScriptContext }
        ScriptContext { lua: Lua::new(), ... }   <-- one full VM per component instance

  BackendScriptContext { lua: Lua::new(), ... }  <-- one full VM per backend module
```

Each `ScriptContext::new()` and `BackendScriptContext::new_*()` call allocates a new `mlua::Lua`
VM — full Luau stdlib setup, internal allocator state, and metatable construction — regardless
of whether the component's script block will ever execute. For a shell with a navigation bar,
audio popover, and several embedded component instances, this means 5–10 or more full VMs
allocated on startup.

### Target State (after v1.17)

```
Shell thread owns one LuaVmPool (thread-local or shell-scoped)
  Pool: Vec<Lua> (idle VMs checked back in)

  FrontendSurfaceComponent
    runtimes: Arc<Mutex<HashMap<instance_key, EmbeddedFrontendRuntime>>>
      EmbeddedFrontendRuntime { module_id, script_ctx: ScriptContext }
        ScriptContext {
          vm: Option<PooledVm>,    <-- lazy: None until first script call
          env: Option<Table>,      <-- per-component _ENV table (isolated globals)
          ...
        }

  ChunkCache (shell-scoped or pool-scoped)
    source_hash -> compiled bytecode (Vec<u8>)
```

---

## Component Boundaries

### New: `LuaVmPool` (in `mesh-core-scripting`)

| Aspect | Detail |
|--------|--------|
| Location | `crates/core/runtime/scripting/src/pool.rs` (new file) |
| Owner | Shell thread — one pool instance per thread that runs component lifecycle |
| API | `checkout() -> PooledVm`, `checkin(PooledVm)` (via `Drop`) |
| Pool size | Configurable, default ~4 VMs. Grows on demand if all checked out |
| Thread model | NOT shared across threads. `Lua` with `send` feature is `Send+Sync`, but a pool that lends owned `Lua` instances does not need `Arc<Mutex>` when it lives on one thread |

The pool holds fully initialized VMs (stdlib installed, `mesh.*` host API installed) ready for
checkout. This amortizes the `Lua::new()` + host API setup cost across all component mounts.

**Confidence:** MEDIUM. mlua's own recommendation for true parallelism is "one VM per thread."
A pool on one thread avoids contention entirely. The `send` feature (already enabled in
`mesh-core-scripting/Cargo.toml`) means `Lua` is `Send`, so a pool that sends VMs across
threads in the future remains possible, but v1.17 targets single-thread shell execution.

### Modified: `ScriptContext` (in `mesh-core-scripting`)

Current `ScriptContext` fields that change:

| Field | Before | After |
|-------|--------|-------|
| `lua: Lua` | Owned, always present | Replaced by `vm: Option<PooledVm>` |
| (new) `env: Option<Table>` | — | Per-component isolated _ENV table, created on checkout |
| (new) `source_hash: Option<u64>` | — | FNV64 of the component's script source |
| (new) `initialized: bool` | — | Whether the VM checkout + host API + script load have run |

`PooledVm` is a newtype wrapping `Lua` that implements `Drop` to return the VM to the pool.

**Lazy-init:** `initialized` starts `false`. The VM checkout + `install_host_api` + `load_script`
sequence runs only on the first call that requires the Lua VM. Candidates for triggering init:
`load_script`, `call_init`, `call_handler`, `call_render_lifecycle`, `has_handler`,
`apply_service_payload`, and `set_global_state`. All these must check `self.initialized` and
call a new `self.ensure_initialized()` helper before touching `self.vm`.

### Modified: `BackendScriptContext` (in `mesh-core-scripting`)

`BackendScriptContext` has the same `lua: Lua` field and the same construction cost. Backend
modules are longer-lived (one per backend provider, not per render frame), so pooling matters
less. However, lazy-init still applies: defer `Lua::new()` until the backend's first poll or
`init()` call. The `pool` is not shared between frontend and backend — backend modules can
either use the same pool or hold their own `Lua` directly (simpler, lower priority).

**Recommended for v1.17:** apply lazy-init to `BackendScriptContext` but do not pool backend VMs.
Backend pool is future work.

### New: `ChunkCache` (in `mesh-core-scripting`)

| Aspect | Detail |
|--------|--------|
| Location | `crates/core/runtime/scripting/src/chunk_cache.rs` (new file) |
| Key | `u64` FNV hash of the script source bytes |
| Value | `Vec<u8>` — Luau bytecode from `lua.load(source).set_name(id).into_function()` then serialized |
| Scope | Shell-wide singleton (wrapped in `Arc<Mutex<>>` or passed into `ScriptContext` at creation) |

**Critical constraint:** Luau removes `string.dump` and restricts raw bytecode for security. The
approach supported by Luau is: compile source once with `luau_compile` (bytecode bytes), then
load those bytes via `luau_load` in each VM where the chunk is needed. In mlua terms:

```rust
// Compile once (on first ScriptContext::load_script for this source):
let func: Function = lua.load(source)
    .set_name(module_id)
    .into_function()?;
// Luau's internal bytecode is NOT directly extractable via string.dump in mlua.
// The chunk cache must store source strings or use luau_compile directly via unsafe.
```

Because mlua's safe API does not expose `luau_compile` or raw bytecode extraction, the
practical v1.17 chunk cache stores the **source string itself** (keyed by hash) rather than
compiled bytecode. Each pool VM re-parses the source on first load, but this is cheap compared
to `Lua::new()` overhead. A future phase can use `unsafe` C FFI to access `luau_compile`/
`luau_load` for true bytecode caching, but that is out of scope for v1.17.

**Confidence for source-cache approach:** HIGH.
**Confidence for bytecode-reuse via mlua safe API:** LOW — mlua does not expose raw bytecode bytes.

---

## _ENV Isolation Pattern

### How it works in Luau with mlua

Luau's `Thread::sandbox()` (mlua 0.11) replaces a coroutine's global environment table with
a proxy that writes locally and reads from the parent `_G`. This is the `luaL_sandboxthread`
C call exposed at the Rust level.

For `ScriptContext`, the isolation model is:

1. On VM checkout, call `lua.sandbox(true)` once on the VM itself (marks stdlib read-only).
2. For each component instance, create a fresh `Table` as the component's private globals.
3. Load the script with `lua.load(source).set_environment(env_table).exec()`.
4. All global reads/writes by that script go through `env_table`, not `_G`.

The `set_environment` method on `Chunk` (confirmed in mlua docs) sets the `_ENV` upvalue,
isolating global reads and writes to that table. The pool VM's `lua.globals()` (the base `_G`)
remains clean; each component's `env_table` is a separate `Table` stored in `ScriptContext`.

### Lifecycle of the `env` table

| Event | Action |
|-------|--------|
| Component mount, first script call | Checkout VM, create `env_table = lua.create_table()`, install host API into `env_table` instead of `lua.globals()` |
| Each handler call | `lua.load(handler_call).set_environment(env_table).call(...)` — or call functions already stored in `env_table` |
| Component unmount | Drop `env_table` (Lua GC collects it), check VM back into pool |

### Impact on `install_host_api`

Currently `install_host_api` writes to `self.lua.globals()`. After the change it must write to
`self.env` (the component's isolated table). The `require`, `self`, `module`, `mesh` globals
all go into `env_table` rather than `lua.globals()`. This keeps the pool VM's base globals
clean for reuse.

**Key invariant:** The pool VM's `_G` must never be mutated by component code. All component
state lives in `env_table`. When a VM is checked back in, `env_table` is dropped and the VM's
`_G` is clean for the next checkout.

---

## Data Flow Changes

### Before v1.17: Component Mount

```
FrontendSurfaceComponent::create_runtime_for_component()
  -> ScriptContext::new()           (allocates Lua::new() IMMEDIATELY)
  -> script_ctx.install_host_api()  (writes to lua.globals())
  -> script_ctx.load_script()       (lua.load(source).exec())
  -> script_ctx.call_init()
```

### After v1.17: Component Mount

```
FrontendSurfaceComponent::create_runtime_for_component()
  -> ScriptContext::new_lazy(pool_ref, chunk_cache_ref, ...)
     (does NOT call Lua::new() — stores pool_ref and sets initialized=false)

First actual script execution (e.g. call_render_lifecycle or call_init):
  -> ScriptContext::ensure_initialized()
     -> pool.checkout() -> PooledVm        (reuses existing Lua if available)
     -> lua.sandbox(true)                  (once per VM checkout — idempotent)
     -> env_table = lua.create_table()
     -> install_host_api_into(&env_table)  (modified: writes to env_table, not globals)
     -> chunk_cache.get_or_compile(source_hash, source)
        (returns source String or bytecode Vec<u8>)
     -> lua.load(source).set_environment(env_table).set_name(id).exec()
     -> call_init()
     -> initialized = true
```

### Component Unmount

```
FrontendSurfaceComponent drops EmbeddedFrontendRuntime
  -> ScriptContext::drop()
     -> flush_storage()
     -> drop(env_table) — Lua GC will collect
     -> drop(vm) triggers PooledVm::drop()
        -> pool.checkin(lua)  (returns VM to pool Vec)
```

---

## New vs Modified Components

### New components

| Component | Location | What it does |
|-----------|----------|--------------|
| `LuaVmPool` | `crates/core/runtime/scripting/src/pool.rs` | Holds `Vec<Lua>`, provides `checkout()`/`checkin()`. Not `Arc`-wrapped — lives on one thread. |
| `PooledVm` | same file | Newtype `struct PooledVm(Option<Lua>)` + `Drop` impl to return VM to pool. |
| `ChunkCache` | `crates/core/runtime/scripting/src/chunk_cache.rs` | `HashMap<u64, String>` (source by hash). `Arc<Mutex<ChunkCache>>` passed at context creation. |

### Modified components

| Component | Location | What changes |
|-----------|----------|--------------|
| `ScriptContext` | `crates/core/runtime/scripting/src/context/runtime.rs` | Replace `lua: Lua` with `vm: Option<PooledVm>` + `env: Option<Table>` + `initialized: bool`. Add `ensure_initialized()`. Change `install_host_api` to take `&Table`. |
| `ScriptContext::new` | same | Becomes `new_lazy(pool, cache, ...)`. No `Lua::new()` call. |
| `install_host_api` | same | Writes `require`, `self`, `module`, `mesh` into the passed `env_table` not `lua.globals()`. |
| `load_script_with_interface_imports` | same | Calls `ensure_initialized()` then `lua.load(src).set_environment(env).exec()`. |
| `apply_service_payload` | same | Calls `ensure_initialized()` before `lua.globals().set(...)` — but writes to `env` not globals. |
| `sync_state_from_lua` | same | Reads from `env_table.pairs()` rather than `lua.globals().pairs()` for user globals discovery. |
| `BackendScriptContext::new_*` | `crates/core/runtime/scripting/src/backend/runtime.rs` | Defer `Lua::new()` to first `init()` or poll call. No pool needed for backends. |
| `Shell` (or `FrontendSurfaceComponent`) | `crates/core/shell` | Construct `LuaVmPool` and `Arc<Mutex<ChunkCache>>` once; pass refs into each `ScriptContext::new_lazy()`. |

---

## Ownership Model

```
Shell (single thread)
  LuaVmPool  (owned directly by shell or by FrontendSurfaceComponent, not Arc-wrapped)
    Vec<Lua>  -- idle VMs

  Arc<Mutex<ChunkCache>>  -- shared between all ScriptContext instances
    HashMap<u64, String>  -- source_hash -> source (or bytecode if C FFI path taken later)

  FrontendSurfaceComponent
    runtimes: Arc<Mutex<HashMap<instance_key, EmbeddedFrontendRuntime>>>
      EmbeddedFrontendRuntime
        script_ctx: ScriptContext
          vm: Option<PooledVm>    -- None until first use
          env: Option<Table>      -- None until checkout
          pool: &'shell LuaVmPool -- borrow or pointer back to pool
          chunk_cache: Arc<Mutex<ChunkCache>>
```

**Lifetime consideration:** `LuaVmPool` must outlive all `ScriptContext` instances. The simplest
model is: shell owns the pool; `ScriptContext` holds `Arc<Mutex<LuaVmPool>>`. The shell creates
the pool during `Shell::run()` before any module mount. Alternatively, store the pool as a
thread-local `RefCell<LuaVmPool>` and access via `thread_local!`, avoiding `Arc` overhead
entirely (since MESH shell runs on one thread).

**Recommended:** `thread_local! { static VM_POOL: RefCell<LuaVmPool> = ... }` in `mesh-core-scripting`.
This removes the need to thread pool ownership through constructor args, which is a significant
simplification given how many constructors `ScriptContext` has.

---

## Build Order

The following order respects existing crate dependencies (lower = must ship first):

1. **`LuaVmPool` + `PooledVm`** — new types in `mesh-core-scripting`, no external deps, pure Rust.
   Test: pool checkout/checkin cycles, grow-on-demand, VM state cleanliness.

2. **`ChunkCache`** — new type in `mesh-core-scripting`, depends only on `std`. Source-string
   cache first, bytecode path deferred.
   Test: cache hit/miss, concurrent access via `Arc<Mutex<>>`.

3. **`install_host_api` refactor** — modify to accept `&Table` target instead of writing to
   `lua.globals()`. This is purely internal to `ScriptContext`. Keep existing behavior by passing
   `lua.globals()` initially, then switch to `env_table` after step 4.
   Test: existing `ScriptContext` tests still pass.

4. **_ENV isolation + lazy-init** — replace `lua: Lua` with `vm: Option<PooledVm>` + `env:
   Option<Table>`, add `ensure_initialized()`, update `load_script_with_interface_imports` and
   all entry-point methods. This is the highest-risk step.
   Test: component script isolation (one component cannot see another's globals), lazy-init
   defers VM allocation until first actual call, unmount returns VM to pool.

5. **`BackendScriptContext` lazy-init** — smaller change, no pool needed.
   Test: backend init() still runs, poll still executes.

6. **Wire pool/cache into `FrontendSurfaceComponent`** — update `create_runtime_for_component`
   to call `ScriptContext::new_lazy(pool, cache, ...)` instead of `ScriptContext::new(...)`.
   Test: full shell startup/render cycle with navigation bar + audio popover.

---

## Architectural Patterns

### Pattern 1: Thread-Local VM Pool

**What:** A `thread_local!` pool of `Lua` instances. Components borrow a VM for the duration of
their script execution window (mount + render + handler + unmount), then return it.

**When to use:** Single-threaded shell loop. No lock overhead. Pool grows lazily and shrinks via
periodic GC if needed.

**Trade-offs:** VM state pollution risk if a component panics mid-execution without returning the
VM. Mitigate with `PooledVm::drop` always returning to pool, and with full `env_table` isolation
so even a leaked VM is safe to reuse.

### Pattern 2: _ENV Table Isolation via `Chunk::set_environment`

**What:** Each component gets a `Table` as its private global namespace. The pool VM's `_G` is
never touched by component code. The `env_table` holds all per-component globals: `require`,
`self`, `module`, `mesh.*`, and user-defined script globals.

**When to use:** Any time multiple "scripts" share one `Lua` instance. This is the standard
Luau sandboxing idiom endorsed by the Luau embedding guide.

**Trade-offs:** `env_table.pairs()` is used instead of `lua.globals().pairs()` for user global
discovery in `sync_state_from_lua`. The builtin-globals exclusion set (`builtin_globals:
HashSet<String>`) must be rebuilt once per VM checkout and stored on `ScriptContext`, or
captured from the clean `_G` snapshot at pool VM construction time (preferred: snapshot once
at `LuaVmPool::new()` and share immutably).

### Pattern 3: Source-String Chunk Cache with Hash Key

**What:** Before calling `lua.load(source).exec()`, compute `FNV64(source.as_bytes())` and look
up in the cache. On miss, store the source string. On hit, the cache confirms the source was
previously seen (useful for future bytecode elevation) but in v1.17 the main value is skipping
repeated file reads and enabling future bytecode caching without API changes.

**When to use:** Any source that is loaded more than once (same `.mesh` component mounted in
multiple instances, or a component that unmounts and re-mounts).

**Trade-offs:** The cache grows unbounded. Cap at 256 entries with LRU eviction or rely on
module count being naturally small (< 50 modules in a typical MESH session).

---

## Anti-Patterns

### Anti-Pattern 1: Sharing the Same `env_table` Across Component Instances

**What people do:** Reuse one `env_table` for all instances of the same component definition
to save allocation.

**Why it's wrong:** Each component instance must have independent reactive state. Two instances
of the same audio button component will clobber each other's `icon_name`, `audio_label`, etc.

**Do this instead:** Always allocate a fresh `Table` per `ScriptContext` instance at checkout time.

### Anti-Pattern 2: Writing to `lua.globals()` After Isolation

**What people do:** Continue writing service payloads or interface bindings to `lua.globals()`
after switching to `env_table` isolation.

**Why it's wrong:** `lua.globals()` is the pool VM's `_G`, shared across all checked-out VMs.
Writing `__mesh_svc_audio` to `_G` contaminates all components' views.

**Do this instead:** All per-component data (`__mesh_svc_*`, `__mesh_locale_current`,
`__mesh_request_redraw`, `require`, `self`, `module`, `mesh`) must go into `env_table` only.

### Anti-Pattern 3: Pooling Backend VMs in v1.17

**What people do:** Apply the same pool pattern to `BackendScriptContext`.

**Why it's wrong:** Backend modules are long-lived singleton-like processes. A pool of idle
backend VMs provides no benefit since each backend module holds its VM for the entire process
lifetime. Pooling adds complexity with zero win.

**Do this instead:** Apply lazy-init only to backends. Pool only frontend ScriptContext VMs.

### Anti-Pattern 4: Extracting Bytecode via `string.dump` or Unsafe FFI in v1.17

**What people do:** Use Luau's C `luau_compile` + `luau_load` via raw FFI to share compiled
bytecode bytes across VMs.

**Why premature:** The safe mlua API does not expose bytecode bytes. Doing this requires
unsafe FFI into the embedded Luau C library, which bypasses mlua's safety guarantees and is
not needed for the v1.17 goal of eliminating `Lua::new()` per component.

**Do this instead:** Cache source strings in v1.17. Bytecode-level sharing can be a future
phase once the pool architecture is stable.

---

## Integration Points

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| `ScriptContext` -> `LuaVmPool` | Direct field/thread-local call | `ensure_initialized()` calls `pool.checkout()` |
| `ScriptContext` -> `ChunkCache` | `Arc<Mutex<ChunkCache>>` | Cache lives on heap, shared via Arc |
| `FrontendSurfaceComponent` -> `ScriptContext` | Existing API unchanged | `create_runtime_for_component` passes pool/cache refs |
| `Shell` -> `LuaVmPool` | Owns pool (or thread-local) | Pool created once at shell startup |
| `BackendScriptContext` -> pool | None in v1.17 | Backends keep own `Lua`, just defer construction |

### Key Invariants to Preserve

- `ScriptContext::state()` must be readable even before `ensure_initialized()` is called (lazy state
  is empty `ScriptState::new()`, which is a valid empty state).
- `has_handler()` must not trigger VM initialization if the component hasn't been initialized yet
  (return `false` when `initialized == false`).
- `drain_published_events()`, `drain_diagnostics()`, `drain_bound_instance_calls()` must be safe
  to call pre-init (return empty vecs).
- The `builtin_globals` snapshot must reflect the clean `env_table` before user code runs. Capture
  it once at pool VM construction into an immutable `Arc<HashSet<String>>` shared across all
  `ScriptContext` instances using the same pool.

---

## Sources

- mlua crate docs: https://docs.rs/mlua/latest/mlua/struct.Lua.html
- mlua Chunk API: https://docs.rs/mlua/latest/mlua/struct.Chunk.html
- mlua Thread API: https://docs.rs/mlua/latest/mlua/struct.Thread.html
- Luau sandboxing guide: https://luau.org/sandbox/
- Luau C API sandboxing: https://sleitnick.github.io/luau-api/guides/sandboxing.html
- mlua concurrency discussion: https://github.com/mlua-rs/mlua/discussions/494
- Direct source read: `crates/core/runtime/scripting/src/context/runtime.rs`
- Direct source read: `crates/core/runtime/scripting/src/backend/runtime.rs`
- Direct source read: `crates/core/shell/src/shell/component/runtime.rs`

---
*Architecture research for: v1.17 VM Pool + _ENV Isolation + Chunk Caching*
*Researched: 2026-06-02*
