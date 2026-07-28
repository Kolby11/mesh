# Phase 92: VM Pool Foundation - Research

**Researched:** 2026-06-07
**Domain:** mlua 0.11 thread-local VM pooling, RAII slot guards, FNV64 content-hash cache (Rust)
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- `LuaVmPool` is thread-local; each pool VM initialized with `Lua::sandbox(true)` so stdlib tables are read-only
- `PooledVm` is a RAII drop guard that returns its slot to the pool; must assert same-thread identity on drop
- Pool grows on-demand with minimum 4 VM floor; never blocks on exhaustion
- `ChunkCache` is process-wide (not thread-local), keyed on FNV64 content hash of source strings
- No changes to `ScriptContext` behavior — pool and cache are standalone types only

### Claude's Discretion
All other implementation choices are at Claude's discretion — pure infrastructure phase.

### Deferred Ideas (OUT OF SCOPE)
- Bytecode cross-VM sharing via `luau_compile`/`luau_load` C FFI
- Pool size auto-tuning
- Backend VM pooling
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| POOL-01 | Thread-local Lua VM pool; each pool VM initialized once with `Lua::sandbox(true)` | Verified: mlua 0.11 `Lua::sandbox(true)` API confirmed. `thread_local!` + `RefCell<Vec<Lua>>` is the standard pattern. |
| POOL-02 | `ScriptContext` checks out a pooled VM via RAII drop guard (`PooledVm`) | Phase 92 scope: only define `PooledVm` type. Checkout plumbing into `ScriptContext` is Phase 94. Pool/guard types are standalone this phase. |
| POOL-03 | Pool grows on-demand with minimum 4 VM floor; simultaneous checkouts never exhaust pool | `grow_to_floor()` on construction + on-demand growth in `checkout()`. No blocking — always create a new VM slot if all existing ones are checked out. |
| POOL-04 | `PooledVm` RAII guard asserts same thread identity on drop | Capture `std::thread::current().id()` at checkout; compare in `Drop::drop()`; `debug_assert!` or `assert!` on mismatch. |
| CACHE-01 | Process-wide source-string cache keyed on FNV64 content hash | `OnceLock<Mutex<HashMap<u64, String>>>`. FNV-1a hand-rolled following existing codebase pattern in `runtime_tree.rs`. |
| CACHE-02 | Pool VM checkout loads the cached source string rather than re-reading from disk | Phase 92 scope: `ChunkCache` stores and retrieves source strings. Integration with checkout is Phase 94. |
</phase_requirements>

---

## Summary

Phase 92 introduces two isolated, independently testable types into `mesh-core-scripting`: a thread-local `LuaVmPool` with a `PooledVm` RAII guard, and a process-wide `ChunkCache` keyed on FNV64 content hashes. Neither type modifies `ScriptContext` behavior — they are pure additions that existing surfaces are unaware of.

The key technical constraints are fully understood from direct codebase inspection and confirmed against mlua 0.11 documentation. With the `send` feature enabled in `mesh-core-scripting/Cargo.toml`, `mlua::Lua` is `Send + Sync` — this means the pool must be `thread_local!` for correctness isolation (preventing concurrent use of the same VM from different threads), not merely because `Lua` is `!Send`. The thread-ID assertion in `PooledVm::drop` exists to detect the specific bug where a checked-out VM is sent to a different thread via `std::thread::spawn` or Tokio and dropped there, which would silently return the slot to the wrong thread's pool.

The `ChunkCache` has no novel challenges: it follows the `OnceLock<Mutex<HashMap<...>>>` pattern already in use for four other caches in `mesh-core-render`. The FNV-1a hash should be hand-rolled following the existing codebase implementation in `runtime_tree.rs` — no `fnv` crate is needed or consistent with workspace style.

**Primary recommendation:** Implement `LuaVmPool` as `thread_local! { static POOL: RefCell<LuaVmPool> }` with `Lua::sandbox(true)` on each VM at construction. `PooledVm` is a newtype wrapping `Lua` + captured `ThreadId`. `ChunkCache` is a newtype wrapping `Arc<Mutex<HashMap<u64, String>>>`.

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| VM lifecycle (construct, checkout, return) | Scripting runtime | — | Pool is a runtime-internal resource; no UI or service concern |
| Thread isolation enforcement | Scripting runtime | — | Thread-ID assertion belongs on the RAII guard at the resource layer |
| Source string caching | Scripting runtime | — | Cache is keyed on source content, not module identity; lives adjacent to load_script |
| Existing `ScriptContext` behavior | Scripting runtime (unchanged) | — | Phase 92 does not touch ScriptContext; behavioral layer is frozen |

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `mlua` | 0.11.6 [VERIFIED: Cargo.lock] | Luau VM construction and sandbox initialization | Already in workspace; `Lua::sandbox(true)` API confirmed present |
| `std::thread_local!` | stable | Thread-local pool storage | Zero-cost per-thread ownership; avoids lock contention on the single render thread |
| `std::sync::OnceLock` | stable | Process-wide cache initialization | Already used for four caches in `mesh-core-render`; idiomatic Rust for lazy static singletons |
| `std::sync::Mutex` | stable | Cache write protection | Standard choice for `OnceLock`-wrapped shared maps; no async needed |
| `std::thread::ThreadId` | stable | Thread identity for drop assertion | `std::thread::current().id()` is the exact API the CONTEXT.md references |

### No New External Crates Required

The `fnv` crate is **not used**. The codebase already has a hand-rolled FNV-1a implementation in `crates/core/shell/src/shell/component/runtime_tree.rs` (constants `FNV_OFFSET` and `FNV_PRIME`, inline byte loop). The `ChunkCache` hash function must follow the same pattern for workspace consistency. [VERIFIED: direct source read of `runtime_tree.rs` lines 10-11, 242-250]

---

## Package Legitimacy Audit

> This phase adds **no new external packages** to `Cargo.toml`. All required capabilities are provided by the already-present `mlua 0.11.6` and the Rust standard library.

| Package | Status | Notes |
|---------|--------|-------|
| mlua 0.11.6 | Already in workspace | No action required |
| std | Built-in | No action required |

**Packages removed due to slopcheck [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

---

## Architecture Patterns

### System Architecture Diagram

```
Thread A (render thread)
  └── thread_local! POOL: RefCell<LuaVmPool>
        ├── slot 0: Lua (sandbox, idle)
        ├── slot 1: Lua (sandbox, idle)
        ├── slot 2: Lua (sandbox, checked out → PooledVm)
        └── slot 3: Lua (sandbox, idle)

Process-wide
  └── CHUNK_CACHE: OnceLock<Mutex<HashMap<u64, String>>>
        ├── 0xabc... → "icon_name = ...\nfunction render..."
        └── 0xdef... → "count = 0\n..."

PooledVm (on stack, RAII)
  ├── lua: Lua           ← moved out of pool slot during checkout
  ├── slot_index: usize  ← which slot to return on drop
  └── owner_thread: ThreadId  ← asserted in Drop
```

Data flows:

1. `LuaVmPool::checkout()` — finds an idle slot, moves `Lua` out of `Vec`, wraps in `PooledVm`
2. `PooledVm::drop()` — asserts `ThreadId`, moves `Lua` back into pool slot
3. `ChunkCache::get_or_insert(source)` — hashes source bytes with FNV-1a, returns clone of cached string or inserts the new one

### Recommended Project Structure

```
crates/core/runtime/scripting/src/
├── pool.rs          ← LuaVmPool, PooledVm (new)
├── chunk_cache.rs   ← ChunkCache (new)
├── context.rs       ← unchanged (re-exports ScriptContext)
├── backend.rs       ← unchanged
├── host_api.rs      ← unchanged
├── storage.rs       ← unchanged
└── lib.rs           ← add pub mod pool; pub mod chunk_cache; pub use ...
```

### Pattern 1: Thread-Local Pool with `RefCell`

**What:** `thread_local!` stores a `RefCell<LuaVmPool>`. Pool methods are called via `with(|pool| pool.borrow_mut()...)`.
**When to use:** Any `!Send`-by-ownership resource that must be accessed from multiple call sites on the same thread without passing ownership through function parameters.
**Key constraint with `send` feature:** `mlua::Lua` is `Send` when the `send` feature is on, but Luau VMs are not safe for concurrent use from multiple threads (the internal mutex serializes access but does not enable true parallelism). Thread-local storage ensures each thread's pool VMs are never shared cross-thread.

```rust
// Source: crates/core/frontend/compiler/src/style.rs (existing codebase pattern)
thread_local! {
    static INHERITED_STYLE_RULE_INDEX: RefCell<InheritedStyleRuleIndex> =
        RefCell::new(InheritedStyleRuleIndex::default());
}

// Adapted for pool:
thread_local! {
    static POOL: RefCell<LuaVmPool> = RefCell::new(LuaVmPool::new(4).expect("pool init"));
}

pub fn checkout() -> PooledVm {
    POOL.with(|pool| pool.borrow_mut().checkout())
}
```

### Pattern 2: RAII Guard Returning to Thread-Local Pool

**What:** `PooledVm` owns a `Lua` plus metadata. `Drop` returns the `Lua` to the thread-local pool.
**When to use:** Any resource that must be borrowed exclusively and returned to a shared pool after use.
**Critical detail:** Because the pool is `thread_local!`, the `Drop` impl calls `POOL.with(...)`. This is safe as long as the drop happens on the same thread. The `ThreadId` assertion catches violations early.

```rust
// Conceptual pattern (not verbatim — planner writes actual code):
pub struct PooledVm {
    lua: Option<mlua::Lua>,   // Option so we can take() in Drop
    slot_index: usize,
    owner_thread: std::thread::ThreadId,
}

impl Drop for PooledVm {
    fn drop(&mut self) {
        assert_eq!(
            std::thread::current().id(),
            self.owner_thread,
            "PooledVm dropped on a different thread than checkout"
        );
        if let Some(lua) = self.lua.take() {
            POOL.with(|pool| pool.borrow_mut().return_slot(self.slot_index, lua));
        }
    }
}
```

### Pattern 3: Process-Wide Cache with `OnceLock<Mutex<HashMap>>`

**What:** Static cache initialized on first access using `OnceLock`. Protected by `Mutex` for concurrent writes.
**When to use:** Process-wide shared state that is initialized once and then read/written from any thread.

```rust
// Source: crates/core/frontend/render/src/surface/painter/text.rs (existing codebase pattern)
static ELLIPSIS_CACHE: OnceLock<Mutex<LruCache<u64, EllipsisCacheEntry>>> = OnceLock::new();

// Adapted for ChunkCache (no LRU needed — source strings are small and evicted explicitly):
static SOURCE_CACHE: OnceLock<Mutex<HashMap<u64, String>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<u64, String>> {
    SOURCE_CACHE.get_or_init(Default::default)
}
```

### Pattern 4: FNV-1a Hash (Inline, No External Crate)

**What:** 64-bit FNV-1a hash over byte slice. Same constants as `runtime_tree.rs`.
**When to use:** Content-addressed keys where a fast non-cryptographic hash suffices.

```rust
// Source: crates/core/shell/src/shell/component/runtime_tree.rs lines 10-11, 242-250
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64  = 0x0000_0100_0000_01b3;

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
```

### Anti-Patterns to Avoid

- **Storing `mlua::Function` in ChunkCache:** A `Function` is VM-bound — loading it into a different `Lua` instance is undefined behavior. The cache MUST store `String` (source text) or `Vec<u8>` (bytecode bytes), never `Function`. [ASSUMED — based on mlua type model; the type system enforces this if the cache type signature is `HashMap<u64, String>`]
- **Using `Arc<Mutex<LuaVmPool>>` instead of `thread_local!`:** Would require acquiring a mutex on every checkout/checkin, adding contention on the render thread. The `thread_local!` pattern is zero-cost once per-thread initialization has happened.
- **`LuaVmPool` without a floor:** Pool must pre-warm at least 4 VMs at construction. If growth is always lazy (starting from 0), the first 4 simultaneous surface initializations each create a new VM synchronously inside `checkout()`, causing a stall. Pre-warming amortizes this cost.
- **Not resetting VM state on checkin:** Phase 92 scope does not include the reset logic (that is Phase 94 / ISO-03), but the pool design must **reserve** the reset point. The `return_slot` method should have a comment noting that Phase 94 will add `thread.reset()` before reinsertion.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| VM sandbox initialization | Custom stdlib-stripping | `Lua::sandbox(true)` via mlua | mlua's sandbox sets read-only on stdlib tables and activates `safeenv`; hand-rolling misses string metatable and metatables on builtin types |
| Thread-local access | `unsafe static` + manual TLS | `std::thread_local!` | Standard, safe, zero-cost after first access |
| Process-wide singleton | Unsafe lazy_static | `std::sync::OnceLock` | Stable Rust since 1.70; already used in codebase |
| FNV hash | External `fnv` crate | Inline constants + byte loop | Codebase already has the pattern; no new dep needed |

---

## Runtime State Inventory

Not applicable — Phase 92 is a greenfield addition of two new types with no migration or rename involved.

---

## Common Pitfalls

### Pitfall 1: `send` Feature Makes `Lua` `Send`, Not Thread-Safe

**What goes wrong:** Developer assumes that because `mlua::Lua` is `Send` (via the `send` feature), VMs can be safely passed to other threads and used concurrently.
**Why it happens:** The `send` feature documentation says `Lua` becomes `Send + Sync`. This is true for ownership transfer, but Luau's internal state is still not designed for concurrent execution from multiple threads simultaneously.
**How to avoid:** Keep pool as `thread_local!`. The thread-ID assertion in `PooledVm::drop` catches the accidental-send bug path.
**Warning signs:** Checkout and drop on different `ThreadId` values → assertion fires.

### Pitfall 2: `PooledVm::drop` Panics During Stack Unwinding

**What goes wrong:** If an assertion fires in `Drop` while the thread is already panicking (e.g., during a test that panics), Rust aborts the process.
**Why it happens:** Double-panic on the same thread causes abort in Rust.
**How to avoid:** Use `debug_assert!` in `Drop` for the thread-ID check (fires in test/debug builds, stripped in release). Add a note in the code about this choice. The CONTEXT.md says "detectable runtime assertion" which is satisfied by `debug_assert!`.
**Warning signs:** Test failures with `process aborted` instead of `test failed`.

### Pitfall 3: Pool `RefCell` Already Borrowed When `checkout()` Is Re-entered

**What goes wrong:** If a closure passed to `POOL.with(...)` itself calls `checkout()` recursively (e.g., Luau script calls a host API that triggers another checkout), `RefCell::borrow_mut()` panics with "already borrowed".
**Why it happens:** `RefCell` tracks borrows at runtime; reentrant mutable borrows panic.
**How to avoid:** Phase 92 does not wire pool into any scripting code path. Document the invariant: pool checkout and checkin must not be reentrant from within a single logical script execution. This is enforced naturally by the Phase 94 design (one checkout per `ScriptContext` activation).
**Warning signs:** `BorrowMutError` panic in pool-related code.

### Pitfall 4: `Lua::sandbox(true)` Called After Any Script Execution

**What goes wrong:** If `sandbox()` is called after globals are modified (e.g., after `load_script` is called on the VM), the stdlib tables are already dirty and the sandbox protection is incomplete.
**Why it happens:** `Lua::sandbox(true)` must be the very first operation after `Lua::new()` (or `Lua::new_with(...)`). It configures the VM's initial state, not a later snapshot.
**How to avoid:** In `LuaVmPool::new()`, call `lua.sandbox(true)` immediately after `Lua::new()` and before any other operation. Assert this in comments and do not expose the raw `Lua` for mutation before sandbox is set.
**Warning signs:** Pool VMs show mutable stdlib tables in tests (write to `string.format` succeeds).

### Pitfall 5: ChunkCache Returns Stale Source After Hot-Reload

**What goes wrong:** Source string is cached by FNV64 hash of the old content. After the file changes, the old hash is still in the cache. New load sees the new content → different hash → cache miss → correct new source is used. But if two paths (hot-reload watcher and cache lookup) race, a transient stale hit is possible.
**Why it happens:** Content-addressed cache is self-invalidating by design — a changed file produces a different hash. However the explicit eviction wiring is Phase 95 scope.
**How to avoid:** Phase 92 scope: note in `ChunkCache` documentation that explicit eviction (`remove(hash)`) is wired by Phase 95. The content-hash key means stale entries are unreachable after source changes anyway (new hash → new entry), so correctness is not compromised even before eviction.
**Warning signs:** Memory growth if many reload cycles happen before eviction is wired.

---

## Code Examples

### `Lua::sandbox(true)` Initialization

```rust
// Source: https://docs.rs/mlua/latest/mlua/struct.Lua.html#method.sandbox
// Available only with `luau` feature. Sets all stdlib tables to read-only,
// activates safeenv, and installs a local environment proxy.
let lua = mlua::Lua::new();
lua.sandbox(true).expect("sandbox init failed");
// After this point: string.format, table.insert, etc. are all read-only.
// Component-level writes must go into a per-component _ENV table (Phase 94).
```

### Thread-Local Pool Access (Existing Codebase Pattern)

```rust
// Source: crates/core/frontend/compiler/src/style.rs (pattern reference)
thread_local! {
    static POOL: std::cell::RefCell<LuaVmPool> =
        std::cell::RefCell::new(LuaVmPool::new(4).expect("pool init"));
}

// Public entry point (module-level function):
pub fn checkout() -> PooledVm {
    POOL.with(|cell| cell.borrow_mut().checkout())
}
```

### `OnceLock<Mutex<HashMap>>` Cache (Existing Codebase Pattern)

```rust
// Source: crates/core/frontend/render/src/surface/painter/text.rs (pattern reference)
use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;

static SOURCE_CACHE: OnceLock<Mutex<HashMap<u64, String>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<u64, String>> {
    SOURCE_CACHE.get_or_init(Default::default)
}
```

### FNV-1a Hash (Existing Codebase Pattern)

```rust
// Source: crates/core/shell/src/shell/component/runtime_tree.rs lines 10-11, 242-250
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64  = 0x0000_0100_0000_01b3;

pub fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `Lua::new()` per `ScriptContext` | Pool with `Lua::sandbox(true)` once per VM | v1.17 (this milestone) | Eliminates per-component VM construction cost |
| Source re-read from disk per activation | `ChunkCache` source string cache | v1.17 (this milestone) | Eliminates per-activation disk I/O for multi-instance modules |
| `mlua 0.10` owned types with `'lua` lifetime | `mlua 0.11` no lifetime, weak references internally | mlua 0.11.0 | Enables storing `Lua` in `Vec` inside pool without lifetime complications |

**Deprecated/outdated:**

- `mlua` owned types with `'lua` lifetime: Removed in 0.11.0. The codebase already uses 0.11.6 so this is not a concern, but any old forum advice involving `'lua` lifetime on `Function` or `Table` is inapplicable.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `Lua::sandbox(true)` must be called immediately after `Lua::new()` before any globals modifications | Pitfall 4 | Low — the pool constructs fresh VMs and sandboxes them before returning; if order matters differently, the sandbox call sequence can be trivially adjusted |
| A2 | `PooledVm::drop` calling back into `thread_local!` pool is safe (no re-entrancy) | Pattern 2 | Low — drop is called at scope exit, never from within an active pool borrow |
| A3 | `std::thread::current().id()` returns a stable, unique `ThreadId` for the lifetime of a thread | Pitfall 1 | Low — this is a fundamental Rust stdlib guarantee |
| A4 | The existing shell render loop is single-threaded (all frontend scripting runs on one thread) | Architecture patterns | Medium — if frontend scripting is ever moved to a thread pool, the `thread_local!` pool design correctly handles it (each thread gets its own pool), but the minimum-4-VM floor would need to be per-pool |

**If this table is empty:** Not empty. All items are low-risk and do not require user confirmation before execution.

---

## Open Questions

1. **`LuaVmPool` module-level checkout API vs. method-on-pool API**
   - What we know: Both `POOL.with(|p| p.borrow_mut().checkout())` (global function) and constructing a `LuaVmPool` and calling `.checkout()` on it (explicit ownership) are valid designs.
   - What's unclear: The CONTEXT.md says `LuaVmPool::checkout()` returns a `PooledVm` — this implies an explicit pool type with a method. The thread-local static wrapping it is an implementation detail.
   - Recommendation: Implement `LuaVmPool` as a concrete struct with `checkout(&mut self) -> PooledVm`. The `thread_local!` static is a module-level convenience that wraps it. Tests instantiate `LuaVmPool` directly without touching the static, enabling clean unit testing per the success criteria.

2. **`debug_assert!` vs `assert!` in `PooledVm::drop`**
   - What we know: The success criterion says "triggers a detectable runtime assertion." `debug_assert!` fires only in debug builds; `assert!` fires in all builds.
   - What's unclear: Whether the test that verifies this (success criterion 3) should use debug or release build.
   - Recommendation: Use `assert!` (not `debug_assert!`) so the check is always active and the test passes in any build profile. The overhead is negligible — drop is called once per component activation.

---

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | All compilation | Confirmed (build works) | 1.85 (workspace rust-version) | — |
| mlua 0.11.6 | Pool VM construction | Confirmed in Cargo.lock | 0.11.6 | — |
| Luau feature in mlua | `Lua::sandbox(true)` | Confirmed in Cargo.toml features | `luau` | — |
| send feature in mlua | `Lua: Send` | Confirmed in Cargo.toml features | `send` | — |

**Missing dependencies with no fallback:** none

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in (`#[test]`) |
| Config file | none — inline `#[cfg(test)]` modules at bottom of source files |
| Quick run command | `cargo test -p mesh-core-scripting` |
| Full suite command | `cargo test -p mesh-core-scripting` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| POOL-01 | `LuaVmPool::new(4)` creates 4 sandbox-initialized VMs | unit | `cargo test -p mesh-core-scripting pool` | ❌ Wave 0 |
| POOL-02 | `checkout()` returns a `PooledVm` that returns its slot on drop | unit | `cargo test -p mesh-core-scripting pool` | ❌ Wave 0 |
| POOL-03 | Pool grows on-demand; 8 simultaneous checkouts succeed without blocking | unit | `cargo test -p mesh-core-scripting pool` | ❌ Wave 0 |
| POOL-04 | `PooledVm` dropped on different thread triggers assertion | unit | `cargo test -p mesh-core-scripting pool` | ❌ Wave 0 |
| CACHE-01 | `ChunkCache::get_or_insert(src)` stores and returns source string keyed by FNV64 | unit | `cargo test -p mesh-core-scripting chunk_cache` | ❌ Wave 0 |
| CACHE-02 | Second lookup returns same string without re-inserting | unit | `cargo test -p mesh-core-scripting chunk_cache` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test -p mesh-core-scripting`
- **Per wave merge:** `cargo test -p mesh-core-scripting`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `crates/core/runtime/scripting/src/pool.rs` — `LuaVmPool`, `PooledVm` with `#[cfg(test)]` block covering POOL-01 through POOL-04
- [ ] `crates/core/runtime/scripting/src/chunk_cache.rs` — `ChunkCache` with `#[cfg(test)]` block covering CACHE-01 and CACHE-02
- [ ] `crates/core/runtime/scripting/src/lib.rs` — add `pub mod pool; pub mod chunk_cache;`

---

## Security Domain

Phase 92 introduces no network I/O, user input handling, secret management, or data persistence. `Lua::sandbox(true)` is a security-relevant operation (it makes stdlib tables read-only), but the correctness invariant is fully covered by unit tests rather than ASVS controls. `security_enforcement` is not set in `.planning/config.json`.

---

## Sources

### Primary (HIGH confidence)

- `crates/core/runtime/scripting/src/context/runtime.rs` — Direct source read: `ScriptContext::new()` calls `Lua::new()` on line 107; `lua: Lua` field on line 44; no existing pool or cache.
- `crates/core/runtime/scripting/Cargo.toml` — Direct source read: `mlua = { version = "0.11", features = ["luau", "serialize", "send"] }` confirms sandbox and send features.
- `crates/core/shell/src/shell/component/runtime_tree.rs` — Direct source read: hand-rolled FNV-1a at lines 10-11 and 242-250; exact constants to reuse.
- `crates/core/frontend/compiler/src/style.rs` — Direct source read: `thread_local!` + `RefCell` pattern for in-crate mutable state.
- `crates/core/frontend/render/src/surface/painter/text.rs` — Direct source read: `OnceLock<Mutex<LruCache>>` pattern for process-wide caches.
- `.planning/research/SUMMARY.md` — Milestone research: confirms no new crate dependencies needed; source-string cache is correct for v1.17.
- `https://docs.rs/mlua/latest/mlua/struct.Lua.html` — `Lua::sandbox()` API: sets stdlib tables read-only; `luau` feature only.
- `https://docs.rs/mlua/latest/mlua/struct.Thread.html` — `Thread::sandbox()` and `Thread::reset()` APIs confirmed for Phase 94 use.

### Secondary (MEDIUM confidence)

- `https://github.com/mlua-rs/mlua` CHANGELOG.md — mlua 0.11.0: `send` feature adds `Send + Sync` to `Lua` and associated types; `'lua` lifetime removed; breaking changes documented.

### Tertiary (LOW confidence)

- Prior research SUMMARY.md note on `Compiler::compile()` bytecode cross-VM loading — rates this LOW confidence; source-string cache is the safe fallback and is explicitly the Phase 92 scope.

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all libraries are already in the workspace with confirmed feature flags
- Architecture: HIGH — based on direct source reads of the exact files to modify
- Pitfalls: HIGH — grounded in mlua 0.11 documentation and existing codebase patterns; no speculative items

**Research date:** 2026-06-07
**Valid until:** 2026-07-07 (mlua 0.11 is stable; no breaking changes expected)
