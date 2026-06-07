# Phase 92: VM Pool Foundation - Pattern Map

**Mapped:** 2026-06-07
**Files analyzed:** 3 (2 new, 1 modified)
**Analogs found:** 3 / 3

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/core/runtime/scripting/src/pool.rs` | utility (resource pool) | request-response (checkout/checkin) | `crates/core/shell/src/shell/tests.rs` (RAII Drop pattern) + `crates/core/frontend/compiler/src/style.rs` (thread_local! + RefCell) | role-match (composite) |
| `crates/core/runtime/scripting/src/chunk_cache.rs` | utility (process-wide cache) | request-response (get_or_insert) | `crates/core/frontend/render/src/surface/painter/text.rs` (OnceLock<Mutex<...>>) + `crates/core/shell/src/shell/component/runtime_tree.rs` (FNV-1a) | exact |
| `crates/core/runtime/scripting/src/lib.rs` | config (crate root) | — | `crates/core/runtime/scripting/src/lib.rs` (self — add mod declarations) | self-reference |

---

## Pattern Assignments

### `crates/core/runtime/scripting/src/pool.rs` (utility, request-response)

**Analogs:**
- `crates/core/frontend/compiler/src/style.rs` — thread_local! + RefCell pattern
- `crates/core/shell/src/shell/tests.rs` — RAII Drop guard returning a resource

**Imports pattern** (model from `crates/core/frontend/compiler/src/style.rs` lines 1-4 and `crates/core/runtime/scripting/src/context/runtime.rs` line 12):
```rust
use mlua::Lua;
use std::cell::RefCell;
use std::thread::ThreadId;
```

**Thread-local pool declaration pattern** (`crates/core/frontend/compiler/src/style.rs` lines 57-60):
```rust
thread_local! {
    static INHERITED_STYLE_RULE_INDEX: RefCell<InheritedStyleRuleIndex> =
        RefCell::new(InheritedStyleRuleIndex::default());
}
```
Apply as:
```rust
thread_local! {
    static POOL: RefCell<LuaVmPool> =
        RefCell::new(LuaVmPool::new(4).expect("pool init"));
}
```

**RAII Drop guard pattern** (`crates/core/shell/src/shell/tests.rs` lines 63-72):
```rust
impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.old {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}
```
Apply as the `PooledVm::drop` structure — use `Option<Lua>` on the struct so `take()` in Drop moves the `Lua` back into the pool slot via `POOL.with(...)`. Capture `ThreadId` at construction; `assert_eq!` on drop.

**Lua VM construction pattern** (`crates/core/runtime/scripting/src/context/runtime.rs` line 107):
```rust
lua: Lua::new(),
```
Pool variant adds sandbox immediately after:
```rust
let lua = Lua::new();
lua.sandbox(true).expect("sandbox init failed");
```

**Test pattern** (modeled on `crates/core/runtime/scripting/src/context/tests.rs` lines 1-29 and `crates/core/runtime/scripting/src/storage.rs` lines 372-410):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_floor_vms() { ... }

    #[test]
    fn checkout_returns_pooled_vm_and_drop_recycles() { ... }

    #[test]
    fn pool_grows_on_demand_beyond_floor() { ... }

    #[test]
    #[should_panic]
    fn pooled_vm_dropped_on_wrong_thread_panics() { ... }
}
```

---

### `crates/core/runtime/scripting/src/chunk_cache.rs` (utility, request-response)

**Analogs:**
- `crates/core/frontend/render/src/surface/painter/text.rs` — OnceLock<Mutex<...>> process-wide static cache
- `crates/core/shell/src/shell/component/runtime_tree.rs` — FNV-1a hash constants and inline loop

**Imports pattern** (`crates/core/frontend/render/src/surface/painter/text.rs` lines 4-5):
```rust
use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;
```

**OnceLock static + accessor function pattern** (`crates/core/frontend/render/src/surface/painter/text.rs` lines 9-14):
```rust
static ELLIPSIS_CACHE: OnceLock<Mutex<LruCache<u64, EllipsisCacheEntry>>> = OnceLock::new();

fn ellipsis_cache() -> &'static Mutex<LruCache<u64, EllipsisCacheEntry>> {
    ELLIPSIS_CACHE.get_or_init(|| Mutex::new(LruCache::new(ELLIPSIS_CACHE_CAPACITY)))
}
```
Apply as (no LRU needed — HashMap is sufficient):
```rust
static SOURCE_CACHE: OnceLock<Mutex<HashMap<u64, String>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<u64, String>> {
    SOURCE_CACHE.get_or_init(Default::default)
}
```

**FNV-1a hash pattern** (`crates/core/shell/src/shell/component/runtime_tree.rs` lines 10-11 and 241-251):
```rust
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    // runtime_tree.rs keeps zero out of the id space; ChunkCache can omit
    // that guard since 0 is a valid (if unlikely) cache key.
    hash
}
```
Use the same constants — do not introduce the `fnv` crate.

**Test pattern** (inline `#[cfg(test)]` at bottom of file, same as `crates/core/runtime/scripting/src/storage.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_or_insert_stores_and_returns_source() { ... }

    #[test]
    fn second_lookup_returns_same_string() { ... }

    #[test]
    fn different_sources_get_different_keys() { ... }
}
```

---

### `crates/core/runtime/scripting/src/lib.rs` (crate root, modified)

**Analog:** Self (`crates/core/runtime/scripting/src/lib.rs` lines 1-29 — current file)

**Current mod/pub-use pattern** (lines 1, 18-29):
```rust
pub mod backend;
// ...
pub mod context;
pub mod host_api;
pub mod storage;

pub use backend::{
    BackendScriptContext, BackendScriptError, BackendScriptEvent, StreamLine, StreamState,
};
pub use context::{
    BoundInstanceCall, LocaleBoundState, PublishedEvent, ScriptContext, ScriptError,
    ScriptInterfaceImport, ScriptState,
};
```
Add two new `pub mod` declarations after `pub mod storage;`:
```rust
pub mod chunk_cache;
pub mod pool;
```
No new `pub use` re-exports are required for this phase — the types are standalone and not yet wired into any public surface.

---

## Shared Patterns

### Thread-Local Mutable State
**Source:** `crates/core/frontend/compiler/src/style.rs` lines 4, 57-60
**Apply to:** `pool.rs` — POOL static declaration
```rust
use std::cell::RefCell;

thread_local! {
    static MY_STATE: RefCell<T> = RefCell::new(T::default());
}
// Access: MY_STATE.with(|cell| cell.borrow_mut().method())
```

### RAII Drop Returning Resource
**Source:** `crates/core/shell/src/shell/tests.rs` lines 55-72
**Apply to:** `pool.rs` — PooledVm struct
Key pattern: hold the owned resource in `Option<T>` so `Drop::drop` can call `take()` to move it out without copying and without needing `ManuallyDrop`. Then call back into thread-local state to return the slot.

### Process-Wide OnceLock Cache
**Source:** `crates/core/frontend/render/src/surface/painter/text.rs` lines 5, 9-14
**Apply to:** `chunk_cache.rs` — SOURCE_CACHE static
Pattern: `static FOO: OnceLock<Mutex<Collection>> = OnceLock::new();` with a private accessor function `fn cache() -> &'static Mutex<Collection>` that calls `get_or_init(Default::default)`.

### Inline FNV-1a Hash
**Source:** `crates/core/shell/src/shell/component/runtime_tree.rs` lines 10-11, 241-251
**Apply to:** `chunk_cache.rs` — `fnv64` function
Constants are identical: `FNV_OFFSET = 0xcbf2_9ce4_8422_2325`, `FNV_PRIME = 0x0000_0100_0000_01b3`. Inline byte loop with `^=` then `wrapping_mul`.

### Inline Test Modules
**Source:** `crates/core/runtime/scripting/src/storage.rs` lines 372-410 and `crates/core/runtime/scripting/src/context/tests.rs` lines 1-29
**Apply to:** Both `pool.rs` and `chunk_cache.rs`
Pattern: `#[cfg(test)] mod tests { use super::*; ... }` at the bottom of each source file. No separate test file needed for these two modules (context.rs uses a separate `tests.rs` only because the context module is itself a directory).

---

## No Analog Found

All three files have sufficient codebase analogs. No items in this section.

---

## Metadata

**Analog search scope:** `crates/core/runtime/scripting/`, `crates/core/frontend/compiler/src/`, `crates/core/frontend/render/src/surface/painter/`, `crates/core/shell/src/shell/component/`, `crates/core/shell/src/shell/`
**Files scanned:** 9 source files read directly
**Pattern extraction date:** 2026-06-07
