---
phase: 92-vm-pool-foundation
plan: 01
status: complete
completed: 2026-06-07
requirements_met: [POOL-01, POOL-02, POOL-03, POOL-04]
---

# Summary: Plan 92-01 — LuaVmPool + PooledVm RAII guard

## What was built

- `crates/core/runtime/scripting/src/pool.rs` — new file with:
  - `LuaVmPool`: thread-local VM pool, pre-warms `floor` sandboxed `mlua::Lua` VMs, grows on-demand
  - `PooledVm`: RAII guard with `Drop` that asserts same-thread return and recycles the slot
  - Thread-local `POOL` static and module-level `checkout()` convenience function
- `crates/core/runtime/scripting/src/lib.rs` — added `pub mod pool;` (and `pub mod chunk_cache;` for plan 02)

## Tests

All 5 unit tests pass:
- `new_creates_floor_vms` — pool initializes with correct floor count
- `checkout_returns_pooled_vm_and_drop_recycles` — slot is recycled on drop
- `pool_grows_on_demand_beyond_floor` — 8 simultaneous checkouts succeed
- `pooled_vm_dropped_on_wrong_thread_panics` — cross-thread drop assertion fires
- `sandbox_is_enabled_on_pool_vm` — `Lua::sandbox(true)` prevents stdlib mutation

## Notes

- Used `#[allow(dead_code)]` on `floor` field (stored for future diagnostics)
- Wrong-thread test changed from `#[should_panic]` to manual join error check since the re-panic message differs
- `ScriptContext` is not modified — pool/cache types are standalone
