# Renderer Ownership

This document records current source ownership. Historical renderer adoption
research and migration gates are retained under `.planning/renderer/`.

## Authoritative boundaries

| Boundary | Source | Responsibility |
| --- | --- | --- |
| Component parser | `crates/core/ui/component/` | Parses `.mesh` template, script, and style blocks |
| Frontend compiler | `crates/core/frontend/compiler/` | Resolves imports and builds runtime widget trees |
| Element/style/layout model | `crates/core/ui/elements/` | Owns `WidgetNode`, computed style, retained Taffy layout, and semantic state |
| Runtime identity | `crates/core/shell/src/shell/component/runtime_tree.rs` | Preserves node identity and dirty categories across updates |
| Render objects and display list | `crates/core/frontend/render/src/render_object.rs` and `display_list.rs` | Synchronizes retained paint data, ordering, selection, and damage |
| Painter contract | `crates/core/frontend/render/src/surface/painter.rs` | Lowers backend-neutral commands and records diagnostics/profiling |
| Skia backend | `crates/core/frontend/render/src/surface/painter/backend.rs` | Executes low-level raster, path, image, gradient, layer, and effect operations |
| Presentation | `crates/core/presentation/` | Owns presented buffers and normalized surface input |
| Wayland | `crates/core/platform/wayland/` and presentation Wayland modules | Owns compositor protocol integration and surface commits |

## Adapter boundaries

`library_adapters.rs`, `parley_adapter.rs`, `accesskit_adapter.rs`,
`anyrender_adapter.rs`, and proof snapshots are internal integration seams.
They may change without modifying `.mesh` authoring contracts.

## Replacement rule

A replacement renderer or adapter must preserve or deliberately replace:

- stable retained identity;
- typed invalidation and damage;
- layout and text-selection behavior;
- backend-neutral display-list semantics;
- diagnostics and profiling;
- input and accessibility mappings;
- Wayland presentation ownership.

Backend-specific types must not leak into element style, display-list, module,
or public author APIs. The stable author contract is documented in
[the renderer contract](frontend/renderer-contract.md).
