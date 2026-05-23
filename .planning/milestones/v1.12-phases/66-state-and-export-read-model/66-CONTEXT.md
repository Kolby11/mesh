# Phase 66: State And Export Read Model - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning
**Mode:** Autonomous smart discuss

<domain>
## Phase Boundary

Phase 66 makes durable module data readable through canonical object syntax. Backend service state should remain available through `module.state` style snapshots, and frontend modules should gain a first runtime-owned `module.exports` table for public values. Later phases will add method result and event subscription lanes.

</domain>

<decisions>
## Implementation Decisions

### Backend State Replay
- Preserve the existing `require("mesh.audio").state.field` proxy behavior.
- Keep direct `audio.field` reads as compatibility aliases for now.
- Cache latest service payloads in frontend components so runtimes created after an update are seeded before script execution.
- Reuse existing capability checks before applying cached service payloads to runtimes.

### Frontend Exports
- Install a Luau `module` object for frontend scripts.
- Use `module.state` as a shell-refreshed snapshot of current `ScriptState`.
- Use `module.exports` as the first public frontend export table, mirrored into `ScriptState["exports"]`.
- Keep frontend export transport scoped to local runtime state in this phase; cross-module frontend imports belong after registry and method/event lanes mature.

### the agent's Discretion
- Exact internal refresh timing is implementation detail as long as top-level script execution can read host-seeded state and exports are captured after load/handlers.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ScriptContext::apply_service_payload` already seeds `__mesh_svc_<service>` globals used by service proxies.
- `FrontendSurfaceComponent::handle_service_event` already fans backend payloads into every active embedded runtime.
- `ScriptState` already owns reactive JSON values and implements `VariableStore`.

### Established Patterns
- Host-maintained state enters scripts through `ScriptState` and Luau globals.
- Service updates are replayed by rebroadcasting cached `ServiceEvent::Updated` values.
- Capability checks live at the shell/component delivery boundary.

### Integration Points
- Add a `ScriptState::snapshot()` read model.
- Install and refresh `module.state` and `module.exports` in `ScriptContext`.
- Cache latest service payloads in `FrontendSurfaceComponent` for newly created runtimes.

</code_context>

<specifics>
## Specific Ideas

The user wants normal Lua object/class-style data access. This phase provides the data/read half of that model before methods and events are implemented.

</specifics>

<deferred>
## Deferred Ideas

- Cross-module frontend `require("@mesh/frontend")` support remains deferred.
- Method result delivery remains Phase 67.
- Event subscription remains Phase 68.

</deferred>
