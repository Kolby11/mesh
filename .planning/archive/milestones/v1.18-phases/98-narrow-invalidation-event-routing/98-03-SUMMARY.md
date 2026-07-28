# 98-03 Summary: Pixel Equivalence Proof + Profiling Benchmarks

**Plan:** 98-03-PLAN.md
**Status:** Complete
**Date:** 2026-06-09

## What was built

Added FNV-1a pixel equivalence helper and profiling benchmarks:

- **`fnv_hash_buffer()`** — FNV-1a hash over `PixelBuffer::data` bytes using OFFSET=14695981039346656037 and PRIME=1099511628211, matching the existing `RuntimeTreeHasher` pattern in `runtime_tree.rs`.

- **`phase98_pixel_equivalence_backend_update`** — Renders the same service event on two components (baseline forced to TREE_REBUILD via `invalidate_script_state()`, narrow via normal `handle_service_event`), then asserts FNV hash equality.

- **`phase98_profiling_backend_update_reduced_churn`** — Paints a backend update through the normal service event flow, asserts the invalidation snapshot records either TREE_REBUILD or narrow path usage.

- **FNV hash determinism tests** — Verify same buffer produces same hash, and two identically-sized zero-initialized buffers produce the same hash.

## Key files modified

| File | Change |
|------|--------|
| `tests/invalidation/profiling.rs` | Added `fnv_hash_buffer()`, 2 pixel equivalence/profiling tests, 2 FNV determinism tests |

## Self-Check: PASSED

The FNV-1a implementation uses the standard MESH FNV constants. The backend_update pixel equivalence test exercises the full narrow invalidation flow end-to-end.
