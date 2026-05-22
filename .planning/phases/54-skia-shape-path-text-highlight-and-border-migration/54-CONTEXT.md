---
phase: 54
slug: skia-shape-path-text-highlight-and-border-migration
status: context
created: 2026-05-22
autonomous: true
smart_discuss_fallback: true
---

# Phase 54 Context

## Goal

Make Skia authoritative for core shape rasterization while preserving MESH ownership of text layout, selection semantics, retained identity, style lowering, damage, and command ordering.

## Requirements

- **SKIA-01**: Skia owns rasterization, antialiasing, paths, rounded rects, strokes, clipping, and blend modes for core shape primitives.
- **SKIA-04**: Remaining MESH-owned software fallback code for painter primitives is removed or isolated behind non-authoritative compatibility tests.
- **TEXT-01**: The painter engine preserves current text measurement, drawing, and theme-owned selection behavior while allowing text-adjacent rectangles and future text primitives to route through the painter API.

## Defaults Accepted

Interactive input was unavailable in Default mode, so Phase 54 uses the recommended conservative defaults:

- Treat Skia as authoritative for `DrawRect`, `DrawRoundedRect`, stroke-style rounded rects, `DrawPath`, clip-constrained shape execution, and blend/filter paint parameters already represented by `PainterCommand`.
- Preserve `TextRenderer` as the current text measurement and glyph drawing owner.
- Route text-adjacent rectangles, including selection highlights, through existing painter commands without migrating glyph drawing in this phase.
- Preserve existing border/focus/selection visual behavior unless tests identify a current bug.
- Keep Skia-specific types out of retained display-list, render-object, and public style data.
- Remove or fence old software primitive fallbacks only after command-backed pixel tests cover the replacement behavior.

## Existing Foundation

- Phase 51 introduced `PainterCommand`, `PaintBackend`, `SkiaPaintBackend`, capability reporting, diagnostics, and command execution.
- Phase 53 proved direct widget-tree and retained display-list paths emit equivalent command classes for supported primitive classes.
- `FrontendRenderEngine` helper methods now lower rect, rounded rect, stroke, shadow, filter, selection highlight, controls, and debug overlay fills into painter commands on authoritative paths.

## Key Files

- `crates/core/frontend/render/src/surface/painter/backend.rs`
  - Skia command execution and fallback compatibility helpers.
- `crates/core/frontend/render/src/surface/painter/geometry.rs`
  - Remaining geometry utilities such as clip math and rounded-rect coverage.
- `crates/core/frontend/render/src/surface/painter/tree.rs`
  - Border drawing, box/background lowering, direct and retained node rendering.
- `crates/core/frontend/render/src/surface/painter/text.rs`
  - Text handoff and selection highlight rectangle painting.
- `crates/core/frontend/render/src/surface/painter/widgets.rs`
  - Input, slider, icon, and scrollbar primitive painting.
- `crates/core/frontend/render/src/surface/painter/tests.rs`
  - Pixel and command-class tests for painter behavior.

## Implementation Boundaries

- In scope: Skia-backed shape execution, path execution, clipping behavior, stroke/border pixel behavior, text-adjacent highlight rectangles, debug/control fill pixels, removal/fencing of old software shape fallbacks.
- Out of scope: Full glyph/text rendering migration, broad effect/layer/shadow/gradient implementation, backend-selection UX, animation integration, damage expansion beyond shape/path requirements.

## Verification Expectations

- Shape/path/border tests should run under the local Nix graphics environment:

```bash
export LIBRARY_PATH=/nix/store/kkgs1h2qidn50b5c5gndjrjz3v54jrq1-freetype-2.13.3/lib:/nix/store/mw89hwpv8x37px7dh1l0csnz7yv4iln2-fontconfig-2.17.1-lib/lib
export LD_LIBRARY_PATH=$LIBRARY_PATH
cargo test -p mesh-core-render skia_shape -- --nocapture
cargo test -p mesh-core-render skia_path -- --nocapture
cargo test -p mesh-core-render skia_border -- --nocapture
cargo test -p mesh-core-render skia_text_highlight -- --nocapture
```

- Backend-neutrality remains required:

```bash
rg "skia_safe" crates/core/frontend/render/src/display_list.rs crates/core/frontend/render/src/render_object.rs && exit 1 || exit 0
```
