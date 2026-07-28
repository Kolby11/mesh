---
status: passed
phase: 23
verified: 2026-05-09
---

# Phase 23 Verification: Text Shaping and Glyph Cache

## Result

Status: `passed`

## Requirement Coverage

- `TEXT-01`: Passed. Text layout output is cached by stable shaping inputs including content, font family, font size, font weight, line height, wrapping width, and alignment.
- `TEXT-02`: Passed. Unchanged text layout buffers are reused across repeated measure/render/selection operations, and the renderer continues using `SwashCache` for glyph raster reuse.
- `TEXT-03`: Passed. Cache keys invalidate on content, font, size, weight, line height, wrapping width, and alignment changes; selection overlay work reuses shaping when metrics are unchanged.

## Commands

- `cargo fmt --check`
- `cargo test -p mesh-core-render text_cache`
- `cargo test -p mesh-core-render selection_geometry_preserves_utf8_boundaries`
- `cargo test -p mesh-core-render`
- `nix develop -c cargo test -p mesh-core-shell profiling_snapshot_exposes_typed_surface_invalidation_counts`

## Residual Risk

Glyph cache internals are owned by `cosmic_text::SwashCache`, which does not expose per-glyph hit/miss counters through the current API. The debug metric reports glyph cache activity honestly rather than fabricating glyph-level counts.
