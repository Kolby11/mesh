# Phase 53: Element And Display-List Primitive Coverage - Context

**Gathered:** 2026-05-22
**Status:** Ready for planning
**Mode:** Smart discuss fallback defaults accepted automatically because interactive questions are unavailable in this runtime mode.

<domain>
## Phase Boundary

Phase 53 ensures every currently supported MESH element/control emits retained
painter intent through backend-neutral commands. It should inventory direct
widget-tree painting and retained display-list replay, then move authoritative
box/text-adjacent/icon/image-like/slider/input/control/debug fill paths behind
the Phase 51 painter command/backend boundary. It must preserve retained
`NodeId` identity, layout/style ownership, material hashes, command ordering,
accessibility metadata, module icon resolution, and shipped navigation/audio
behavior. It should not make Skia authoritative for raster details, paths,
effects, layers, images, gradients, animation invalidation, or damage policy;
those belong to later phases.

</domain>

<decisions>
## Implementation Decisions

### Primitive Coverage
- Inventory the current supported MESH element/control vocabulary and cover only
  the shipped/declared primitives that already exist: generic boxes/rows/columns,
  text-adjacent selection/highlight rectangles, icons, image-like icon/source
  primitives, sliders, inputs, control fills, scrollbars, and debug overlay fills.
- Keep arbitrary browser/DOM primitives out of scope. Phase 53 should not add
  new author-facing elements just to satisfy painter coverage.
- Treat shipped navigation/audio modules as the compatibility proof surfaces.
  Their controls should continue to render through the same class of commands
  when direct and retained paths are compared.
- Preserve compatibility wrappers where needed, but the authoritative render
  paths should emit or replay backend-neutral painter commands rather than
  bypassing the command backend with helper-shaped calls.

### Direct vs Retained Equivalence
- Direct widget-tree painting and retained display-list replay should converge on
  the same command classes for equivalent node/style inputs.
- Prefer shared lowering helpers or command recorder fixtures over duplicating
  primitive-specific logic in `render_node_self` and `render_display_node_self`.
- Retained `DisplayPaintCommand`, `DisplayPaintNode`, `DisplayPaintContent`,
  `DisplayPrimitiveSlot`, `NodeId`, material hashes, ordering, and damage inputs
  remain MESH-owned. Do not move identity/order/damage decisions into the backend.
- It is acceptable to add test-only recording backends/fixtures to compare command
  classes without requiring pixel-perfect output for every primitive.

### Testing And Proof
- Add command-class fixture coverage for box/background/border, text selection
  highlight rectangles, icon/image-like primitives, slider/input/control fills,
  scrollbars where currently retained, and debug overlay fills.
- Prove direct widget-tree rendering and retained display-list replay produce the
  same command classes for the same node/style inputs where both paths exist.
- Keep tests targeted and colocated with renderer/painter tests. Prefer existing
  `crates/core/frontend/render/src/surface/painter/tests.rs` and display-list
  fixtures before creating broad new harnesses.
- Run shipped navigation/audio regression tests or focused render proof commands
  that already exist; avoid requiring manual UAT for this phase unless automated
  evidence cannot prove a shipped surface path.

### Helper Retirement And Rollback
- Isolate or delete non-authoritative software helper code only after equivalent
  command-backed coverage exists. Do not remove helper wrappers that are still
  needed as narrow compatibility adapters.
- Keep `PaintBackend` capability/diagnostic behavior observable and non-fatal for
  unsupported command classes.
- Preserve a small rollback path: if a primitive cannot be migrated safely in
  Phase 53, document it as an explicit gap/deferred item rather than silently
  leaving an authoritative bypass.
- The planner may split work by primitive family to keep commits and verification
  small.

### the agent's Discretion
The planner may choose exact helper names and file placement. Prefer minimal
compile-safe slices that first add command recording/equivalence tests, then move
one primitive family at a time. If a direct path and retained replay path cannot
share production lowering yet, require tests that prove command-class parity and
document the remaining adapter boundary.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/frontend/render/src/surface/painter/backend.rs` defines
  `PainterCommand`, `PaintBackend`, `SkiaPaintBackend`, capability flags, and
  diagnostic behavior. Existing compatibility helpers already lower many helper
  calls into command slices.
- `crates/core/frontend/render/src/display_list.rs` defines
  `DisplayPaintCommand`, `DisplayPaintNode`, `DisplayPaintContent`,
  `DisplayPrimitiveSlot`, retained command spans, material hashes, and damage
  metrics. This remains the retained identity/order source.
- `crates/core/frontend/render/src/surface/painter/tree.rs` owns both direct
  widget-tree rendering (`render_node_self`) and retained replay
  (`render_display_node_self`).
- `crates/core/frontend/render/src/surface/painter/widgets.rs` owns direct and
  retained input, slider, icon, and scrollbar primitive paths.
- `crates/core/frontend/render/src/surface/painter/text.rs` owns text rendering
  and selection highlight rectangle drawing.
- `crates/core/frontend/render/src/surface/debug_overlay.rs` uses public fill
  helpers and should be included in the command-boundary inventory.
- Existing painter tests in
  `crates/core/frontend/render/src/surface/painter/tests.rs` already use
  recording backends and command assertions for painter helper behavior.

### Established Patterns
- Renderer changes should keep MESH-owned style/layout/animation/display-list/
  damage/presentation responsibilities outside the backend.
- Tests are colocated in Rust source under `#[cfg(test)]` and use descriptive
  behavior names.
- Phase 51 locked backend-neutral painter commands and Vello compatibility; no
  `skia_safe` types should enter display-list/style/render-object data.
- Phase 52 locked the bounded style profile and diagnostics; unsupported browser
  primitives should not be promoted during this phase.

### Integration Points
- Direct path: `render_tree_at_for_module*` -> `render_node_with_filter` ->
  `render_node_self`.
- Retained path: `render_display_list_for_module` -> `render_display_node_self`.
- Widget/control paths: `render_input_node`, `render_display_input_node`,
  `render_slider_node`, `render_display_slider_node`, `render_icon_node`,
  `render_display_icon_node`, and `render_display_scrollbars`.
- Text selection paths: `render_selection_highlights` and
  `render_display_selection_highlights`.
- Debug overlay path: `surface/debug_overlay.rs` fill helpers.

</code_context>

<specifics>
## Specific Ideas

The target proof should read like:

```text
WidgetNode + ComputedStyle
  -> DisplayPaintCommand / PainterCommand class stream
  -> direct render path and retained replay path emit equivalent primitive classes
  -> PaintBackend executes command stream
```

Command-class parity is more important than pixel-perfect proof in Phase 53.
Skia raster fidelity comes in Phase 54 and effects/layers in Phase 55.

</specifics>

<deferred>
## Deferred Ideas

- Skia authoritative rasterization, antialiasing, paths, rounded rects, strokes,
  and clipping details belong to Phase 54.
- Shadows, blur, images, gradients, layer/effect command semantics, and asset
  lifetime rules belong to Phase 55.
- Animation/transition invalidation and paint-only animation updates belong to
  Phase 56.
- Damage/visual-bounds expansion for effects and transforms belongs to Phase 57.
- Backend rollback/capability observability expansion belongs to Phase 58.
- Full shipped-surface proof and renderer documentation closure belong to Phase
  59.

</deferred>
