# Phase 77: Component Definitions And Binding - Context

**Gathered:** 2026-05-26
**Status:** Ready for planning
**Mode:** Autonomous smart discuss

<domain>
## Phase Boundary

Phase 77 connects require-discovered frontend component definitions to existing markup instantiation and child runtime state. It should preserve legacy component import syntax while adding `bind:this` mounted instance binding as the foundation for parent access to child public members.

</domain>

<decisions>
## Implementation Decisions

### Component Definitions
- Reuse Phase 75 `local Alias = require("...")` import discovery for component definitions.
- Keep actual component instantiation in markup through PascalCase component tags.
- Preserve legacy `import Alias from "..."` compatibility.

### Binding
- Markup attributes continue to become direct public fields on mounted child runtime state.
- `bind:this={name}` should bind the mounted child instance into the parent runtime.
- If full callable cross-runtime function proxying is too large for this phase, expose inspectable child fields/functions and record the remaining callable-function gap explicitly.

### the agent's Discretion
The agent may choose the internal representation for bound child instance metadata as long as it does not break existing local/module component rendering.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ComponentImportTarget` and compiler component collection already handle local and module component imports.
- `FrontendCompositionResolver::render_import` already instantiates local and module components.
- Child component attributes already flow into child runtime state as direct public fields.
- Phase 76 added public field/function inspection helpers on script contexts.

### Established Patterns
- Composition runtime uses stable instance keys for local and imported components.
- Component props intentionally filter internal `__mesh_binding_*` attributes before child runtime state.

### Integration Points
- Parser: add `bind:this` attribute representation.
- Frontend render: pass `bind:this` through as internal composition metadata.
- Shell composition: bind child public member snapshot into parent runtime state.

</code_context>

<specifics>
## Specific Ideas

- Use a reserved internal prop such as `__mesh_bind_this` to carry binding names through the render/composition boundary.

</specifics>

<deferred>
## Deferred Ideas

- Fully callable parent-to-child public function proxies may require an explicit cross-runtime call queue and should not be faked if it cannot be implemented safely.
</deferred>
