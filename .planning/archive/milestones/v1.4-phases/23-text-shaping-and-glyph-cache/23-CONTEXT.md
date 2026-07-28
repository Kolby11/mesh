# Phase 23: Text Shaping and Glyph Cache - Context

**Gathered:** 2026-05-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Stop unchanged text from reshaping or rerasterizing during unrelated updates while preserving correct invalidation for text-specific changes. This phase should reuse shaped text layout data and existing glyph cache behavior for stable text inputs, then expose text cache hit/miss/invalidation counters through the established debug profiling snapshot path.

</domain>

<decisions>
## Implementation Decisions

### Cache Ownership and Keys
- Shaped text cache lives in `mesh-core-render` near `surface/text.rs` and glyph rendering code.
- Shape cache keys include stable text inputs: content, font family, font size, font weight, line height, wrapping width, text alignment, and effective scale through the already-scaled font size and width.
- Cache shaped/measured text layout data and reuse existing glyph raster hooks without rewriting all text painting.
- Reshape and rerasterize on uncertain input changes.

### Invalidation Rules
- Content, font, size, weight, line height, wrapping width, selection/highlight, and scale factor affect cache behavior.
- Hover, unrelated state, and layout updates reuse text cache when text inputs are unchanged.
- Selection/highlight changes may repaint overlays, but reusable shaping survives when text metrics are unchanged.
- Correctness proof uses focused unit tests plus debug metric proof.

### Metrics and Scope
- Debug exposes text cache hits, misses, invalidations, shaped entries, and glyph-cache availability/reuse signals where available.
- Benchmark proof uses existing profiling/debug snapshot paths and canonical benchmark scenarios.
- Full font fallback, GPU glyph atlas work, and bidi/complex shaping expansion beyond current renderer behavior remain out of scope.
- Keep implementation minimal around current `TextRenderer`, `SharedTextMeasurer`, and text painter.

### the agent's Discretion
The agent may choose exact counter names and cache capacity, provided metrics remain honest and text rendering behavior stays visually unchanged.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/frontend/render/src/surface/text.rs` owns `TextRenderer`, `SharedTextMeasurer`, `FontSystem`, and `SwashCache`.
- `crates/core/frontend/render/src/surface/glyph.rs` already caches icon-font glyph rasters.
- Phase 22 added `ProfilingInvalidationSnapshot.paint`; Phase 23 can extend the same invalidation snapshot structure for text cache metrics.

### Established Patterns
- Rendering tests are colocated in render modules.
- Debug/profiling data flows through `mesh_core_debug` types, shell profiling state, and debug JSON serialization.
- Current shell tests should run under `nix develop` because Wayland dependencies require `xkbcommon.pc`.

### Integration Points
- Cache `cosmic_text::Buffer` layout data inside `TextRenderer`.
- Expose per-render text cache metrics from `FrontendRenderEngine` through the existing paint call.
- Record text cache metrics in `FrontendSurfaceComponent` invalidation snapshots.

</code_context>

<specifics>
## Specific Ideas

Use a small bounded text layout cache keyed by stable text shaping inputs, reuse it for measurement, drawing, and selection geometry, and report hits/misses without changing the software paint output.

</specifics>

<deferred>
## Deferred Ideas

Full font fallback, GPU glyph atlas, broader text shaping behavior changes, and a new standalone benchmark harness remain deferred.

</deferred>
