# Phase 51: Painter Contract And Backend Boundary - Context

**Gathered:** 2026-05-21
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 51 defines the painter contract below the retained display list and above concrete painter backends. It should lock the command model, backend obligations, capability/error behavior, Vello compatibility expectations, and migration map from existing helper calls. It should not migrate all primitives yet; Phase 52 and later perform the Skia implementation refactor.

</domain>

<decisions>
## Implementation Decisions

### Command Model

- **D-01:** Use a backend-neutral command model, not Skia-shaped method names. The contract should describe high-level paint operations such as push clip, pop clip, push layer, pop layer, draw rect, draw rounded rect, draw path, draw text, draw image, draw shadow, apply filter, and optional state commands.
- **D-02:** Retained display-list data must not expose Skia-only types. Skia-specific conversion belongs inside the Skia backend adapter.
- **D-03:** The command model should support both widget-tree direct painting and retained display-list replay, but Phase 51 may define the migration route before Phase 52 fully converts both paths.

### Backend Capabilities

- **D-04:** Backends must publish capability/unsupported-feature behavior. Unsupported commands should be explicit through diagnostics, capability records, or controlled fallback paths; silent incorrect rendering is not acceptable.
- **D-05:** Backend selection should remain observable and reversible. The planner should preserve a narrow rollback route while the Skia path is hardened.
- **D-06:** The initial authoritative backend is Skia. Vello is a compatibility target for API shape, not a production backend in this phase.

### MESH Ownership Boundary

- **D-07:** MESH retains ownership of widget traversal, style resolution, layout, animation state, retained display-list ordering, damage selection, z-order, module boundaries, input handling, and presentation.
- **D-08:** Skia owns low-level paint/raster work below the backend boundary: rasterization, antialiasing, paths, rounded rects, strokes, clipping, blend modes, shadows, blur/image filters, saveLayer/layers, gradients/images, and future text primitives where they fit.
- **D-09:** Phase 51 should produce a migration map from current helper APIs (`fill_rect_clipped`, `fill_rounded_rect_clipped`, `stroke_rounded_rect_clipped`, `draw_box_shadow`, `apply_backdrop_filter`, selection/debug fills, widget primitive drawing) to the new painter command model.

### Vello Compatibility

- **D-10:** Vello compatibility should be captured as a design constraint and compatibility notes, not as production code. The API should avoid assumptions that only Skia can satisfy, while still allowing Skia to use its full canvas, paint, image filter, and saveLayer model internally.
- **D-11:** The contract may define capability-gated command subsets so Vello can later implement a supported subset with explicit diagnostics for unsupported features.

### the agent's Discretion

The planner may choose exact Rust type names, file placement, and incremental refactor order, provided the public contract remains backend-neutral and Skia-specific conversion stays behind the backend adapter. The planner should prefer small compile-safe steps over a single broad painter rewrite.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone Scope

- `.planning/ROADMAP.md` — Phase 51 goal, requirement mapping, and success criteria.
- `.planning/REQUIREMENTS.md` — PAINT-01, PAINT-02, BACKEND-01, BACKEND-02 definitions and v1.10 out-of-scope boundaries.
- `.planning/PROJECT.md` — Current milestone framing and key decisions: Skia is the paint backend, not the render engine; the painter API must remain backend-neutral enough for Vello.

### Renderer Architecture Docs

- `crates/core/frontend/render/README.md` — Current WebEngine/Qt-style split and desired long-term paint command shape.
- `docs/renderer-ownership.md` — Authoritative ownership boundaries for retained display list, render engine, Skia backend, presentation, and Vello as a replacement candidate.
- `docs/renderer-migration.md` — Migration principles, Skia-centric paint boundary, reversibility, and observability gates.

### Current Painter Code

- `crates/core/frontend/render/src/surface/painter/backend.rs` — Current first-pass `PaintBackend` and `SkiaPaintBackend` implementation. This is the main migration input and currently still exposes helper-shaped operations.
- `crates/core/frontend/render/src/surface/painter.rs` — `FrontendRenderEngine` backend ownership and helper methods that call the current backend.
- `crates/core/frontend/render/src/surface/painter/tree.rs` — Widget-tree and retained display-list paint traversal, border drawing, and effect calls that must eventually emit painter commands.
- `crates/core/frontend/render/src/surface/painter/widgets.rs` — Slider/input/icon/control drawing paths using current helper methods.
- `crates/core/frontend/render/src/surface/painter/text.rs` — Selection and text paint integration points that must remain compatible with the painter boundary.
- `crates/core/frontend/render/src/display_list.rs` — Retained display-list ownership, paint command ordering, damage selection, visual bounds, and batching evidence.
- `crates/core/frontend/render/src/render_object.rs` — Retained render-object synchronization and material hashing that must remain MESH-owned.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- `PaintBackend` / `SkiaPaintBackend` in `crates/core/frontend/render/src/surface/painter/backend.rs`: Existing seam to evolve into the fuller painter API.
- `DisplayPaintCommand`, `DisplayPaintNode`, `RetainedDisplayList` in `crates/core/frontend/render/src/display_list.rs`: Existing retained command ordering and damage-selection source. The new painter command stream should sit below this, not replace it.
- `FrontendRenderEngine` in `crates/core/frontend/render/src/surface/painter.rs`: Existing ownership point for the active backend and text renderer.
- Painter tests in `crates/core/frontend/render/src/surface/painter/tests.rs`: Existing coverage for pluggable backend construction, opacity, borders, clipping, shadows, blur, and retained display-list replay.

### Established Patterns

- Renderer changes need Nix-backed verification when linking/render tests need native graphics/font libraries.
- Existing retained display-list and render-object code owns identity, material, visual bounds, damage metrics, and repaint policies; backend work should not absorb these responsibilities.
- Documentation and tests should accompany ownership-boundary changes because renderer migration uses explicit rollback, observability, and author-contract gates.
- Current backend trait is too helper-shaped: `fill_rect`, `fill_rounded_rect`, `stroke_rounded_rect`, `draw_box_shadow`, and `apply_backdrop_filter` encode call sites rather than a durable command model.

### Integration Points

- Direct widget-tree rendering: `render_node_with_filter` -> `render_node_self` in `painter/tree.rs`.
- Retained display-list replay: `render_display_list_for_module` -> `render_display_node_self` in `painter/tree.rs`.
- Control primitives: `render_slider_node`, `render_display_slider_node`, input caret drawing, and widget-specific fill calls in `painter/widgets.rs`.
- Text and selection: `TextRenderer` remains separate for now; selection highlight rectangles should route through the painter API without breaking theme-owned selection behavior.
- Debug overlay: `surface/debug_overlay.rs` uses the public `fill_rect_clipped` compatibility helper and needs either a painter command route or a consciously isolated debug-only compatibility path.

</code_context>

<specifics>
## Specific Ideas

The target architecture should mirror the user's WebEngine/Qt-style split:

```text
WidgetNode tree
  -> computed style + layout
  -> retained render/display list
  -> layer/effect model
  -> PaintBackend trait
  -> Skia backend now
  -> Vello backend later
```

The first command contract should make it possible to express:

```text
PushClip
PushLayer
DrawRect
DrawRoundedRect
DrawPath
DrawText
DrawImage
DrawShadow
ApplyFilter
PopLayer
PopClip
```

</specifics>

<deferred>
## Deferred Ideas

- Full Vello production backend implementation — later milestone after the Skia implementation proves the contract.
- Broad animation/motion-fidelity redesign — separate milestone unless required to preserve current animation behavior through the painter boundary.
- Full GPU compositor replacement — out of scope; presentation remains `mesh-core-presentation`.

### Reviewed Todos (not folded)

- `2026-05-13-phase31-audio-popover-transition-delay.md` — Matched only by broad render keywords. This is animation/polish work, not Phase 51 painter contract scope.
- `2026-05-15-define-module-install-requirement-resolution.md` — Matched only by broad architecture keywords. This belongs to module install/graph resolution, not painter backend design.

</deferred>

---

*Phase: 51-Painter Contract And Backend Boundary*
*Context gathered: 2026-05-21*
