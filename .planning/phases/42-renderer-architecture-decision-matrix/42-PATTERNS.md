# Phase 42 - Pattern Map

## Existing MESH Patterns To Preserve

- **Retained identity first:** `crates/core/frontend/render/src/render_object.rs` keeps stable render-object slots for transform, clip, opacity, geometry, material, text, and accessibility. Decision artifacts must score every candidate path against retained identity rather than only drawing quality.
- **Display-list boundary:** `crates/core/frontend/render/src/display_list.rs` is the current retained paint boundary. Any AnyRender, Vello, or Skia path should be judged by how well it consumes or replaces this boundary without forcing whole-tree repaint semantics.
- **Painter isolation:** `crates/core/frontend/render/src/surface/painter.rs` owns software painting behind a narrow surface boundary. Candidate renderers should be evaluated as alternate painters/backends, not as replacements for `.mesh`, service, module, or shell runtime.
- **Profiling and diagnostics as runtime contracts:** `crates/core/frontend/render/src/surface/profiling.rs` and debug payloads are existing acceptance surfaces. Phase 42 can allow prototype observability regression only if the final migration plan restores equivalent data.
- **Wayland-native presentation:** `crates/core/presentation/src/lib.rs` and `crates/core/presentation/src/wayland_surface/*` make MESH a shell surface runtime, not a generic app window. Winit and Blitz shell integration must be judged against this constraint.
- **Shipped proof surfaces:** `modules/frontend/navigation-bar/src/main.mesh` and `modules/frontend/audio-popover/src/main.mesh` are the real surface shapes Phase 43 must compare.

## Architecture Borrowing Targets

- Blitz DOM/style/layout/paint/shell boundaries are useful as a reference model even if direct adoption fails.
- Taffy maps to MESH layout needs when driven from retained nodes and custom measurement.
- Parley maps to text layout, glyph traversal, selection geometry, and future editor needs.
- AnyRender maps to a backend abstraction for MESH-owned display-list commands.
- AccessKit maps to retained node identity and atomic accessibility updates.

## Local Anti-Patterns To Avoid

- Do not choose a renderer path because it can render arbitrary web pages. Full browser compatibility is out of scope.
- Do not let Winit or Blitz shell ownership displace existing Wayland/layer-shell lifecycle without an explicit fit verdict.
- Do not treat Skia's prior isolated spike as a production adoption decision.
- Do not defer navigation bar or audio popover from Phase 43; both are required comparison targets.
- Do not accept html5ever/xml5ever unless markup import or Blitz HTML parsing becomes a concrete v1.8 need.
