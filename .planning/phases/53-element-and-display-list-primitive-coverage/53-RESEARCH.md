---
phase: 53
slug: element-and-display-list-primitive-coverage
created: 2026-05-22
status: complete
---

# Phase 53 Research

## Implementation Findings

Phase 53 should build on the Phase 51 painter command contract rather than inventing a second primitive model. `PaintBackend` already executes `PainterCommand` slices and the helper methods in `surface/painter/backend.rs` lower rect, rounded rect, stroke, shadow, and backdrop-filter helper calls into commands. The remaining risk is that many authoritative paths still make primitive decisions outside a reusable command-class view.

Direct widget rendering and retained replay are parallel implementations:

- Direct: `render_tree_at_for_module*` -> `render_node_with_filter` -> `render_node_self`.
- Retained: `render_display_list_for_module` -> `render_display_node_self`.
- Control primitives: `render_input_node` / `render_display_input_node`, `render_slider_node` / `render_display_slider_node`, `render_icon_node` / `render_display_icon_node`.
- Text selection: `render_selection_highlights` / `render_display_selection_highlights`.
- Debug overlay: `surface/debug_overlay.rs` calls the public `fill_rect_clipped` helper.

`RetainedDisplayList` already owns `NodeId`, `DisplayPaintCommand`, `DisplayPaintNode`, `DisplayPaintContent`, `DisplayPrimitiveSlot`, material hashing, command ordering, spans, and damage metrics. Phase 53 should not move these responsibilities into `PaintBackend`.

## Recommended Architecture

Add a small command-class recording/test utility around existing painter command emission before broad production refactors. This lets Phase 53 prove equivalence without forcing Skia/text/icon ownership early.

Recommended shape:

1. Add command-class helpers in painter tests or a small private test module:
   - Normalize `PainterCommand` variants into stable class names such as `draw_rect`, `draw_rounded_rect`, `draw_shadow`, `apply_filter`, `draw_text`, `draw_image`.
   - Use `RecordingPaintBackend` for helper-backed commands.
   - Use direct/retained render calls against equivalent node/style fixtures and compare class streams.
2. Move primitive fill decisions into narrow helper methods where duplication exists:
   - Background/border/shadow/filter are already command-backed through helper calls.
   - Selection highlight and debug overlay fills can route through existing command-backed `fill_rect_clipped`.
   - Slider/input decorative fills can be proven through command-backed helper calls.
3. Keep text glyph rendering and icon rasterization as current specialized renderers for now, but add explicit painter command-class intent/proof for their backing rectangles or image-like primitive boundary. Full text/image backend ownership is later.

## Validation Architecture

Phase 53 verification should use automated Rust tests only:

- `cargo test -p mesh-core-render painter_primitive -- --nocapture`
- `cargo test -p mesh-core-render display_list_primitive -- --nocapture`
- `cargo test -p mesh-core-render shipped_surface_painter -- --nocapture`
- `rg "skia_safe" crates/core/frontend/render/src/display_list.rs crates/core/frontend/render/src/render_object.rs && exit 1 || exit 0`

The tests should assert command classes, not pixel-perfect raster output. Pixel fidelity and Skia authoritative primitive rendering belong to Phase 54.

## Pitfalls

- Do not add Skia-specific types to `DisplayPaintCommand`, `DisplayPaintNode`, `DisplayPaintStyle`, or `RenderObject`.
- Do not move retained ordering, identity, damage, material hashes, or module icon resolution into `PaintBackend`.
- Do not delete helper wrappers until tests prove the authoritative path routes through equivalent command classes.
- Do not interpret text/icon specialized renderers as Phase 53 failure if the painter command-class boundary documents their current adapter role.

## Plan Shape

Use four sequential plans:

1. Command-class recorder and parity fixture foundation.
2. Box/background/border/text-selection/debug fill command coverage.
3. Input/slider/icon primitive command coverage.
4. Shipped-surface/direct-vs-retained proof, validation metadata, and helper bypass audit.
