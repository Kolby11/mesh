# Phase 1: Backend Host API Contract - Context

**Gathered:** 2026-05-01
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase stabilizes the backend Luau host API contract only: command execution, shell execution, plugin configuration, structured logging, service state emission, and backend poll interval control. It should make backend service plugins reliable and diagnosable without implementing the frontend service proxy delivery layer, interactive surfaces, or documentation reference plugin work from later phases.

</domain>

<decisions>
## Implementation Decisions

### API Shape
- **D-01:** Preserve the public backend API names already used by bundled Luau plugins: `mesh.exec`, `mesh.exec_shell`, `mesh.config`, `mesh.log`, `mesh.service.emit`, and `mesh.service.set_poll_interval`.
- **D-02:** Treat existing bundled backend plugins as compatibility fixtures. Phase 1 should avoid gratuitous script rewrites unless the API contract requires them.
- **D-03:** Keep service-specific logic in Luau backend plugins; the Rust core remains the wiring/runtime layer.

### Command Results and Errors
- **D-04:** `mesh.exec` and `mesh.exec_shell` should return structured result tables with stdout, stderr, exit status/code, and success state rather than throwing for normal nonzero command exits.
- **D-05:** Host API misuse, serialization failures, capability violations, and runtime-level failures should be visible through explicit Luau errors or diagnostics, not silent no-ops.
- **D-06:** Shell command support is required for existing service plugins, but planning should check command quoting and capability boundaries rather than broadening this phase into a security model rewrite.

### Config and Logging
- **D-07:** `mesh.config()` should expose plugin settings as a Luau table. If current code only exposes `mesh.config.get` or `mesh.config.get_all`, planning should reconcile that mismatch with the milestone requirement.
- **D-08:** `mesh.log(level, msg)` is the milestone requirement, while code also advertises `mesh.log.info`, `mesh.log.warn`, and `mesh.log.error`. Planning should decide whether to implement `mesh.log(level, msg)` as the canonical API, keep method aliases, or support both for compatibility.
- **D-09:** Logs should be associated with the plugin identity so backend author failures can be traced to the responsible service.

### Service Emission and Polling
- **D-10:** `mesh.service.emit(payload)` should accept JSON-compatible Luau tables and produce shell-deliverable backend updates.
- **D-11:** `mesh.service.set_poll_interval(ms)` should affect the backend poll loop without requiring shell restart.
- **D-12:** `emit_json` and `emit_unavailable` are existing adjacent APIs and may be preserved, but Phase 1 success is measured against the explicit v1 host API requirements.

### the agent's Discretion
- The planner may choose the exact Rust module split, test names, and diagnostic types as long as public Luau behavior stays stable and testable.
- The planner may add narrowly scoped tests around existing API behavior before changing implementation.
- The planner should defer frontend proxy semantics, quick settings interactions, icon rendering, and API reference docs to later phases unless a small fixture is needed to prove backend emission leaves the backend runtime.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning Scope
- `.planning/PROJECT.md` — milestone goal, core value, in-scope APIs, and out-of-scope follow-up work.
- `.planning/REQUIREMENTS.md` — Phase 1 requirement IDs HOST-01 through HOST-06.
- `.planning/ROADMAP.md` — Phase 1 boundary, dependencies, and success criteria.

### Codebase Map
- `.planning/codebase/ARCHITECTURE.md` — system layers, backend runtime flow, and service event flow.
- `.planning/codebase/STACK.md` — Rust/Luau/mlua/Tokio stack constraints.
- `.planning/codebase/INTEGRATIONS.md` — existing service plugin integrations that exercise backend host APIs.

### Runtime Code
- `crates/core/runtime/scripting/src/backend.rs` — `BackendScriptContext` and backend Luau API implementation.
- `crates/core/runtime/scripting/src/host_api.rs` — host API surface notes and shared helpers.
- `crates/core/runtime/backend/src/lib.rs` — backend poll loop and update channel into the shell.
- `crates/core/shell/src/shell/mod.rs` — backend service spawning and update handling.

### Compatibility Fixtures
- `packages/plugins/backend/core/pipewire-audio/src/main.luau` — real `mesh.exec_shell`, `mesh.service.emit`, and polling usage.
- `packages/plugins/backend/core/pulseaudio-audio/src/main.luau` — alternate audio provider using the same backend API style.
- `packages/plugins/backend/core/networkmanager-network/src/main.luau` — command-heavy network backend and permission-sensitive operations.
- `packages/plugins/backend/core/upower-power/src/main.luau` — `emit_json` and unavailable-state behavior.
- `packages/plugins/backend/core/shell-theme/src/main.luau` — simple periodic service emission.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `BackendScriptContext` in `crates/core/runtime/scripting/src/backend.rs`: central place to expose and test backend Luau API functions.
- Backend runtime tests already embedded near `BackendScriptContext`: useful starting point for HOST requirement coverage.
- Existing core backend plugins under `packages/plugins/backend/core/`: realistic fixtures for command execution, poll interval, service emission, and unavailable states.
- LSP API knowledge in `crates/tools/lsp/src/knowledge/mesh_api.rs`: useful cross-check for public API names, even though LSP work is out of scope.

### Established Patterns
- Rust core wires plugin discovery, runtime, and event routing; service-specific audio/network/power/media logic belongs in Luau plugins.
- Backend plugins run `init()`, `on_poll()`, and `on_command_*()` from `spawn_backend_service()`.
- Service payloads flow as JSON-compatible values from Luau to `BackendServiceUpdate`, then to shell service events.
- Capability-aware backend contexts already exist via `BackendScriptContext::new_with_capabilities`.

### Integration Points
- `crates/core/runtime/scripting/src/backend.rs` connects Luau-visible functions to Rust behavior.
- `crates/core/runtime/backend/src/lib.rs` consumes `BackendScriptContext` and controls polling/command dispatch.
- `crates/core/shell/src/shell/mod.rs` receives backend updates and routes them into shell state.
- Bundled backend plugins are the integration smoke tests because they already use the intended public API.

</code_context>

<specifics>
## Specific Ideas

- Use the existing bundled audio, network, power, and theme backend scripts as real-world compatibility checks.
- Reconcile the milestone's `mesh.config()` and `mesh.log(level, msg)` requirements with current code comments that mention `mesh.config.get/get_all` and `mesh.log.info/warn/error`.
- Keep normal command failure as data in a returned result table; reserve exceptions/diagnostics for API misuse and runtime failures.

</specifics>

<deferred>
## Deferred Ideas

- Frontend `require('@mesh/<service>')` proxy behavior belongs to Phase 2.
- Frontend reactivity and element handler reliability belong to Phase 3.
- Top panel and quick settings integration belong to Phase 4.
- XDG icon rendering reliability belongs to Phase 5.
- API reference docs and fresh reference plugin validation belong to Phase 6.

</deferred>

---

*Phase: 1-Backend Host API Contract*
*Context gathered: 2026-05-01*
