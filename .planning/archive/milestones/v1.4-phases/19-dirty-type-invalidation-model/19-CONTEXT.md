# Phase 19: Dirty-Type Invalidation Model - Context

**Gathered:** 2026-05-09
**Status:** Ready for planning
**Mode:** Auto-generated (infrastructure phase)

<domain>
## Phase Boundary

Turn the retained widget-tree dirty summaries into typed invalidation data that downstream style, layout, paint, text, accessibility, metrics, and surface configuration systems can consume without treating every mutation as a full render rebuild.

</domain>

<decisions>
## Implementation Decisions

### the agent's Discretion
- Keep the existing full widget-tree rebuild path as the correctness fallback for script and text mutations.
- Preserve the retained style path for interaction and surface/layout invalidations when a previous tree exists.
- Expose typed invalidation counts through the existing debug profiling snapshot instead of adding a separate trace channel.
- Keep Phase 19 scoped to classification, routing, and observability; actual subtree layout reuse, damage, and render-object synchronization belong to later phases.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `FrontendSurfaceComponent` already tracks `ComponentDirtyFlags` for script, state, style, layout, paint, text, accessibility, metrics, and surface configuration.
- `RetainedWidgetTree` already compares stable runtime node snapshots and reports inserted, removed, layout, style, attributes, children, and state dirty categories.
- `ProfilingRuntimeState` already owns per-surface debug profiling summaries and serializes them through the `mesh.debug` service payload.

### Established Patterns
- Component hot paths record profiling through `ComponentProfilingRecord`, then shell runtime rolls records into per-surface accumulators.
- Debug payloads are plain serde JSON built in `shell/runtime/debug.rs`.
- Focused tests live beside shell/component/runtime code with behavior-oriented names.

### Integration Points
- Phase 19 connects `FrontendSurfaceComponent::paint` dirty decisions to `ProfilingRuntimeState`.
- The `ShellComponent` host trait is the narrow boundary for handing component invalidation snapshots back to the shell runtime.

</code_context>

<specifics>
## Specific Ideas

No product-facing behavior changes. Preserve shipped surface visuals and use the debug inspector/profiling payload as the observable proof channel.

</specifics>

<deferred>
## Deferred Ideas

- Previous/next bounds for damage regions are completed in Phase 22.
- Scoped subtree restyle/layout reuse is completed in Phase 20.
- Retained render-object slot updates are completed in Phase 21.

</deferred>
