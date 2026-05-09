# Phase 24: Typed Slots, Interning, and Selector Indexing - Context

**Gathered:** 2026-05-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Reduce hot-path lookup and restyle candidate cost by adding typed selector/style lookup helpers, bounded string interning for repeated selector inputs, and selector indexes that preserve fallback correctness. Existing string attribute maps remain source-compatible.

</domain>

<decisions>
## Implementation Decisions

### Typed Hot Data
- Prioritize high-traffic style and attribute lookups already visible in render/restyle hot paths.
- Typed slots live in `mesh-core-elements` style/runtime node data with compatibility accessors for existing string attributes.
- Add typed fast paths while preserving current string maps as source-compatible fallback.
- Prove value with unit tests and profiling/debug counters where practical.

### Interning
- Intern repeated style property names, class/id/key strings, attribute names, and selector components where they occur on hot paths.
- Keep interning local to UI/style code rather than adding a global shell service.
- Intern compile/style-resolution hot-path strings only; do not intern every runtime string.
- Raw strings remain accepted and comparable.

### Selector Indexing
- Index class, id/key, pseudo-state, attribute, and simple structural selector triggers first.
- Build indexes alongside style rule caching/resolution so retained restyles can ask for candidate keys.
- Use indexes only when selector dependencies are known; otherwise fall back to full restyle.
- Full browser selector-engine rewrite and speculative parallel restyle remain out of scope.

### the agent's Discretion
The agent may choose the exact index representation and typed helper names as long as current selector behavior remains unchanged and fallback behavior stays explicit.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/ui/elements/src/style/resolve.rs` owns selector matching, style resolution, and retained restyle helpers.
- `WidgetNode.attributes` remains the compatibility source for class, id, `_mesh_key`, and `_mesh_module_id`.
- `FrontendSurfaceComponent` already caches module restyle rules and uses targeted key restyles for interaction changes.

### Established Patterns
- Style resolver APIs accept existing `StyleRule` slices and raw strings.
- Focused unit tests for style resolution live in `crates/core/ui/elements/src/style.rs`.
- Keep public behavior stable and add helper APIs instead of replacing the selector engine.

### Integration Points
- Add typed selector attribute extraction in `mesh-core-elements`.
- Add a selector rule index that filters rule candidates by tag/class/id/state before falling back to full rule scans.
- Use the index in retained restyle paths where the node's typed attributes are known.

</code_context>

<specifics>
## Specific Ideas

Start with typed selector matching and indexed rule lookup for existing simple selectors. Preserve full-scan fallback for unsupported or unknown selector dependencies.

</specifics>

<deferred>
## Deferred Ideas

Full browser selector engine rewrite, interned-only storage, broad attribute-map replacement, and parallel restyle are deferred.

</deferred>
