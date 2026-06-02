# Stack Research

**Domain:** mlua VM pooling, _ENV isolation, and compiled chunk caching in Rust
**Researched:** 2026-06-02
**Confidence:** HIGH (all key claims verified against mlua 0.11 docs and official Luau documentation)

---

## Context

MESH already uses `mlua = "0.11"` with the `luau` and `send` features. The current model creates one `mlua::Lua` instance per `ScriptContext` via `Lua::new()` on every component mount. This milestone replaces that model with:

1. A per-thread VM pool so components share VMs
2. `_ENV`-based isolation so each component gets a private namespace within a shared VM
3. Lazy-init so inactive components skip VM allocation entirely
4. Shared pre-compiled Luau bytecode chunks reused across all VMs

No new crate dependencies are required for goals 1-4. All needed APIs are already in `mlua 0.11`.

---

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `mlua` | 0.11 (already in workspace) | All four VM consolidation goals | Provides `Chunk::set_environment()`, `Compiler::compile()`, `Lua::load()` with binary mode, `Lua::sandbox()` for Luau — no version bump needed |
| Rust `std::thread_local!` | stdlib | Per-thread `Lua` VM storage | VMs are not `Send` across threads without the mutex overhead of the `send` feature; `thread_local!` gives zero-cost per-thread ownership with no contention |
| Rust `std::sync::OnceLock<Mutex<HashMap<String, Vec<u8>>>>` | stdlib | Shared compiled bytecode cache | Pre-compiled `Vec<u8>` from `Compiler::compile()` is plain bytes — safe to store in a global static, loaded per-VM with `set_mode(ChunkMode::Binary)` |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `mlua::Compiler` | bundled with mlua 0.11 luau feature | Compile Luau source to `Vec<u8>` bytecode at startup | Use once per unique source string; store result in global cache; skip re-compilation on subsequent mounts of the same component |
| `mlua::ChunkMode::Binary` | bundled with mlua 0.11 | Load pre-compiled `Vec<u8>` into any VM | Use in `lua.load(&bytecode).set_mode(ChunkMode::Binary)` when a cached chunk exists for the module |
| `mlua::Lua::sandbox()` | bundled with mlua 0.11, luau feature | Make shared VM globals read-only and activate safeenv | Call once after constructing the pool VM so component scripts cannot mutate stdlib tables in the shared globals |
| `mlua::Chunk::set_environment()` | bundled with mlua 0.11 | Redirect a chunk's `_ENV` upvalue to a per-component table | Use at load time per component: `lua.load(&bytecode).set_environment(component_env).into_function()` |
| `mlua::Lua::new_with(StdLib, LuaOptions)` | bundled with mlua 0.11 | Create VMs with a reduced stdlib footprint | Use instead of `Lua::new()` when constructing pool VMs to exclude `io`, `os`, `coroutine`, `package` — libraries component scripts should not reach |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| `cargo bench` + existing `mesh-core-scripting` tests | Measure per-component mount cost before and after | Benchmark `ScriptContext::new()` + `load_script()` as the baseline; measure pool acquire + `into_function()` with env table as the target path |
| `heaptrack` / `valgrind massif` | Confirm heap reduction from shared VMs | Run against the shipped navigation-bar surface; expect a measurable drop in per-frame allocations once pool is wired in |

---

## Key API Details (Verified Against mlua 0.11 Docs)

### VM Pool — `thread_local!` Pattern

The `send` feature makes `Lua` internally mutex-guarded and `Send`, but that introduces lock contention on every VM access. For a shell that runs all component rendering on one main thread, `thread_local!` is strictly better: zero lock overhead, no `Send` requirement on closures or userdata. Both coexist cleanly — the workspace can keep the `send` feature for the backend module path while frontend rendering uses `thread_local!` VMs.

```rust
thread_local! {
    static LUA_POOL: std::cell::RefCell<Vec<mlua::Lua>> = std::cell::RefCell::new(Vec::new());
}

fn acquire_vm() -> mlua::Lua {
    LUA_POOL.with_borrow_mut(|pool| {
        pool.pop().unwrap_or_else(|| {
            let lua = mlua::Lua::new_with(
                mlua::StdLib::BASE
                    | mlua::StdLib::STRING
                    | mlua::StdLib::TABLE
                    | mlua::StdLib::MATH,
                mlua::LuaOptions::default(),
            )
            .expect("lua vm init");
            // sandbox() makes builtins read-only; component _ENV tables can
            // still write locals because they shadow, not modify, the global table.
            lua.sandbox(true).expect("sandbox failed");
            lua
        })
    })
}

fn release_vm(lua: mlua::Lua) {
    LUA_POOL.with_borrow_mut(|pool| pool.push(lua));
}
```

The pool holds `Lua` instances directly — no `Arc`, no `Mutex`. The thread-local guarantees exclusive access within the rendering thread.

### _ENV Isolation — Per-Component Private Namespace

`Chunk::set_environment(env: Table)` replaces the `_ENV` upvalue of the chunk before it is loaded. The effect: all global reads and writes inside the script go to `env`, not to `lua.globals()`. Two components sharing one VM cannot observe each other's state.

The component `env` table uses `__index` delegation so stdlib functions (`math.floor`, `string.format`, `table.insert`, etc.) still resolve from the VM's read-only global table without being copied:

```rust
fn make_component_env(lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
    let env = lua.create_table()?;
    let meta = lua.create_table()?;
    // Reads that miss the env table fall through to the shared VM globals.
    // The VM's globals are read-only (sandbox mode), so scripts cannot
    // mutate stdlib tables even through the __index chain.
    meta.set("__index", lua.globals())?;
    env.set_metatable(Some(meta))?;
    Ok(env)
}

fn load_component_script(
    lua: &mlua::Lua,
    bytecode: &[u8],
    env: mlua::Table,
) -> mlua::Result<mlua::Function> {
    lua.load(bytecode)
        .set_mode(mlua::ChunkMode::Binary)
        .set_environment(env)
        .into_function()
}
```

After `into_function()` the returned `Function` carries the `_ENV` upvalue pointing at `env`. Calling it executes the script with that namespace active.

The host API tables (`self`, `mesh`, `require`, `module`, service keys starting with `__mesh_svc_`) must be written into `env`, not `lua.globals()`, otherwise they bleed across components sharing the same VM.

### Chunk Caching — Shared Compiled Bytecode

`mlua::Compiler::compile()` is independent of any `Lua` instance. It turns Luau source text into `Vec<u8>` bytecode that can be loaded into any VM with `set_mode(ChunkMode::Binary)`. The `Vec<u8>` has no VM affinity and is safe to store in a global static.

```rust
use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;

static CHUNK_CACHE: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();

fn get_or_compile(module_id: &str, source: &str) -> Vec<u8> {
    let cache = CHUNK_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap();
    if let Some(cached) = guard.get(module_id) {
        return cached.clone();
    }
    let bytecode = mlua::Compiler::new()
        .set_optimization_level(1)
        .set_debug_level(1) // line info + function names; sufficient for backtraces
        .compile(source);
    guard.insert(module_id.to_string(), bytecode.clone());
    bytecode
}
```

Cache key is `module_id` (already a stable string per `ScriptContext`). Hot-reload invalidates the entry by removing it from the map before recompiling. Cache entries are plain `Vec<u8>` with no lifetime dependency on any `Lua` instance.

Note: `Compiler::compile()` returns `Vec<u8>` directly for Luau (not `Result`) — syntax errors manifest as runtime errors when the bytecode is loaded into a VM. Validate source at compile time if early error reporting is needed.

### Lazy-Init — Deferred VM Acquisition

The simplest lazy-init pattern wraps VM acquisition in an `Option` and defers it until first use:

```rust
pub struct ScriptContext {
    module_id: String,
    capabilities: CapabilitySet,
    lua: Option<mlua::Lua>,  // None until first load_script() or call_handler()
    // ... remaining fields unchanged
}

impl ScriptContext {
    fn lua_mut(&mut self) -> &mut mlua::Lua {
        self.lua.get_or_insert_with(acquire_vm)
    }
}

impl Drop for ScriptContext {
    fn drop(&mut self) {
        if let Some(lua) = self.lua.take() {
            release_vm(lua);
        }
    }
}
```

Components that are mounted but never rendered (hidden surfaces, inactive popovers) never acquire a VM. The pool shrinks to the number of simultaneously active components rather than the total number of mounted ones.

---

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| `thread_local!` VM pool | `Arc<Mutex<Lua>>` shared across threads | Only if MESH ever moves component rendering to a work-stealing thread pool; today all rendering is single-threaded so the mutex is pure overhead |
| `Compiler::compile()` bytecode cache returning `Vec<u8>` | Caching `mlua::Function` objects | `Function` holds an internal reference to the `Lua` instance that created it and cannot be used with a different instance; `Vec<u8>` is VM-agnostic and is the correct cross-VM sharing unit |
| `__index` metatable for `_ENV` fallback to VM globals | Copying all stdlib entries into each component env | Copying full stdlib per component wastes memory and breaks sandbox invariants; `__index` delegation keeps one authoritative copy in the VM's global table |
| `Lua::sandbox()` called once on pool VM construction | `sandbox()` per-component env | `sandbox()` operates on the whole VM's global table, not on individual env tables; call it once when constructing the pool VM; per-component isolation comes from the separate `env` table |
| `mlua::Lua::new_with(StdLib::BASE | STRING | TABLE | MATH, ...)` | `Lua::new()` | `Lua::new()` loads all safe stdlib including `io`, `os`, `coroutine`, `package`; component scripts should not access those; `new_with()` excludes them at construction time |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `mlua::Function` as the cross-VM bytecode cache unit | `Function` holds an internal reference to the originating `Lua` state; using it with a different `Lua` instance is undefined behavior | `mlua::Compiler::compile()` returning `Vec<u8>` — plain bytes with no VM affinity |
| `lua.sandbox(true)` after user scripts have already run | `sandbox()` is designed for pre-script VM setup; called after scripts load it may corrupt closures' upvalue chains | Call `sandbox(true)` once during pool VM construction, before any component scripts are loaded into the VM |
| `mlua::Lua::clone()` for "copying" VMs | `Lua` clone is a reference-counted handle to the same underlying C state; cloning does not produce a second isolated VM | `Lua::new_with(...)` or pool acquisition for a fresh isolated VM |
| Writing per-component state to `lua.globals()` | Globals are shared across all components on the same VM; the existing `builtin_globals` snapshot heuristic assumes globals are VM-exclusive and breaks under pool sharing | Per-component `env` table passed via `set_environment`; write all component-private data to `env` |
| `Lua::set_globals()` for per-call environment swapping | The mlua docs warn that "existing Lua functions have cached global environment and will not see the changes"; it cannot be used for per-call isolation | `Chunk::set_environment(env)` before `into_function()` — modifies the `_ENV` upvalue at load time, which is the correct mechanism |

---

## Integration Notes for MESH

The existing `ScriptContext` in `crates/core/runtime/scripting/src/context/runtime.rs` is the only change surface. Key integration points:

**`ScriptContext::new()` / `new_with_storage_root()`** — replace `lua: Lua::new()` with `lua: None` (lazy) or `lua: Some(acquire_vm())`. The pool VM must have `sandbox()` applied once at construction time; per-component setup happens via `env`.

**`install_host_api()`** — currently writes `self`, `mesh`, `require`, `module`, `__mesh_request_redraw`, `__mesh_locale_current` into `lua.globals()`. Under the pool model these must go into `env` instead, because `lua.globals()` is shared across all components on the VM. The `__index` fallback on `env` ensures stdlib still resolves from globals.

**`sync_state_from_lua()`** — currently iterates `lua.globals()` for user globals. Under env isolation it must iterate `env` instead. The `builtin_globals` snapshot logic can be simplified: the `env` table starts empty, so every key in it after `load_script` is user-authored (no need to diff against a pre-script snapshot).

**`load_script_with_interface_imports()`** — the `lua.load(source).set_name(...).exec()` call becomes: `get_or_compile(module_id, source)` to get `Vec<u8>`, then `lua.load(&bytecode).set_mode(ChunkMode::Binary).set_environment(env).into_function()?.call(())`.

**`ScriptContext` cleanup** — after `flush_storage()` the component's `env` table is dropped with the context. Before returning the VM to the pool, clear any side-channel globals that `install_host_api` wrote into `lua.globals()` during the prior ownership period (if any were written there by mistake), then push the VM back via `release_vm(self.lua.take().unwrap())`.

---

## Version Compatibility

| Package | Compatible With | Notes |
|---------|-----------------|-------|
| `mlua 0.11` with `luau` feature | `Compiler::compile()`, `Chunk::set_environment()`, `Lua::sandbox()`, `ChunkMode::Binary` | All four APIs are present in 0.11; no version bump needed |
| `mlua 0.11` with `send` feature | `thread_local!` pool on the rendering thread | `send` enables `Lua: Send + Sync` with internal mutex for backend threads; `thread_local!` bypasses that mutex for the single-threaded render path; both coexist |
| Rust 1.85 (workspace minimum) | `OnceLock`, `RefCell::borrow_mut`, `thread_local!` | All stable since Rust 1.70 |

---

## Sources

- `/websites/rs_crate_mlua` via Context7 — `Chunk` methods, `Compiler` struct, `LuaOptions`, `sandbox()` API
- https://docs.rs/mlua/0.11.0/mlua/struct.Chunk.html — `set_environment()` signature, `into_function()`, `set_mode()` confirmed HIGH confidence
- https://docs.rs/mlua/0.11.0/mlua/struct.Lua.html — `new_with()`, `StdLib`, `sandbox()`, `set_globals()` caveats confirmed HIGH confidence
- https://docs.rs/mlua/latest/mlua/struct.Function.html — `dump()`, VM-affinity behavior confirmed HIGH confidence
- https://github.com/mlua-rs/mlua/discussions/494 — per-thread VM pattern; maintainer recommendation for true parallelism MEDIUM confidence (discussion thread)
- https://github.com/mlua-rs/mlua/discussions/137 — bytecode `Vec<u8>` as cross-VM sharing unit; `ChunkMode::Binary` loading MEDIUM confidence (discussion thread)
- https://luau.org/sandbox/ — `__index`-based global table delegation for per-script env isolation; read-only builtin strategy HIGH confidence (official Luau documentation)

---
*Stack research for: mlua VM pooling, _ENV isolation, chunk caching — MESH v1.17*
*Researched: 2026-06-02*
