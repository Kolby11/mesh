---
status: complete
phase: 25
plan: 1
completed: 2026-05-09
---

# Summary 25-01: Display-List Batching Metrics and GPU Guardrails

## Completed

- Added typed display-list batching barrier reasons in `mesh-core-render`.
- Added conservative adjacent primitive batching summaries for retained display-list entries.
- Counted compatible batches, batched primitives, barrier totals, and barrier reason occurrences.
- Extended retained paint debug metrics with batching counters and barrier reason counters.
- Serialized batching metrics in debug profiling JSON under `invalidation.paint`.
- Added GPU-readiness proof documentation for future GPU and parallel paint/layout handoff.

## Files Changed

- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/frontend/render/src/lib.rs`
- `crates/core/foundation/debug/src/lib.rs`
- `crates/core/shell/src/shell/component.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`
- `crates/core/shell/src/shell/tests.rs`
- `.planning/phases/25-display-list-batching-and-gpu-readiness-guardrails/25-GPU-READINESS.md`

## Verification

- `cargo fmt --check` — passed
- `cargo test -p mesh-core-render display_list_batches_adjacent_compatible_primitives` — passed
- `cargo test -p mesh-core-render display_list_records_batch_barriers` — passed
- `cargo test -p mesh-core-render` — passed
- `nix develop -c cargo test -p mesh-core-shell profiling_snapshot_exposes_typed_surface_invalidation_counts` — passed

## Notes

- Batching is metadata-only. The software painter remains the authoritative paint path.
- No GPU command abstraction or backend implementation was added.
