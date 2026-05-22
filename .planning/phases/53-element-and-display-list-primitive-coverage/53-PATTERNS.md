---
phase: 53
slug: element-and-display-list-primitive-coverage
created: 2026-05-22
status: complete
---

# Phase 53 Pattern Map

## Target Files And Closest Analogs

| Target | Role | Closest Analog | Pattern To Follow |
|---|---|---|---|
| `crates/core/frontend/render/src/surface/painter/tests.rs` | Command-class and parity tests | Existing `RecordingPaintBackend` tests | Use `FrontendRenderEngine::with_paint_backend`, collect `PainterCommand` values, assert variants/classes. |
| `crates/core/frontend/render/src/surface/painter/backend.rs` | Painter command contract | Phase 51 command helpers | Keep commands backend-neutral and diagnostics non-fatal for unsupported features. |
| `crates/core/frontend/render/src/surface/painter/tree.rs` | Direct and retained node primitive routing | Existing `render_node_self` / `render_display_node_self` symmetry | Keep direct and retained logic structurally parallel; avoid backend ownership of `DisplayPaintNode`. |
| `crates/core/frontend/render/src/surface/painter/widgets.rs` | Input/slider/icon primitives | Existing direct/display method pairs | Add or test paired direct/display behavior together. |
| `crates/core/frontend/render/src/surface/painter/text.rs` | Selection highlight rectangles | Existing direct/display selection highlight helpers | Route highlight fill proof through command-backed rect helpers while preserving TextRenderer. |
| `crates/core/frontend/render/src/surface/debug_overlay.rs` | Debug layout-bounds fills | Existing public `fill_rect_clipped` helper | Treat as compatibility helper path; prove helper emits `DrawRect`. |
| `crates/core/frontend/render/src/display_list.rs` | Retained command content and identity | Existing `DisplayPaintCommand` data model | Preserve `NodeId`, command ordering, spans, material hashes, and damage metrics. |

## Existing Test Patterns

- `painter_command_contract_constructs_required_command_set` asserts the command enum covers the required contract.
- `painter_backend_capabilities_identify_skia_and_unsupported_commands_diagnose` checks backend capabilities and diagnostics.
- Existing retained display-list tests in `painter/tests.rs` build widget trees, update `RetainedDisplayList`, and replay through `FrontendRenderEngine`.
- Existing shipped-surface tests compile navigation/audio `.mesh` surfaces and render proof snapshots.

## Concrete Reuse

- Reuse `RecordingPaintBackend` rather than adding a separate mock backend.
- Reuse `node(...)`, `text_node(...)`, `full_clip(...)`, and `pixel(...)` helpers in `painter/tests.rs` where possible.
- Add helper assertions that reduce `PainterCommand` into stable class strings. Do not compare every numeric rect unless the test is specifically about geometry.
- Prefer test names starting with `painter_primitive_`, `display_list_primitive_`, or `shipped_surface_painter_` so Phase 53 verification commands can target them.

## Guardrails

- No `skia_safe` in retained display-list or render-object data.
- No new public author-facing MESH elements.
- No broad rewrite of text/icon rasterization.
- No changes to module icon resolution semantics.
