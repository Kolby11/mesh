---
phase: 92-vm-pool-foundation
plan: 02
status: complete
completed: 2026-06-07
requirements_met: [CACHE-01, CACHE-02]
---

# Summary: Plan 92-02 — ChunkCache (FNV64 source-string cache)

## What was built

- `crates/core/runtime/scripting/src/chunk_cache.rs` — new file with:
  - `ChunkCache`: zero-sized handle type; all methods are associated functions
  - Process-wide `OnceLock<Mutex<HashMap<u64, String>>>` static backing store
  - FNV-1a 64-bit hash (`fnv64`) using same constants as `runtime_tree.rs`
  - `get_or_insert`, `get`, `remove`, `len` methods
- `crates/core/runtime/scripting/src/lib.rs` — `pub mod chunk_cache;` added

## Tests

All 5 unit tests pass:
- `get_or_insert_stores_and_returns_source` — stores and returns correct key
- `second_lookup_returns_same_string` — cache grows by exactly 1 for duplicate insert
- `different_sources_get_different_keys` — distinct sources yield distinct keys
- `fnv64_matches_reference_value` — FNV-1a empty-input and single-byte reference values
- `remove_evicts_entry` — `remove()` makes `get()` return `None`

## Notes

- No `fnv` crate added; inline FNV-1a using constants from `runtime_tree.rs`
- Stores `String` source, not `mlua::Function` (VM-bound, unsafe to share)
- `remove()` pre-wires Phase 95 hot-reload eviction path with a comment marker
- Tests use unique source strings per test to avoid cross-test cache contamination
