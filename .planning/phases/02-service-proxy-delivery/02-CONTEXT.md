# Phase 02: service-proxy-delivery - Context

**Gathered:** 2026-05-02
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase stabilizes `require("@mesh/<service>")` as the frontend/backend bridge for service state reads, backend command methods, field-level reactive invalidation, and visible contract diagnostics. It should replace callback-style service update APIs in shipped frontend surfaces, keep service proxies as a state-and-command surface, and lock the interface/provider model that richer backend providers build on.

</domain>

<decisions>
## Implementation Decisions

### Reactive Update Model
- **D-01:** Service proxies are a read-and-command surface only. They should not expose subscription/update APIs such as `proxy.on_change(...)`, `mesh.service.on(...)`, or `mesh.service.bind(...)`.
- **D-02:** Phase 2 should remove legacy callback/bind service update paths rather than keep them as compatibility APIs.
- **D-03:** Components should rerender only when a service update changes a top-level proxy field that the component actually read during normal script execution and render.
- **D-04:** Dependency tracking should refresh on each rerender.
- **D-05:** Change detection should be value-based for tracked top-level fields, not whole-service invalidation and not "field present in payload" invalidation.

### Proxy Command Surface
- **D-06:** Frontend writes happen through proxy command methods only, never by mutating proxy state directly.
- **D-07:** The proxy command shape should be named methods on the proxy, such as `audio.set_volume(50)` and `network.set_wifi_enabled(true)`.
- **D-08:** The command lifecycle is: frontend calls a proxy command, backend handles it, backend emits updated state, and frontend rerenders if tracked fields changed.
- **D-09:** If the active provider does not support a called command, the runtime should raise a Lua error and emit a visible diagnostic.

### Failure Visibility
- **D-10:** Missing or invalid `require("@mesh/<service>")` lookups should emit a visible plugin diagnostic and still return the normal Lua error.
- **D-11:** Wrapping a lookup in `pcall(require, ...)` may catch the Lua error for graceful fallback UI, but it must not suppress the visible diagnostic.
- **D-12:** Lookup diagnostics should include the calling plugin, the requested interface, and the concrete failure reason.
- **D-13:** These diagnostics should be treated as errors, not warnings.

### Service Interface Model
- **D-14:** Services should have a real interface rather than a purely ad hoc runtime surface.
- **D-15:** The portable core should live as an explicit base interface plugin. The user-described model is analogous to a base contract such as `@mesh/backend/base/network`, which other providers can inherit from while preserving the exported interface.
- **D-16:** Providers should formally declare that they inherit from or extend the base interface plugin.
- **D-17:** Providers may completely change their internal logic as long as they continue exporting the same base interface contract unless they intentionally add extensions.
- **D-18:** Richer providers may expose additive fields and commands on the same interface instead of being forced into a separate interface by default.

### Core vs Advanced Provider Surface
- **D-19:** The ecosystem model is a portable core plus a richer dominant provider. The weakest provider should not define the practical public API ceiling.
- **D-20:** Built-in surfaces may rely on dominant-provider extras for advanced behavior, but the core user path should still work on the shared base contract.
- **D-21:** The base contract should support both core reads and the primary user commands, not read-only display.
- **D-22:** The split should be "documented core, runtime-defined extras": keep a clearly defined shared baseline while letting richer provider fields/commands emerge from runtime behavior and shipped surface usage.

### Bundled Surface Migration Scope
- **D-23:** Phase 2 should migrate all bundled consumers in scope, including panel and relevant quick-settings components.
- **D-24:** Bundled panel and quick-settings code should remove legacy `mesh.service.bind(...)`, `mesh.service.on(...)`, and `proxy.on_change(...)` usage completely.
- **D-25:** Bundled surfaces may assume the dominant richer provider for advanced functionality, but their core read path and primary commands should still function on the base interface.

### the agent's Discretion
- The planner may choose the exact dependency-tracking data structure, field snapshot strategy, and invalidation wiring as long as the public semantics above hold.
- The planner may decide how base interface plugin inheritance is represented internally in registry/runtime code, as long as providers can formally declare inheritance/extension and keep exporting the same public interface.
- The planner may choose the exact test split across runtime, shell, interface, and bundled-plugin coverage.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning Scope
- `.planning/PROJECT.md` — milestone goal, external-developer target, and the requirement that `require("@mesh/<service>")` becomes the reliable frontend/backend bridge.
- `.planning/REQUIREMENTS.md` — Phase 2 requirement IDs `PROXY-01` through `PROXY-06` and `SURF-06`.
- `.planning/ROADMAP.md` — Phase 2 goal, success criteria, and dependency on Phase 1.
- `.planning/STATE.md` — current project position and note that Phase 2 already has plans created without user context.
- `.planning/phases/01-backend-host-api-contract/01-CONTEXT.md` — prior locked decisions about backend Luau APIs, explicit failures, and Rust as the wiring layer.

### Phase 2 Research and Existing Plans
- `.planning/phases/02-service-proxy-delivery/02-RESEARCH.md` — current runtime findings, legacy proxy/callback mix, and recommended runtime/docs/surface split.
- `.planning/phases/02-service-proxy-delivery/02-01-PLAN.md` — current runtime-oriented plan that should be reviewed against the new field-level reactivity and callback-removal decisions.
- `.planning/phases/02-service-proxy-delivery/02-02-PLAN.md` — current contract/docs plan that should be reviewed against the base-interface-plugin and runtime-defined-extras decisions.
- `.planning/phases/02-service-proxy-delivery/02-03-PLAN.md` — current bundled-surface plan that should be reviewed against the "migrate all consumers" and "dominant-provider advanced behavior" decisions.

### Codebase Map
- `.planning/codebase/STACK.md` — Rust/Luau/mlua/Tokio constraints for proxy runtime and provider/plugin design.
- `.planning/codebase/ARCHITECTURE.md` — frontend/backend/runtime/service-event flow and the no-service-logic-in-core architectural rule.
- `.planning/codebase/INTEGRATIONS.md` — current audio/network/power/media provider landscape and existing interface contracts.

### Runtime and Shell Code
- `crates/core/runtime/scripting/src/context.rs` — current `require("@mesh/<service>")` proxy implementation, legacy `:on_change(...)`, `mesh.service.bind(...)`, and `mesh.service.on(...)` behavior.
- `crates/core/shell/src/shell/component.rs` — current service update handling, rerender dirtiness, and explicit service handler dispatch.
- `crates/core/shell/src/shell/service.rs` — service command routing from frontend script events toward backend requests.
- `crates/core/extension/service/src/interface.rs` — interface/provider resolution and the right place to reason about base-interface inheritance and provider selection.
- `crates/core/extension/service/src/contract.rs` — current contract model and likely integration point for interface/base-contract evolution.

### Bundled Contracts and Surfaces
- `packages/plugins/backend/core/audio-interface/interface.toml` — current audio command/event contract.
- `packages/plugins/backend/core/network-interface/interface.toml` — current network command/event contract.
- `packages/plugins/backend/core/power-interface/interface.toml` — current power command/event contract.
- `packages/plugins/backend/core/media-interface/interface.toml` — current media command/event contract.
- `packages/plugins/frontend/core/panel/src/main.mesh` — bundled panel consumer that should validate the finalized proxy model.
- `packages/plugins/frontend/core/quick-settings/src/main.mesh` — quick-settings root with current legacy bind/on usage.
- `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` — current `require("@mesh/audio@>=1.0")` and `audio.on_change(...)` usage.
- `packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh` — bundled network consumer with current legacy service subscription behavior.
- `packages/plugins/frontend/core/quick-settings/src/components/bluetooth-section.mesh` — bundled network-adjacent consumer with current legacy service subscription behavior.

### Docs and Editor Knowledge
- `crates/tools/lsp/src/knowledge/mesh_api.rs` — current editor-facing proxy description that still advertises `:bind(...)` and `:on_change(...)`.
- `docs/plugins/backend/core/README.md` — backend contract documentation that should stay aligned with the service-interface model.
- `docs/plugins/frontend/core/README.md` — frontend plugin guidance that should reflect the finalized read-and-command proxy model.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ScriptContext` in `crates/core/runtime/scripting/src/context.rs`: existing proxy creation path, service payload injection, and current tests that can be extended for field-level dependency tracking.
- `FrontendSurfaceComponent::handle_service_event` in `crates/core/shell/src/shell/component.rs`: current place where service payloads are applied and components are marked dirty.
- `script_events_to_requests` in `crates/core/shell/src/shell/service.rs`: existing command-publication path that should back named proxy methods.
- `InterfaceRegistry` and contract loading in `crates/core/extension/service/src/interface.rs` and `contract.rs`: current location for modeling base interface plugins and provider inheritance.
- Bundled panel and quick-settings `.mesh` files: real fixtures for validating migration off legacy callback/bind APIs.

### Established Patterns
- Rust owns shared service state and routing; frontend and backend Luau runtimes do not share direct Lua references.
- Backend services emit JSON-compatible payloads; frontend proxies read from Rust-owned service payload tables.
- Service updates already pass through shell/component state application before rerender; Phase 2 should refine that path rather than invent a second event model.
- Provider-specific service logic belongs in backend plugins, while the Rust core should remain generic wiring/runtime infrastructure.

### Integration Points
- Proxy field read tracking will connect `crates/core/runtime/scripting/src/context.rs` with shell-side service update invalidation in `crates/core/shell/src/shell/component.rs`.
- Named proxy command methods will continue flowing through `crates/core/shell/src/shell/service.rs` into backend providers.
- Base interface plugin inheritance will likely connect contract parsing/loading in `crates/core/extension/service/src/contract.rs` with provider resolution in `crates/core/extension/service/src/interface.rs`.
- Bundled surface migration will connect runtime semantics, interface expectations, and docs/LSP language in a single end-to-end proof path.

</code_context>

<specifics>
## Specific Ideas

- Treat service proxies as a strict "state view + command surface" model: read fields like `audio.percent`, call methods like `audio.set_volume(50)`, and let backend emissions drive rerender.
- Use top-level field dependency tracking rather than whole-service invalidation or deep-path tracking.
- Keep the base contract as a plugin-level artifact so future shell concepts can define their own base interface plugins and richer derived providers.
- Let richer providers extend the same interface additively while keeping the core user path portable.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 02-service-proxy-delivery*
*Context gathered: 2026-05-02*
