# Phase 67: Method Call Result Lane - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning
**Mode:** Autonomous smart discuss

<domain>
## Phase Boundary

Phase 67 makes object method calls observable as a real shell-managed lane. It should keep existing service proxy command behavior while recording queued calls and backend command results beyond tracing. Fully synchronous Lua awaits are out of scope.

</domain>

<decisions>
## Implementation Decisions

### Method Lane Shape
- Treat existing generated service proxy methods as the first object method lane.
- Keep immediate Lua return values as dispatch acknowledgements.
- Record queued dispatches and backend results in debug state so result/failure data is visible.
- Preserve coalescing, capability checks, contract checks, and active-provider routing.

### Backend Results
- Promote `BackendServiceEvent::CommandResult` from tracing-only to a shell message.
- Store backend result payloads in a bounded recent method-call list.
- Represent backend handler failures as method call entries with failed status when the result payload carries `ok=false`.

### the agent's Discretion
- Debug field names and retention length are implementation details as long as recent calls are deterministic and bounded.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `dispatch_service_command` already performs capability, contract, provider, coalescing, and optimistic-audio behavior.
- Backend runtime already emits `BackendServiceEvent::CommandResult`.
- `mesh.debug` already publishes shell debug payloads to frontend consumers.

### Established Patterns
- Shell messages bridge async backend runtime events back into the shell loop.
- Debug payloads keep recent bounded runtime data visible to inspector surfaces.

### Integration Points
- Add a backend command result shell message.
- Record dispatch acknowledgements in `dispatch_service_command`.
- Record backend command results in the shell runtime handler.
- Expose the recent method lane through `mesh.debug.method_calls`.

</code_context>

<specifics>
## Specific Ideas

The user wants module calls to feel like class/object method calls. This phase does not yet add async Lua handles, but it makes calls and their backend results first-class shell data instead of logs only.

</specifics>

<deferred>
## Deferred Ideas

- Synchronous or awaitable Lua method result handles.
- Frontend-to-frontend method calls.
- Event subscriptions, which are Phase 68.

</deferred>
