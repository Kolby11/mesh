---
phase: 54-skia-shape-path-text-highlight-and-border-migration
status: complete
created: 2026-05-22
---

# Phase 54 Research

## Current State

- `SkiaPaintBackend` already executes `DrawRect`, `DrawRoundedRect`, `DrawShadow`, and backdrop `ApplyFilter`.
- `DrawRect` fill and rectangular stroke still use software `PixelBuffer::clear_rect` through `fill_rect_impl` and `stroke_rect_impl`.
- Rounded fill/stroke prefer Skia helpers on `PixelBuffer`, with a software rounded-coverage fallback still present.
- `DrawPath` is still diagnostic-only with `UnsupportedPainterFeature::Path`.
- `PushClip`/`PopClip` are still diagnostic-only even though commands carry per-command clips.
- Text glyph rendering remains owned by `TextRenderer`; selection highlight rectangles already route through `DrawRect`.
- Retained display-list and render-object data remain Skia-free.

## Implementation Direction

1. Move rectangular fill/stroke rasterization into `buffer.with_skia_canvas` using `skia_safe::Canvas::draw_rect`.
2. Add `DrawPath` execution by converting `PainterPath`/`PainterPathElement` into `skia_safe::Path`, with fill and stroke support.
3. Add command-sequence clip stack execution inside `SkiaPaintBackend::execute_commands`, intersecting push clips with each command's existing clip.
4. Keep layer/effect work deferred to Phase 55 except for preserving current diagnostics.
5. Preserve text glyph drawing and selection behavior; add pixel tests proving selection highlight rectangles still paint with theme-owned colors.

## Verification Targets

```bash
cargo test -p mesh-core-render skia_shape -- --nocapture
cargo test -p mesh-core-render skia_path -- --nocapture
cargo test -p mesh-core-render skia_border -- --nocapture
cargo test -p mesh-core-render skia_text_highlight -- --nocapture
rg "skia_safe" crates/core/frontend/render/src/display_list.rs crates/core/frontend/render/src/render_object.rs && exit 1 || exit 0
```

Use local Nix graphics library paths for test linking:

```bash
export LIBRARY_PATH=/nix/store/kkgs1h2qidn50b5c5gndjrjz3v54jrq1-freetype-2.13.3/lib:/nix/store/mw89hwpv8x37px7dh1l0csnz7yv4iln2-fontconfig-2.17.1-lib/lib
export LD_LIBRARY_PATH=$LIBRARY_PATH
```
