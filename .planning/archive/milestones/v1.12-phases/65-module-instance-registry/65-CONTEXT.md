# Phase 65: Module Instance Registry - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning
**Mode:** Autonomous smart discuss

<domain>
## Phase Boundary

Phase 65 establishes the first runtime-visible module object registry. It should register backend services and frontend modules as stable object instances with inspectable metadata, without yet implementing state/export replay, method results, or event subscriptions from later phases.

</domain>

<decisions>
## Implementation Decisions

### Registry Shape
- Use existing module ids and surface ids as stable instance identities for the first implementation.
- Represent backend provider instances separately from module lifecycle entries so active-provider status and interface/version metadata are visible.
- Treat frontend mounted surfaces as frontend object instances keyed by surface id, with parent module id preserved.
- Keep the registry shell-owned and derived from existing runtime state for this phase.

### Debug And Diagnostics
- Expose the registry through `mesh.debug` so authors and tests can inspect module instances without adding a new public API surface yet.
- Include lifecycle, active flag, capabilities, interface, and version metadata where existing runtime structures already know them.
- Keep diagnostics generic and avoid service-specific branches.
- Preserve existing `modules`, `interfaces`, and `backend_runtimes` debug payloads for compatibility.

### the agent's Discretion
- Exact Rust type names, debug field names, and sorting rules are implementation details as long as the resulting payload is deterministic and supports later phases.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `mesh_core_debug::DebugSnapshot` already carries shell-visible debug payload sections.
- `Shell::debug_snapshot()` already combines modules, interfaces, backend runtimes, health, keybinds, active surfaces, benchmarks, and profiling.
- `InterfaceRegistry::catalog()` exposes provider module, interface, version, backend name, and priority.
- `Shell` already tracks mounted `ComponentRuntime` entries and active `BackendRuntimeSlot` entries.

### Established Patterns
- Debug payloads are Rust structs in `crates/core/foundation/debug`, then serialized in `crates/core/shell/src/shell/runtime/debug.rs`.
- Interface contracts are documented in `modules/interfaces/debug.toml`.
- Existing debug arrays sort deterministically before exposure where ordering matters.

### Integration Points
- Add a `module_instances` section to `mesh.debug`.
- Populate it from discovered modules, mounted frontend components, and registered interface providers.
- Use the new payload as the Phase 65 proof point for later state/export, method, and event lanes.

</code_context>

<specifics>
## Specific Ideas

The user wants backend and frontend modules to feel like class-like Luau object instances with attributes, functions, and subscribable events. Phase 65 only creates the inspectable object-instance registry underneath that model.

</specifics>

<deferred>
## Deferred Ideas

- Replayable `module.state` and `module.exports` data are Phase 66.
- Method call result handling is Phase 67.
- Typed event subscription and cleanup are Phase 68.
- Shipped author-facing migration and bundled module proof are Phase 69.

</deferred>
