---
phase: 55
status: issues
depth: standard
files_reviewed: 11
findings:
  critical: 0
  warning: 1
  info: 1
  total: 2
---

# Phase 55 Code Review

## Findings

### WR-001: Layer opacity/filter is applied per command rather than as grouped isolation

**Severity:** Warning  
**Files:** `crates/core/frontend/render/src/surface/painter/backend.rs`, `crates/core/frontend/render/src/surface/painter/tests.rs`

`PainterCommand::PushLayer` records layer state, but subsequent draw commands still execute through independent `PixelBuffer::with_skia_canvas` calls. The current implementation applies layer opacity/filter to each command via `layer_paint` instead of preserving a single Skia `saveLayer` across the pushed command range. This passes the single-child pixel tests, but it is not equivalent for grouped opacity/filter when multiple overlapping child commands are inside the same layer.

**Risk:** Overlapping descendants under `opacity < 1` can blend differently than a true isolated group. Layer blur can also blur each primitive independently instead of blurring the composited group.

**Suggested fix:** Refactor `SkiaPaintBackend::execute_commands` so layer ranges are executed in one canvas pass with a real `save_layer`/restore stack, or explicitly downgrade the supported capability and tests to per-command opacity/filter semantics until grouped isolation is implemented.

### IN-001: Skia image/gradient helpers use deprecated skia-safe APIs

**Severity:** Info  
**Files:** `crates/core/frontend/render/src/surface/painter/backend.rs`

The implementation uses `skia_safe::gradient_shader::linear` and `Image::from_raster_data`, both of which compile but emit deprecation warnings in the current `skia-safe` version.

**Suggested fix:** Migrate to `skia_safe::gradient` and `images::raster_from_data()` in a cleanup pass.

## Scope

Reviewed source files changed by Phase 55 summaries:

- `crates/core/frontend/render/src/display_list.rs`
- `crates/core/frontend/render/src/render_object.rs`
- `crates/core/frontend/render/src/surface/icon.rs`
- `crates/core/frontend/render/src/surface/painter.rs`
- `crates/core/frontend/render/src/surface/painter/backend.rs`
- `crates/core/frontend/render/src/surface/painter/tests.rs`
- `crates/core/frontend/render/src/surface/painter/tree.rs`
- `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs`
- `crates/core/ui/elements/src/style.rs`
- `crates/core/ui/elements/src/style/parse.rs`
- `crates/core/ui/elements/src/style/types.rs`

## Test Coverage Observed

The final focused Phase 55 suites passed before this review was written.
