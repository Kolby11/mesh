# Phase 20: Incremental Style and Layout Propagation - Context

**Gathered:** 2026-05-09
**Status:** Ready for planning
**Mode:** Auto-generated (infrastructure phase)

<domain>
## Phase Boundary

Reuse retained widget and layout data for unaffected subtrees while restyling and laying out only nodes whose dirty types require it.

</domain>

<decisions>
## Implementation Decisions

### the agent's Discretion
- Start with interaction-state restyle targeting because hover/focus changes are the existing retained-path hot case.
- Recompute targeted nodes from the full rule set so previous hover/focus styles are removed correctly when a node leaves the active state.
- Keep non-interaction style/layout changes on the conservative full restyle/layout path until retained layout dependency boundaries are explicit.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `RetainedWidgetTree` provides stable runtime node identity.
- `StyleResolver::resolve_node_style_for_module` can recompute a single node from the full style rule set.
- `FrontendSurfaceComponent::finalize_tree` already knows whether the current render is a rebuild or retained restyle path.

### Established Patterns
- Interaction restyles use retained `last_tree` and avoid Luau tree rebuilds.
- Layout still runs through `LayoutEngine::compute_with_measurer` over the full tree.

### Integration Points
- `StyleResolver` now offers keyed subtree restyle for targeted nodes.
- `FrontendSurfaceComponent` builds target keys from previous and current stateful nodes before layout.

</code_context>

<specifics>
## Specific Ideas

First incremental slice targets hover/focus/active/checked interaction-state restyles. Broader layout reuse is intentionally not generalized until layout dependency classes are available.

</specifics>

<deferred>
## Deferred Ideas

- Local vs ancestor/surface-wide layout invalidation still needs a dedicated dependency model.
- Retained layout output reuse for unaffected subtrees remains open.

</deferred>
