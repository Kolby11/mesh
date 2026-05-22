---
phase: 54-skia-shape-path-text-highlight-and-border-migration
status: complete
created: 2026-05-22
---

# Phase 54 Patterns

## Files

- `crates/core/frontend/render/src/surface/painter/backend.rs`
  - Extend `SkiaPaintBackend` command execution.
  - Keep Skia imports private to backend implementation.
  - Preserve diagnostics for out-of-scope text/image/layer work.
- `crates/core/frontend/render/src/surface/painter/geometry.rs`
  - Keep clip and geometry math backend-neutral.
  - Remove or fence software fallback code only after pixel tests pass.
- `crates/core/frontend/render/src/surface/painter/tests.rs`
  - Add pixel tests with `skia_shape_`, `skia_path_`, `skia_border_`, and `skia_text_highlight_` prefixes.
  - Reuse `PixelBuffer`, `full_clip`, `pixel`, `FrontendRenderEngine`, and `PainterCommand` fixtures.

## Existing Test Style

- Use focused Rust unit tests in `painter/tests.rs`.
- Prefer pixel assertions for Phase 54 because the phase is about raster behavior, not just command classes.
- Keep tests deterministic and small: 16-80 px buffers, opaque colors, and precise sample points.
- Use command filters so targeted feedback stays fast.

## Boundaries

- Do not add Skia types to `display_list.rs`, `render_object.rs`, or style data.
- Do not migrate glyph text drawing.
- Do not implement full layer/effect behavior; leave diagnostics for Phase 55.
