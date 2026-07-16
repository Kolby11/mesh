# Rendering and Runtime Crate Refactor

## Overview

The frontend path is now split into explicit compile, runtime, paint, and presentation layers. The goal is to keep author-facing `.mesh` compilation, shell runtime orchestration, software painting, and Wayland/dev-window presentation from growing back into one coupled crate.

## Current Boundaries

| Crate | Path | Responsibility |
|-------|------|----------------|
| `mesh-core-component` | `crates/core/ui/component` | Parses `.mesh` single-file components into template, script, style, and import structures. |
| `mesh-core-frontend` | `crates/core/frontend/compiler` | Compiles frontend modules, resolves local component imports, lowers source tags through `UiTag`, and builds `WidgetNode` trees. |
| `mesh-core-elements` | `crates/core/ui/elements` | Owns the runtime widget tree, computed style, layout, accessibility, and element state contracts. |
| `mesh-core-interaction` | `crates/core/ui/interaction` | Provides hit testing, focus traversal, scroll helpers, and widget-tree interaction queries. |
| `mesh-core-render` | `crates/core/frontend/render` | Paints `WidgetNode` trees into `PixelBuffer`s, including text, glyph, icon, debug overlay, and primitive widget drawing. |
| `mesh-core-presentation` | `crates/core/presentation` | Presents `PixelBuffer`s through the dev-window or layer-shell backend and normalizes input events. |
| `mesh-core-shell` | `crates/core/shell` | Glues module discovery, scripting runtime, services, surface configuration, component invalidation, rendering, and presentation into the shell event loop. |

## Rendering Flow

```text
.mesh source
  -> mesh-core-component parser
  -> mesh-core-frontend compiler/lowering
  -> CompiledFrontendModule
  -> FrontendSurfaceComponent runtime in mesh-core-shell
  -> WidgetNode tree + retained dirty summary
  -> mesh-core-render painter
  -> PixelBuffer
  -> mesh-core-presentation backend
  -> dev window or layer-shell surface
```

The shell still owns when a surface needs work. `FrontendSurfaceComponent` tracks dirty categories, script state, interaction state, retained widget identity, and service/theme/locale invalidation. Painting itself is delegated to `mesh-core-render`, and surface creation or commit is delegated to `mesh-core-presentation`.

## Runtime Flow

```text
Shell::run()
  -> discover modules and compile frontend catalog
  -> create frontend component runtimes
  -> spawn backend Luau providers on Tokio
  -> handle backend/service/IPC messages
  -> tick components and drain CoreRequest queues
  -> render dirty components
  -> present buffers and pump presentation events
```

Runtime scripting remains in `mesh-core-scripting` and backend polling/commands remain in `mesh-core-backend`. The runtime crates should not depend on software painting, text shaping, glyph caches, or presentation backends.

## What Moved

- Frontend compilation now lives in `crates/core/frontend/compiler`.
- Software rendering now lives in `crates/core/frontend/render`.
- Surface/window presentation now lives in `crates/core/presentation`.
- Sandbox runtime metadata lives in `crates/core/runtime/sandbox`.
- Frontend host contract types live in `crates/core/frontend/host`.
- Surface layout policy resolution lives in `crates/core/surface-config`.
- Animation and interaction helpers live in `crates/core/ui/animation` and `crates/core/ui/interaction`.

## Dependency Direction

Normal dependency direction is:

```text
shell -> frontend compiler
shell -> render
shell -> presentation
shell -> animation / interaction / surface-config / scripting / backend
presentation -> render
render -> elements + icon
frontend compiler -> component + elements + module + theme
interaction -> elements
animation -> elements
```

Lower-level crates should not import `mesh-core-shell`. If render, presentation, or compiler code needs a shell-facing concept, define a small contract type in the appropriate boundary crate instead of reaching upward.

## Code Entry Points

- Compile frontend modules: `crates/core/frontend/compiler/src/compile.rs`
- Build widget trees: `crates/core/frontend/compiler/src/render.rs`
- Lower source tags: `crates/core/frontend/compiler/src/tags.rs`
- Paint surfaces: `crates/core/frontend/render/src/surface/painter.rs`
- Text and glyph rendering: `crates/core/frontend/render/src/surface/text.rs`, `crates/core/frontend/render/src/surface/glyph.rs`
- Icon painting: `crates/core/frontend/render/src/surface/icon.rs`
- Presentation selection and commit: `crates/core/presentation/src/lib.rs`
- Layer-shell backend: `crates/core/presentation/src/wayland_surface/`
- Shell render loop: `crates/core/shell/src/shell/runtime/render.rs`
- Component runtime and invalidation: `crates/core/shell/src/shell/component.rs`
- Retained widget identity and dirty summary: `crates/core/shell/src/shell/component/runtime_tree.rs`

## Practical Rules

- Put source parsing in `mesh-core-component`.
- Put source-to-runtime lowering and widget tree construction in `mesh-core-frontend`.
- Put runtime-inspectable element, style, layout, accessibility, and state contracts in `mesh-core-elements`.
- Put hit testing, focus, scroll, and tree queries in `mesh-core-interaction`.
- Put pixel-buffer painting, text measurement, glyphs, icons, and debug overlay drawing in `mesh-core-render`.
- Put layer-shell/dev-window commit and input normalization in `mesh-core-presentation`.
- Put service state, scripting, request draining, and event-loop orchestration in `mesh-core-shell`.

This split keeps rendering reusable and testable without letting runtime crates own paint backends or letting render code own shell policy.
