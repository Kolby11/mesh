# Phase 21: Retained Render-Object Scene Graph - Context

**Gathered:** 2026-05-09
**Status:** Ready for planning
**Mode:** Auto-generated (infrastructure phase)

<domain>
## Phase Boundary

Add a retained render-object tree keyed by stable widget identities so widget changes can synchronize into render-facing slots instead of treating paint data as an unstructured full rebuild.

</domain>

<decisions>
## Implementation Decisions

### the agent's Discretion
- Keep the software painter output unchanged in this phase.
- Introduce the render-object tree as a render crate data structure, then synchronize it from shell component paint finalization.
- Separate transform, clip, opacity, geometry, material, text, and accessibility-facing slots so later phases can update display lists and damage regions independently.
- Keep ownership single-threaded and mutation explicit; future render-thread/GPU work can consume immutable snapshots later.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- Stable `WidgetNode::id` values are assigned during runtime tree annotation.
- `FrontendSurfaceComponent` already updates retained widget-tree state during paint.
- The render crate owns painter-facing concepts and already depends on `mesh-core-elements`.

### Established Patterns
- Retained structures report a dirty summary and generation count.
- Focused render crate unit tests exercise data-model changes without requiring Wayland.

### Integration Points
- `mesh_core_render::RenderObjectTree` is owned by `FrontendSurfaceComponent`.
- Shell paint updates render objects after style animation application and before software paint.

</code_context>

<specifics>
## Specific Ideas

No visual behavior change. The scene graph is a synchronization boundary and dirty-slot classifier for later retained display-list and damage work.

</specifics>

<deferred>
## Deferred Ideas

- Painting from render objects is deferred to retained display-list work.
- GPU resource ownership and render-thread handoff remain future milestones.

</deferred>
