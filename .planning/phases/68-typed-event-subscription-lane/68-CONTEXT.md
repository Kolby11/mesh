# Phase 68: Typed Event Subscription Lane - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning
**Mode:** Autonomous smart discuss

<domain>
## Phase Boundary

Phase 68 introduces the Luau object event subscription API for interface proxies and frontend module objects. It should make declared events available as `module.events.Name:subscribe(fn)` style channels with deterministic unsubscribe behavior.

</domain>

<decisions>
## Implementation Decisions

### Subscription API
- Use `events.<Name>:subscribe(fn)` as the canonical subscription shape.
- Return an unsubscribe function from every subscription.
- Provide `emit(payload)` on event channels for local/frontend module events and focused proof.
- Build interface proxy event tables from declared interface contract events.

### Scope
- Establish the event object shape and local dispatch semantics in scripting first.
- Keep full backend-to-frontend typed event transport separate from state snapshots and command results.
- Preserve existing `mesh.events.publish` behavior for shell commands.

### the agent's Discretion
- Internal subscriber storage can be Lua-table based for this phase as long as unsubscribe works and callbacks receive payloads.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `InterfaceContract.events` already parses event metadata from interface contracts.
- Service proxies already have generated object tables for methods/state.
- Frontend scripts now have a `module` object from Phase 66.

### Established Patterns
- Scripting tests are the right focused proof for Luau API shape.
- Transport can be introduced later without changing author-facing syntax.

### Integration Points
- Add `events` to interface proxy tables.
- Add dynamic `module.events.<Name>` channel creation.
- Cover subscribe/emit/unsubscribe in scripting tests.

</code_context>

<specifics>
## Specific Ideas

The user explicitly wants events that can be subscribed to with normal Lua object syntax. This phase prioritizes that syntax and callback behavior.

</specifics>

<deferred>
## Deferred Ideas

- Backend-emitted typed events crossing the Rust shell event bus to frontend subscribers.
- Debug inspector subscription visibility.
- Event payload schema enforcement beyond declared event name exposure.

</deferred>
