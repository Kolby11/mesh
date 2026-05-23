# Phase 57: Stacking, Clipping, Visual Bounds, And Damage - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 57 preserves retained rendering correctness after the painter subset gained effects, layers, images, gradients, and animation. The phase owns display-list ordering, clipping, visual-bounds expansion, damage selection, partial-present fallback behavior, and profiling counters for those decisions. It does not move layout, style resolution, animation scheduling, input, module, presentation, or backend raster ownership into Skia.

</domain>

<decisions>
## Implementation Decisions

### Ordering And Clipping
- Keep MESH-owned traversal authoritative: z-index sorting happens before backend execution in the retained display-list and software painter paths.
- Preserve existing DOM/source-order behavior for equal z-index children by using stable child index order.
- Treat overflow clipping as a MESH display-list clip decision before command replay; backend clips execute only the already bounded command stream.
- Add tests around overlapping z-index children and nested clipped descendants rather than relying on shipped-surface proof alone.

### Visual Bounds And Damage
- Use painter visual bounds, not layout bounds, for effect overflow damage and sparse replay selection.
- Include shadows, filters, transforms, images, gradients, layers, and animated paint changes in damage accounting where the retained data already exposes them.
- Promote to full-surface repaint only for deterministic broad-damage or ambiguous-dirty cases; expose the promotion through counters.
- Keep pathological expansion conservative: clamp to the surface before presentation and distinguish fallback promotion from normal partial repaint.

### Diagnostics And Profiling
- Extend retained paint profiling with counters that distinguish changed layout, changed paint, effect overflow, and fallback promotion.
- Reuse existing debug/profiling JSON payloads instead of adding a new diagnostics channel.
- Keep counter names backend-neutral and tied to retained display-list decisions, not Skia-specific execution.
- Preserve existing inspector consumers by only adding fields.

### the agent's Discretion
The agent may choose exact helper names and test fixture shape, provided the changes remain in the retained display-list/debug payload path and keep backend-neutral data structures.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/frontend/render/src/display_list.rs` owns retained display-list entries, command traversal, z-index sorting, visual clipping, sparse replay selection, and damage metrics.
- `RenderObjectDirtySummary` in `crates/core/frontend/render/src/render_object.rs` already separates geometry, material, primitive, text, transform, clip, opacity, reordered, inserted, and removed changes.
- `mesh_core_debug::RetainedPaintSnapshot` and `crates/core/shell/src/shell/runtime/debug.rs` already publish retained paint metrics through the debug/profiling JSON payload.

### Established Patterns
- Display-list tests live inline in `display_list.rs` under `#[cfg(test)]`.
- Focused verification uses `nix develop -c cargo test -p <crate> <selector>` because workspace tests require Nix system libraries.
- Debug/profiling changes add fields to the snapshot struct, wire them through shell conversion, then serialize them in `runtime/debug.rs`.

### Integration Points
- `retained_paint_snapshot()` in `crates/core/shell/src/shell/component.rs` converts render metrics into debug snapshots.
- Shell debug JSON serialization in `crates/core/shell/src/shell/runtime/debug.rs` exposes profiling counters to inspector consumers.
- Phase 59 shipped-surface proof can consume the added counters without needing new runtime APIs.

</code_context>

<specifics>
## Specific Ideas

No user-specific overrides were provided for this phase. Use the roadmap success criteria and established retained display-list conventions.

</specifics>

<deferred>
## Deferred Ideas

Full compositor replacement, full browser stacking contexts, and Vello production backend behavior remain out of scope for Phase 57.

</deferred>
