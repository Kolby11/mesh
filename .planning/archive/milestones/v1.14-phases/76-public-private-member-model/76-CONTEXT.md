# Phase 76: Public Private Member Model - Context

**Gathered:** 2026-05-26
**Status:** Ready for planning
**Mode:** Autonomous smart discuss

<domain>
## Phase Boundary

Phase 76 locks the runtime vocabulary for script members: Lua `local` values remain private, non-local values/functions are public object members, and lifecycle hooks are reserved runtime hooks rather than ordinary public API. It should preserve the current reactive global sync behavior while exposing enough runtime metadata for later component binding and diagnostics phases.

</domain>

<decisions>
## Implementation Decisions

### Public Member Surface
- Treat synced non-local variables as public fields using the existing `ScriptState` path; do not introduce `self.props`, `self.state`, or a second field store.
- Treat non-local functions as public functions for runtime inspection and later binding, but exclude reserved lifecycle hooks from ordinary public API membership.
- Keep `module.exports` compatibility unchanged during v1.14; docs should teach public members as the canonical wording.

### Privacy Boundary
- Preserve Lua locals as private by relying on normal Luau lexical scoping; no parser-level privacy syntax is needed.
- Do not expose local functions or local values through runtime public-member metadata.

### Reserved Runtime Hooks
- Reserve frontend lifecycle hooks such as `init`, `render`, `mount`, `unmount`, and legacy `onRender`.
- Reserve backend lifecycle hooks such as `start`, `stop`, and legacy `init`.
- Runtime hooks can remain callable by shell internals and visible in diagnostics, but should not appear as ordinary public member names.

### the agent's Discretion
The agent may choose whether public member metadata is exposed through methods, debug state, or state snapshots, provided existing rendering behavior remains compatible and future phases can build binding diagnostics on it.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ScriptState` already stores non-local JSON-like globals as reactive state.
- `ScriptContext::sync_state_from_lua` already skips builtins, private locals, functions, and `__mesh_` internals.
- Frontend docs already use "public/private members" wording, so implementation can align with current documentation.

### Established Patterns
- Runtime state should not mark host-maintained metadata dirty unless user-visible values changed.
- Lifecycle compatibility paths from Phase 74 should remain intact.

### Integration Points
- Add focused public-member inspection helpers to scripting runtime contexts.
- Add tests that prove locals stay private, public fields stay reactive, public functions are discoverable, and lifecycle hooks are reserved.

</code_context>

<specifics>
## Specific Ideas

- Avoid storing Lua functions in `ScriptState` because it is JSON-backed and feeds template rendering.
- A name-list API is enough for this phase; actual bound instance invocation can be implemented in Phase 77.

</specifics>

<deferred>
## Deferred Ideas

- Parent-to-child mounted instance calls are deferred to Phase 77.
- Event/member conflict diagnostics are deferred to Phase 78.
</deferred>
