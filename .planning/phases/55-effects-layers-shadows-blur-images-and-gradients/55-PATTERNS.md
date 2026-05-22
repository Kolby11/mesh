# Phase 55 Pattern Map

## Purpose

Closest existing analogs for implementing Phase 55 effects, layers, images, and gradients without violating MESH renderer ownership boundaries.

## Files And Analogs

| Target File | Role | Closest Existing Analog | Pattern To Follow |
|-------------|------|-------------------------|-------------------|
| `crates/core/ui/elements/src/style/types.rs` | Backend-neutral style structs for background image/gradient | Existing `BoxShadow`, `VisualFilter`, `TransitionProperties` | Small copyable structs/enums with `Default`/`NONE` values; no backend types |
| `crates/core/ui/elements/src/style/parse.rs` | Bounded parser for `background-image` | `parse_filter`, `parse_box_shadow` | Accept compact supported syntax; return `NONE`/diagnostics for unsupported values |
| `crates/core/ui/elements/src/style/resolve.rs` | Resolver integration | Existing `box-shadow`, `filter`, `backdrop-filter` arms | Resolve variables first, then parse into `ComputedStyle` |
| `crates/core/ui/elements/src/style.rs` | Style/profile tests | Existing animation/filter/shadow diagnostics tests | Test exact supported syntax and diagnostic messages |
| `crates/core/frontend/render/src/display_list.rs` | Retained effect/image/gradient data and visual bounds | Existing `DisplayPaintNode`, `visual_clip_for`, `damage_rect_for_node_at`, material signatures | Store backend-neutral fields, hash them, and expand bounds only where pixels escape layout |
| `crates/core/frontend/render/src/render_object.rs` | Retained render-object dirty slots | Existing `material_hash` and primitive hash | Include new style paint data in material/primitive dirty summaries |
| `crates/core/frontend/render/src/surface/painter/backend.rs` | Painter commands and Skia execution | Existing `DrawShadow`, `ApplyFilter`, `fill_shape`, `draw_path_command` | Add commands first, then execute through `with_skia_canvas`, and update capabilities after tests |
| `crates/core/frontend/render/src/surface/painter/tree.rs` | Direct/retained lowering | Existing direct/retained shadow/filter/background lowering | Keep direct and retained paths equivalent; shared helpers are preferred |
| `crates/core/frontend/render/src/surface/painter/tests.rs` | Pixel and command-class proof | Existing `RecordingPaintBackend`, `painter_command_classes`, Skia shape tests | Add focused tests named with `painter_effect`, `skia_effect`, and `display_list_effect` prefixes |
| `crates/core/frontend/render/src/surface/icon.rs` | Image cache precedent | Existing file freshness and raster variant cache | Reuse or adapt cache concepts; avoid broad cache tuning |

## Data Flow

```text
StyleResolver
  -> ComputedStyle background/effect fields
  -> RenderObjectTree material dirty summary
  -> DisplayPaintNode backend-neutral fields
  -> direct and retained painter command lowering
  -> SkiaPaintBackend executes supported commands or emits PainterDiagnostic
```

## Constraints

- No `skia_safe` references in `display_list.rs`, `render_object.rs`, or `mesh-core-elements`.
- Keep retained `NodeId`, z-order, command filtering, and damage inputs outside `PaintBackend`.
- Add tests before changing capability flags to supported.
- Unsupported browser-like syntax diagnoses; it does not become silent fallback.
