---
status: complete
phase: 23
plan: 1
completed: 2026-05-09
---

# Summary 23-01: Text Layout Cache and Debug Metrics

## Completed

- Added a bounded text layout cache inside `TextRenderer`.
- Reused cached `cosmic_text::Buffer` layouts across measurement, rendering, and selection geometry.
- Keyed cached layouts by text, font family, font size, font weight, line height, wrapping width, and alignment.
- Added text cache metrics for layout hits, misses, invalidations/evictions, shaped entries, and glyph cache activity.
- Returned text cache metrics from frontend paint calls without breaking existing paint callers.
- Extended profiling invalidation snapshots and debug JSON with text cache metrics under `invalidation.text`.

## Files Changed

- `crates/core/frontend/render/src/surface/text.rs`
- `crates/core/frontend/render/src/surface/painter.rs`
- `crates/core/frontend/render/src/surface/mod.rs`
- `crates/core/frontend/render/src/lib.rs`
- `crates/core/foundation/debug/src/lib.rs`
- `crates/core/shell/src/shell/component.rs`
- `crates/core/shell/src/shell/component/shell_component.rs`
- `crates/core/shell/src/shell/runtime/debug.rs`
- `crates/core/shell/src/shell/tests.rs`

## Verification

- `cargo fmt --check` — passed
- `cargo test -p mesh-core-render text_cache` — passed
- `cargo test -p mesh-core-render selection_geometry_preserves_utf8_boundaries` — passed
- `cargo test -p mesh-core-render` — passed
- `nix develop -c cargo test -p mesh-core-shell profiling_snapshot_exposes_typed_surface_invalidation_counts` — passed

## Notes

- The existing `cosmic_text::SwashCache` remains the glyph raster cache; Phase 23 reports that glyph caching is active without claiming internal glyph hit/miss counts that the dependency does not expose.
- Full text rendering behavior remains software-rendered and visually unchanged.
