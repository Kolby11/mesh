# Phase 22: Retained Display List and Damage Tracking - Context

**Gathered:** 2026-05-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Retain paint commands across frames from the Phase 21 render-object tree and compute surface damage so unchanged regions can avoid unnecessary paint or present work where the current backend can support it. The phase must expose retained display-list reuse, dirty render-object effects, damage area, skipped paint, and partial-present support through existing debug profiling snapshots.

</domain>

<decisions>
## Implementation Decisions

### Display List Ownership
- Retained display-list data lives in `mesh-core-render`, derived from retained render objects, so the painter remains the owner of paint commands.
- Display-list entries are keyed by stable render-object node identity plus primitive slot/type, matching Phase 21 retained render object keys.
- Full repaint remains the fallback when node identity, ordering, unsupported primitive handling, or backend partial-present support is uncertain.
- Add narrow display-list and damage types near the current painter modules, then integrate minimally instead of rewriting the renderer.

### Damage and Partial Present
- Ship rectangle damage regions unioned and clipped per surface, with old and new bounds for dirty render objects.
- Paint, layout, transform, clip, text, visibility, insertion, and removal invalidations all contribute to damage; unknowns force full-surface damage.
- Compute and expose damage on all runs; skip or clip paint only where supported, otherwise present the full buffer and report that partial present was unavailable.
- Correctness proof must keep visual output unchanged and add targeted tests for damage unioning, clipping, and fallback behavior.

### Debug Metrics and Benchmark Proof
- Extend existing debug profiling surface snapshots with display-list reuse, damage area, skipped paint, and partial-present support.
- Use existing canonical v1.3 benchmark scenarios with before/after counters where automated runs are possible.
- Report computed optimization opportunities separately from actually skipped work so metrics do not imply backend support that does not exist.
- Keep GPU backend work, parallel paint/layout, and broad painter redesign out of scope.

### the agent's Discretion
The agent may choose exact Rust type names, module boundaries, and test fixture shapes as long as the implementation stays narrow, preserves current rendering behavior, and follows existing codebase conventions.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/frontend/render/src/render_object.rs` already models retained render-object nodes from Phase 21 and should be the source for retained paint data.
- `crates/core/frontend/render/src/surface/painter.rs` and `surface/painter/*` own software paint behavior and are the right integration point for retained paint commands.
- `crates/core/foundation/debug/src/lib.rs`, `crates/core/shell/src/shell/runtime/profiling.rs`, and `crates/core/shell/src/shell/runtime/debug.rs` already carry profiling/debug snapshot data.

### Established Patterns
- Rust modules use focused domain structs and colocated unit tests.
- Rendering changes should preserve existing software-painter output and add incremental metadata beside the existing full repaint path.
- Debug/profiling state is exposed through existing surface profiling snapshots rather than a separate channel.

### Integration Points
- Build retained display-list and damage primitives inside `mesh-core-render`.
- Thread retained-rendering metrics through shell runtime profiling snapshots.
- Keep presentation backend behavior conservative when partial present is unavailable.

</code_context>

<specifics>
## Specific Ideas

Use the accepted smart-discuss defaults: narrow display-list ownership in the render crate, rectangle damage unioning/clipping, full-surface fallback for unsupported cases, and honest debug counters that separate computed opportunities from actual skipped work.

</specifics>

<deferred>
## Deferred Ideas

GPU backend implementation, parallel paint/layout, tile-based damage tracking, per-primitive damage graphs, and broad renderer rewrites remain out of scope.

</deferred>
