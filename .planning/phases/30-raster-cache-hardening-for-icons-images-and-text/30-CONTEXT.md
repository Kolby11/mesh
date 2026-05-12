# Phase 30: Raster Cache Hardening for Icons, Images, and Text - Context

**Gathered:** 2026-05-12
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 30 removes repeat rasterization, resize, parse, and shaping cost from steady-state CPU painting. It covers retained raster caches for SVG icons, bitmap icons/images, and text/glyph reuse where profiling justifies it. It also carries opaque/translucent metadata far enough for the software painter to avoid unnecessary blending or redundant background work. Phase 31 still owns threshold tuning and visible-smoothness acceptance.

</domain>

<decisions>
## Implementation Decisions

### Cache Boundaries and Keys
- Cache rasterized icon/image output variants first, because `surface/icon.rs` currently caches decoded bitmap source images but still resizes bitmap targets, parses/rasterizes SVG targets, and rasterizes the built-in missing icon on repeat paints.
- Cache ownership stays in `mesh-core-render` surface/resource code, close to paint execution and existing `icon_image_raster_micros` profiling.
- Cache keys include source identity plus rendered dimensions, tint or multicolor mode, icon axes when applicable, and conservative asset freshness metadata where available.
- Cache invalidation is driven by explicit visual input changes plus conservative file metadata changes. Theme changes naturally miss through tint/key changes.

### Correctness, Metrics, and Fallbacks
- Cache hits must be visually identical to current uncached rendering. Unsupported, missing, or failed assets keep the existing fallback chain.
- Missing or unsupported resource states should not poison successful future lookups unless the key includes enough freshness/source identity to prove the miss remains valid.
- Cache misses, hits, and fallback behavior should remain observable through existing raster/text profiling payloads rather than a new diagnostics surface.
- Opaque/translucent metadata must be conservative: unknown or mixed alpha is treated as translucent; only proven fully opaque resources can opt into blend/background elision.

### Scope and Sequencing
- Phase 30 focuses on renderer-owned cache hardening and proof for `CACHE-01`, `CACHE-02`, and `CACHE-03`.
- Basic bounded capacity is in scope if needed for safety, but sophisticated cache-size tuning and eviction policy optimization are deferred to Phase 31.
- Visible-smoothness acceptance remains Phase 31. Phase 30 records deterministic cache proof and benchmark evidence through the existing canonical path.
- Text/glyph cache work stays in scope, but the first priority is preserving and extending existing `TextRenderer` and glyph cache reuse rather than replacing text layout architecture.

### the agent's Discretion
The agent may choose the exact cache structs, bounded-capacity strategy, and test decomposition, provided the plan preserves visual correctness, keeps profiling in existing payloads, and avoids new benchmark or diagnostics systems.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/frontend/render/src/surface/icon.rs` owns file icon/image drawing, decoded bitmap source caching, SVG rasterization, built-in missing icon drawing, and icon/image raster profiling.
- `crates/core/frontend/render/src/surface/glyph.rs` already caches font bytes and font glyph rasters by font path hash, codepoint, size, tint, and supported variable-font axes.
- `crates/core/frontend/render/src/surface/text.rs` already caches cosmic-text layout buffers with bounded capacity and exposes layout hit/miss/invalidation metrics.
- `crates/core/frontend/render/src/surface/profiling.rs` and `PaintProfilingMetrics` already carry icon/image raster timing and text cache metrics into shell profiling.
- `crates/core/shell/src/shell/component/shell_component.rs` already publishes retained paint/text/raster proof into existing debug profiling state.

### Established Patterns
- Render caches live in renderer-local or module-local statics guarded by `Mutex`/`OnceLock`, with focused unit tests that clear cache state when needed.
- Phase 29 kept proof under existing debug/profiling payloads, and Phase 30 should follow that pattern.
- Cache behavior is tested with deterministic unit/integration selectors such as `cargo test -p mesh-core-render icon`, `glyph`, `text`, and shell profiling selectors.

### Integration Points
- Icon and image cache work connects through `draw_icon_from_path_with_options`, `draw_missing_icon_fallback`, and `draw_icon_resolution_with_axes`.
- Text/glyph cache proof connects through `TextRenderer::cache_metrics`, `draw_font_glyph`, and `PaintProfilingMetrics`.
- Opaque/translucent metadata likely connects through cached raster metadata and display-list/painter decisions around blending, barriers, and background work.

</code_context>

<specifics>
## Specific Ideas

Use the accepted conservative cache policy:

- Renderer-owned raster variant caches keyed by visual inputs.
- Conservative file metadata invalidation.
- Existing profiling payloads for proof.
- Phase 31 owns tuning and visible-smoothness acceptance.

</specifics>

<deferred>
## Deferred Ideas

- Sophisticated cache-size tuning and eviction heuristics.
- New cache diagnostics or trace persistence.
- Milestone-level visible-smoothness acceptance and repaint/cache threshold tuning.
- GPU-backed raster/resource caches.

</deferred>
