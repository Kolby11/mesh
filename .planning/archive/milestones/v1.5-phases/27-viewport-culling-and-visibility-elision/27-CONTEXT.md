# Phase 27: Viewport Culling and Visibility Elision - Context

**Gathered:** 2026-05-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Reduce CPU paint work on the existing retained software-render path by omitting work that is provably outside explicit viewport or clip boundaries, or explicitly hidden, before paint execution runs. This phase clarifies when existing retained paint work may be skipped; it does not introduce a new renderer architecture, global occlusion system, GPU work, or broad retained-command ownership changes.

</domain>

<decisions>
## Implementation Decisions

### Visibility Semantics
- **D-01:** Follow Qt Quick style conservative visibility semantics for this phase. Pruning is driven by explicit hidden state and explicit viewport or clip relationships, not by a global smart visibility heuristic.
- **D-02:** Do not treat `opacity: 0` as a general hidden-state shortcut in Phase 27. The phase should stay aligned with explicit visibility semantics rather than introduce broad opacity-based omission rules.

### Viewport Authority
- **D-03:** Explicit clip and scroll viewport boundaries are the authority for pruning decisions in this phase.
- **D-04:** Allow cheap root-surface omission only when a subtree is trivially and provably outside the root surface bounds, but do not build a global CPU-side occlusion or viewport-culling pass.

### Elision Granularity
- **D-05:** Keep Phase 27 scoped to paint-time omission first. Do not pull retained display-list ownership or render-object synchronization pruning forward from later phases.
- **D-06:** Within the Qt-like model, use whole-subtree omission inside explicit clip or scroll viewports, plus localized rough pre-clipping for viewport-aware content.
- **D-07:** Do not introduce broad command-level smart filtering across partially visible subtrees in this phase; that belongs closer to later retained-command and damage-indexed work.

### Debug and Proof
- **D-08:** Reuse the existing profiling and debug pipeline from Phase 26 instead of creating a second diagnostics system.
- **D-09:** Debug proof for Phase 27 should expose aggregate counters only, not per-node or per-subtree trace detail.

### the agent's Discretion
- The planner and researcher may decide the exact API and data-flow seam for viewport-aware pruning as long as it stays inside the existing retained render-object, retained display-list, and filtered paint-node architecture.
- The planner and researcher may choose the exact aggregate counter names and placement in the existing debug payload, provided they remain lightweight and clearly attributable to Phase 27 pruning work.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Scope and Requirements
- `.planning/ROADMAP.md` — Phase 27 goal, planned work, and milestone boundaries for viewport culling and visibility elision.
- `.planning/REQUIREMENTS.md` — active requirements `CULL-01`, `CULL-02`, and `CULL-04`, plus milestone out-of-scope constraints.
- `.planning/PROJECT.md` — milestone-level decisions: CPU smoothness before GPU work, Qt-inspired retained-rendering direction, and real-surface smoothness as the acceptance priority.

### Prior Phase Context
- `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md` — baseline proof showing paint traversal as the dominant current hotspot and shipped scenarios still hitting full rebuild paths.
- `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-SUMMARY.md` — Phase 26 profiling extension and proof flow that Phase 27 must reuse.
- `.planning/milestones/v1.4-phases/25-display-list-batching-and-gpu-readiness-guardrails/25-CONTEXT.md` — retained display-list ownership, debug metrics, and software-painter-authoritative constraints.
- `.planning/milestones/v1.4-phases/24-typed-slots-interning-and-selector-indexing/24-CONTEXT.md` — style/runtime-node compatibility constraints and guidance to extend existing hot-path helpers rather than rewrite systems.
- `.planning/milestones/v1.4-phases/23-text-shaping-and-glyph-cache/23-CONTEXT.md` — precedent for keeping optimizations local to `mesh-core-render` and proving them through focused tests and existing debug metrics.

### Renderer and Integration Seams
- `crates/core/frontend/render/src/render_object.rs` — retained render-object dirty categories already include `clip` and `opacity`; phase work should respect this boundary.
- `crates/core/frontend/render/src/display_list.rs` — retained display-list collection, clip propagation, paint-command generation, and aggregate metrics.
- `crates/core/frontend/render/src/surface/mod.rs` — existing clipped and filtered paint entrypoints for tree and display-list rendering.
- `crates/core/frontend/render/src/surface/painter/tree.rs` — current paint-time filter seam (`paint_nodes`) and clip-aware traversal behavior.
- `crates/core/shell/src/shell/component/rendering.rs` — shell/component retained-tree finalize flow and the boundary between runtime tree preparation and render/painters.
- `crates/core/frontend/render/README.md` — renderer crate ownership boundary; keep render-specific work in `mesh-core-render`.

### External Design Reference
- `https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph-renderer.html` — Qt Quick default renderer guidance used to lock the phase toward explicit visibility state and explicit viewport-aware rough pre-clipping, while avoiding a broad global CPU-side occlusion pass.
- `https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html` — Qt Quick scene graph overview reinforcing retained-tree / scene-graph style rendering assumptions.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/frontend/render/src/surface/mod.rs`: already exposes clipped and `paint_nodes`-filtered paint entrypoints for both widget-tree and retained display-list rendering.
- `crates/core/frontend/render/src/surface/painter/tree.rs`: already skips traversal for filtered-out node IDs and applies clip intersection per command and per node.
- `crates/core/frontend/render/src/display_list.rs`: already propagates clip rectangles and scroll offsets while collecting retained paint commands and aggregate display-list metrics.
- `crates/core/frontend/render/src/render_object.rs`: already tracks retained dirty summaries for clip and opacity changes, giving a stable place to align new pruning-related invalidation semantics.

### Established Patterns
- Render-path optimizations stay inside `mesh-core-render`; shell/runtime code should provide state and profiling hooks, not duplicate renderer behavior elsewhere.
- New performance proof should extend the existing `mesh.debug.profiling` and invalidation/debug JSON path rather than invent a second benchmark or trace channel.
- MESH prefers incremental, narrowly-scoped fast paths layered onto compatibility-preserving fallbacks rather than rewrites of core retained-rendering systems.

### Integration Points
- Phase 27 likely connects at the retained display-list update and paint-execution boundary, not at component compilation or backend/runtime layers.
- Scroll and clip authority already flows from annotated runtime-tree data into render/display-list code via overflow styles, scroll offsets, and clip intersections.
- Aggregate proof should surface through the same debug snapshot and profiling payload chain used in Phase 26.

</code_context>

<specifics>
## Specific Ideas

- The user explicitly wants Phase 27 to behave like Qt Quick’s renderer model rather than a generic custom visibility engine.
- “Qt-like” here means: explicit hidden state and explicit clip/viewport relationships drive pruning; no global smart occlusion/culling pass; localized rough pre-clipping is acceptable where a node is already viewport-aware.
- Paint traversal is the primary hotspot to attack first, but the user preferred to keep this phase scoped to paint-time omission rather than pulling retained-command rebuild ownership into the phase.

</specifics>

<deferred>
## Deferred Ideas

- Unified package/module manifest planning remains a separate backlog item and was not folded into Phase 27 because it is unrelated to retained rendering or viewport culling.
- Broad command-level selective execution across partially visible subtrees is deferred to later retained paint-command and damage-indexed phases.
- Any global opacity-based hidden heuristic, global occlusion system, or second diagnostics/trace system remains out of scope for this phase.

</deferred>

---

*Phase: 27-Viewport Culling and Visibility Elision*
*Context gathered: 2026-05-11*
