# Phase 74: Import Resolver Contract - Context

**Gathered:** 2026-05-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 74 defines the author-facing Luau import and execution-context contract for MESH runtimes. It must clarify how scripts import external dependencies and how the current frontend component or backend provider exposes its own fields/functions, without implementing the later component compiler work from Phase 76.

</domain>

<decisions>
## Implementation Decisions

### Current Instance Context
- **D-01:** Use explicit lifecycle parameters for the current runtime instance: `function render(self)`, `function mount(self)`, `function unmount(self)` for frontend components and `function start(self)`, `function stop(self)` for backend providers.
- **D-02:** `self` represents the current object instance, not an importable dependency. Do not introduce `require("mesh.module")` as the way to access the current module/component context.
- **D-03:** Keep the author-facing `self` surface narrow for now: `self.meta` for identity/diagnostics and `self.storage` for shell-backed persistence.

### Public And Private Script Members
- **D-04:** Lua `local` variables and functions are private implementation details of the script/component.
- **D-05:** Non-local variables and functions are public members of the script object. Other modules/components can read or call them after requiring the script or binding a mounted component instance.
- **D-06:** Use plain Lua identifier syntax and document snake_case for public member names. Dashed member names are not normal Lua identifiers and should not be the primary authoring style.
- **D-07:** Lifecycle hooks such as `render`, `mount`, `unmount`, `start`, and `stop` are reserved runtime hooks. They can be inspectable for diagnostics but should not be treated as ordinary public API members.

### Frontend Component Definitions And Instances
- **D-08:** `require("./component")` for a frontend component returns a component definition, equivalent to a class/component constructor conceptually. It is not the mounted instance.
- **D-09:** Frontend component instantiation happens through markup usage such as `<AudioSlider />`.
- **D-10:** Mounted component instances are exposed through Svelte-style binding syntax such as `<AudioSlider bind:this={audio_slider} />`.
- **D-11:** Markup attributes become direct public fields on the component instance, not entries in `self.props`.
- **D-12:** A bound instance exposes its public variables and functions directly, for example `audio_slider.audio_volume` and `audio_slider.increase_volume(10)`.
- **D-13:** Runtime diagnostics should make definition-vs-instance misuse visible, for example calling an instance method on a component definition should tell the author to bind the mounted instance.

### Backend Singletons
- **D-14:** Backend services/providers imported through `require("mesh.audio")` or `require("mesh.audio@>=1.0")` remain singleton objects for now.
- **D-15:** Backend provider scripts should expose interface variables and functions through non-local members, with manifest/interface validation handled by the runtime and later planning.

### Imports
- **D-16:** Use `require(...)` for external dependencies such as services, shell APIs, libraries, and component definitions.
- **D-17:** Keep `self.*` for the current instance and `require(...)` for external dependencies. The two concepts should not overlap.
- **D-18:** Do not introduce JavaScript-style named imports in this milestone.

### Rendering And Invalidation
- **D-19:** Dependency changes should automatically rerender affected frontend components. Authors should not need to call explicit invalidation for normal service, locale, theme, storage, or bound-field dependency updates.
- **D-20:** The runtime/planner may choose the dependency tracking mechanism, but the user-facing contract should be automatic rerendering of dependencies.

### Events
- **D-21:** Event syntax is not locked yet. The user does not like `emit(...)` with string literal event names as the final authoring model.
- **D-22:** Event design is a required follow-up discussion before planning phases that expose or consume new public event APIs.

### the agent's Discretion
The agent may choose the best implementation details for lifecycle cleanup, error boundaries, async/timer helpers, capability checks, type validation, and reload semantics, as long as they preserve the locked author-facing syntax above and keep frontend/backend behavior aligned where their host contexts overlap.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone Planning
- `.planning/REQUIREMENTS.md` - v1.14 requirements, including import resolver, runtime parity, component imports, compatibility, and docs.
- `.planning/ROADMAP.md` - phase boundaries and execution rules for v1.14.

### Existing Module Object Contract
- `.planning/milestones/v1.12-phases/65-module-instance-registry/65-CONTEXT.md` - prior decision that backend and frontend modules should feel like class-like Luau object instances.
- `.planning/milestones/v1.12-phases/66-state-and-export-read-model/66-CONTEXT.md` - existing state/export read model and `require("mesh.audio").state.field` compatibility.
- `.planning/milestones/v1.12-phases/67-method-call-result-lane/67-CONTEXT.md` - method call result lane decisions.
- `.planning/milestones/v1.12-phases/68-typed-event-subscription-lane/68-CONTEXT.md` - current event subscription API decisions; revisit because event syntax remains unlocked for v1.14.
- `.planning/milestones/v1.12-phases/69-shipped-module-object-proof/69-CONTEXT.md` - shipped module object proof context.

### Runtime And Author Docs
- `docs/frontend/mesh-syntax.md` - current `.mesh` component syntax and service proxy examples.
- `docs/module-system.md` - module system principles and lifecycle ownership.
- `docs/modules/backend/core/README.md` - current backend service `require("mesh.<interface>")` author guidance.
- `docs/health.md` - current interface proxy require behavior and diagnostics.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/runtime/scripting/src/context/runtime.rs`: existing Luau context setup and host API installation path, including global `mesh` and `require`.
- `crates/core/runtime/scripting/src/host_api.rs`: existing `require("mesh.<service>")` proxy behavior.
- `crates/core/frontend/compiler/src/compile.rs`: current frontend `.mesh` import handling and component compilation integration point.

### Established Patterns
- Service/interface imports already use `require("mesh.<interface>@<constraint>")` and should remain compatible.
- v1.12 established module object registry, state/export reads, method results, and typed event channels. v1.14 should align author syntax with that model rather than discarding the runtime foundation.
- v1.13 established runtime localization updates and should inform automatic rerender dependency behavior for locale/theme/service changes.

### Integration Points
- Runtime context setup must provide explicit `self` lifecycle arguments while preserving existing globals during migration.
- Frontend compiler/runtime must distinguish component definitions returned by `require(...)` from mounted component instances exposed by `bind:this`.
- Backend scripting must continue treating providers/services as singleton objects while adopting the same public/private script member rules.

</code_context>

<specifics>
## Specific Ideas

- The user prefers Lua-native variable/function exposure over `self.props`, `self.state`, `self.events`, or `self.exports` as primary author-facing syntax.
- Markup attributes should write direct public fields on the mounted component instance.
- `bind:this` is the preferred instance binding concept, inspired by Svelte.
- Automatic dependency rerendering is required; explicit invalidation should not be the normal author workflow.

</specifics>

<deferred>
## Deferred Ideas

- Final event authoring syntax is deferred to a dedicated follow-up discussion. Avoid locking `emit("event.name")` string-literal APIs as the long-term public model.
- JavaScript-style named imports remain deferred until the Luau-native contract is stable.
- Advanced component instantiation APIs such as manually calling `.new(...)` from Lua remain deferred; markup owns frontend component instantiation for now.

</deferred>

---

*Phase: 74-Import Resolver Contract*
*Context gathered: 2026-05-24*
