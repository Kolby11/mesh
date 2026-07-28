# Phase 75: Require Resolver And Host APIs - Context

**Gathered:** 2026-05-26
**Status:** Ready for planning
**Mode:** Autonomous smart discuss

<domain>
## Phase Boundary

Phase 75 makes `require(...)` the canonical entry point for host-provided dependencies already supported by the runtime: shell API tables, service/interface proxies, bundled Luau helper libraries, and frontend component import targets. It must preserve current `require("mesh.audio@>=1.0")` behavior while moving hardcoded branches into a reusable resolver shape that later phases can extend.

</domain>

<decisions>
## Implementation Decisions

### Resolver Scope
- Support canonical require specifiers for existing frontend host API tables with current runtime support: `mesh.locale`, `mesh.ui`, `mesh.events`, `mesh.log`, and `mesh.popover`.
- Keep `mesh.theme` on the existing interface proxy path until a concrete theme host table exists; do not invent a new theme API in this phase.
- Preserve `@mesh/i18n` and add `mesh.i18n` as a Luau helper library alias so author examples can converge on the `mesh.*` namespace without breaking shipped modules.
- Keep unsupported imports pcall-safe by returning Lua errors backed by `ScriptError`, and keep interface lookup diagnostics visible for capability/provider/contract failures.

### Service And Interface Imports
- Preserve `require("mesh.audio@>=1.0")` exactly, including version parsing, capability checks, provider/contract lookup, proxy creation, and diagnostic behavior.
- Treat service/interface proxies as the default resolution path for `mesh.<name>` specifiers not claimed by host API tables or helper libraries.
- Do not reintroduce the older `@mesh/audio` require syntax; existing tests rejecting that form should continue to pass.

### Frontend Component Requires
- Parse Luau-native component definition imports such as `local Slider = require("./slider.mesh")` and `local Panel = require("@mesh/panel")` into the same `ComponentImportTarget` records as legacy `import Alias from "..."`.
- Leave component definition object semantics to Phase 77. Phase 75 should make the compiler see the import target through the canonical require syntax so the later runtime work has a typed graph to consume.
- Preserve the existing `import Alias from "..."` compatibility syntax during v1.14.

### the agent's Discretion
The agent may choose the internal resolver abstraction shape, parser helpers, and tests as long as the author-facing import strings above remain stable and current compatibility tests keep passing.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/runtime/scripting/src/context/runtime.rs` installs frontend `mesh` globals and the current `require` closure.
- `crates/core/runtime/scripting/src/context/lookup.rs` already centralizes lookup diagnostics and Lua error mapping.
- `crates/core/runtime/scripting/src/context/proxy.rs` creates service/interface proxy tables and event channels.
- `crates/core/ui/component/src/parser/script.rs` already classifies legacy import sources into local component, module component, and interface API targets.
- `crates/core/frontend/compiler/src/compile.rs` already consumes `ComponentImportTarget` records for component graph collection.

### Established Patterns
- Resolver failures should be non-fatal at component level when wrapped in `pcall`, but diagnostics must still be captured for missing interfaces.
- The compiler owns component import graph discovery; the scripting runtime owns live Lua dependency resolution.
- Compatibility tests already reject legacy `@mesh/<service>` service imports and should remain valid.

### Integration Points
- Add helper functions around frontend `require` resolution without changing public `ScriptContext` construction.
- Extend component parser import extraction to also recognize simple `local Alias = require("source")` declarations.
- Add focused tests in `mesh-core-scripting` and `mesh-core-component`; avoid broad shell behavior changes in this phase.

</code_context>

<specifics>
## Specific Ideas

- Prefer a small internal enum/resolver function over leaving all `require` behavior inline in the closure.
- Keep frontend component require parsing intentionally conservative: only simple local assignment to `require("...")`, not destructuring or dynamic expressions.

</specifics>

<deferred>
## Deferred Ideas

- Returning real component definition objects from Lua require is deferred to Phase 77.
- Public/private member extraction from required scripts is deferred to Phase 76.
- Named event channel convenience APIs are deferred to Phase 78.
</deferred>
