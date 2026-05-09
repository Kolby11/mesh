# Phase 4: Service Provider Contract - Context

**Gathered:** 2026-05-03
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase connects backend providers to service interfaces generically so backend module state and command dispatch work without service-specific Rust branches. It defines the MVP service contract for active provider imports, exported backend state, latest-state storage, command result visibility, and contract validation behavior.

This phase does not add Lua package manager/dependency installation, remote module distribution, sandboxing/signing, new frontend shell surfaces, LSP tooling, or the full Phase 5 diagnostics/reference plugin proof. It should leave Rust as generic wiring and keep service behavior in Luau modules.

</domain>

<decisions>
## Implementation Decisions

### Reactive Backend State Contract
- **D-01:** Backend module identity comes from the module's own `package.json` `id`. Runtime/provider metadata should use that package id instead of inventing a separate backend identity.
- **D-02:** Backend Luau modules should expose service state through a special exported top-level `state` variable. A non-`local` top-level assignment such as `state = { available = true, percent = 65 }` becomes the module's public reactive state.
- **D-03:** The runtime snapshots exported `state` after `init()`, `on_poll()`, and `on_command_*()` return. If the JSON-compatible snapshot changed, the shell propagates it reactively to consumers.
- **D-04:** `mesh.service.emit(payload)` is no longer the preferred MVP author-facing state API. It may remain as compatibility/migration behavior if useful, but planning should move bundled providers and docs toward exported `state`.
- **D-05:** Provider authors should not manually inject runtime identity fields such as `source_plugin` into service state. Provider identity is runtime metadata, not part of the normal state table.

### Module Import And Provider Resolution
- **D-06:** Normal consumers import the service/interface id, not the concrete provider. For example, `local audio = require("@mesh/audio")` resolves to the active provider for the audio interface.
- **D-07:** Direct provider imports such as `require("@mesh/pipewire-audio")` may exist for explicit provider-specific use, but ordinary frontend/backend module code should use the interface import so provider swapping stays possible.
- **D-08:** Imported service modules expose reactive state as `module.state`, for example `audio.state.percent`.
- **D-09:** The `require(...)` design should not block future Lua package manager support, but package manager/imported Lua library behavior is out of scope for Phase 4.

### Command Dispatch And Results
- **D-10:** Service proxy methods dispatch generically to backend Luau command handlers without service-specific Rust branches.
- **D-11:** Command methods return a small caller-visible result table and may also update reactive provider state. Example: `local result = audio.set_volume("default", 0.65)` returns `{ ok = true }` or `{ ok = false, error = "..." }`.
- **D-12:** State updates remain the source of truth for current service data after commands. The command result communicates immediate success/failure; the reactive `state` communicates the resulting service state.

### Latest State Storage
- **D-13:** The shell stores latest state per interface for normal consumers. `require("@mesh/audio").state` means the active audio provider's latest state.
- **D-14:** Provider identity is tracked as metadata alongside latest interface state for diagnostics/debugging/provider replacement, but it is not injected into the public `state` table by default.
- **D-15:** Provider swaps replace the interface-level state mapping with the newly active provider's state according to the existing explicit provider-selection model from Phase 2.

### Contract Validation Strictness
- **D-16:** Phase 4 should validate provider declarations and command names against interface metadata, but state shape mismatches should be warning/diagnostic-visible rather than stopping the provider.
- **D-17:** Runtime validation should be strict enough to catch unknown command methods and bad interface/provider wiring, while leaving full failure degradation, deduplication, and reference proof work to Phase 5.
- **D-18:** Planner/researcher should choose the exact warning/diagnostic text and validation layering, as long as plugin authors can see contract mismatches without Rust core becoming service-specific.

### the agent's Discretion
- Planner/researcher may choose the exact Rust and Luau host API mechanics for detecting exported top-level `state`, snapshotting it, and converting it to JSON-compatible state.
- Planner/researcher may choose whether `mesh.service.emit(...)` remains as an alias to state assignment, a compatibility-only API, or a deprecated API with tests documenting the transition.
- Planner/researcher may choose the exact result-table fields beyond `ok` and `error`, but command success/failure must be visible to the caller.
- Planner/researcher may decide whether direct provider imports are implemented in Phase 4 or only preserved as a future-compatible design point, as long as interface imports resolve to the active provider.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone And Phase Scope
- `.planning/PROJECT.md` — Active v1.1 milestone intent, Phase 3 completion state, and locked architecture.
- `.planning/REQUIREMENTS.md` — BSVC-01 through BSVC-05 requirements and traceability.
- `.planning/ROADMAP.md` — Phase 4 goal, dependencies, and success criteria.
- `.planning/STATE.md` — Current accumulated decisions, including Phase 4 proxy/capability decisions already recorded.
- `.planning/phases/01-plugin-package-manifest-foundation/01-CONTEXT.md` — Package/module naming, installed graph, provider identity, and `package.json` decisions.
- `.planning/phases/02-backend-lifecycle-foundation/02-CONTEXT.md` — Explicit active provider selection, strict no-fallback behavior, lifecycle ordering, and runtime eligibility decisions.
- `.planning/phases/03-backend-host-api-contract/03-CONTEXT.md` — Strict backend host API, structured `mesh.exec`, settings/logging/poll interval decisions, and Phase 4 command-response deferral.

### Existing Codebase Maps
- `.planning/codebase/STACK.md` — Rust/Luau/Tokio/mlua/serde_json stack constraints.
- `.planning/codebase/ARCHITECTURE.md` — Shell/runtime/service layering, backend event flow, and no-service-logic-in-core rule.
- `.planning/codebase/INTEGRATIONS.md` — Existing bundled backend providers and interface contracts.

### Service Runtime And Shell Wiring
- `crates/core/runtime/scripting/src/backend.rs` — Current `BackendScriptContext`, `mesh.service.emit`, command handler invocation, payload API, and backend host API tests.
- `crates/core/runtime/backend/src/lib.rs` — Backend poll loop, command dispatch, `BackendServiceCommand`, `BackendServiceUpdate`, and lifecycle event bridge.
- `crates/core/shell/src/shell/mod.rs` — Service handler map, latest service event cache, backend runtime slots, command dispatch permission checks, and backend event handling.
- `crates/core/shell/src/shell/types.rs` — `CoreRequest::ServiceCommand`, `ServiceEvent::Updated`, and component-facing service event shape.

### Interface And Manifest Contracts
- `crates/core/extension/plugin/src/package.rs` — Installed module graph and active provider model from Phase 1/2.
- `crates/core/extension/plugin/src/manifest.rs` — Normalized manifest/provider metadata and entrypoint declarations.
- `crates/core/extension/service/src/contract.rs` — `interface.toml` parser for state fields, methods, types, and capabilities.
- `crates/core/extension/service/src/interface.rs` — Interface/provider registry and canonical interface naming.
- `crates/core/extension/service/src/registry.rs` — Legacy typed registry, useful mainly as a migration boundary.

### Existing Provider And Interface Fixtures
- `packages/plugins/backend/core/audio-interface/plugin.json` — Audio interface package manifest.
- `packages/plugins/backend/core/audio-interface/interface.toml` — Audio state fields, command methods, and capability contract.
- `packages/plugins/backend/core/network-interface/interface.toml` — Network state fields, command methods, and capability contract.
- `packages/plugins/backend/core/pipewire-audio/plugin.json` — Backend provider package id, capabilities, dependencies, and `provides` metadata.
- `packages/plugins/backend/core/pipewire-audio/src/main.luau` — Current emitted-state and command-handler provider implementation to migrate toward exported `state`.
- `packages/plugins/backend/core/pulseaudio-audio/src/main.luau` — Alternative audio provider for active-provider resolution coverage.
- `packages/plugins/backend/core/networkmanager-network/src/main.luau` — Network provider to validate generic non-audio behavior.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `BackendScriptContext` already owns the Luau VM and can snapshot globals or a service table after lifecycle callbacks.
- `BackendServiceUpdate` already carries `service`, `source_plugin`, and JSON payload, which can evolve into interface-level latest-state propagation with provider metadata.
- `Shell.latest_service_events` already caches latest service updates by service/interface and replays them to components.
- `InterfaceContract` already parses `state_fields`, `methods`, and `capabilities` from `interface.toml`; Phase 4 can use this for warning-level validation.
- Existing provider scripts already use `on_command_*` handlers and `mesh.service.payload()`, giving a clear migration target for result-table support.

### Established Patterns
- Rust core remains generic wiring and must not branch on service names or implement audio/network/power behavior.
- Backend behavior belongs in Luau modules under `packages/plugins/backend/core/**/src/main.luau`.
- Service proxy command methods require `service.<name>.control`; read capability remains state-only.
- Frontend and backend code should depend on interface/service imports when provider swapping matters.
- Phase 2 strict active provider semantics still apply: no hidden provider fallback when a configured provider fails.

### Integration Points
- `BackendScriptContext::run_poll` and `run_command` are the natural points to snapshot exported `state` after callbacks.
- `spawn_backend_service()` is the bridge from backend runtime state snapshots and command handler results into shell-facing events.
- `Shell::dispatch_service_command()` is the current generic command path and should become result-aware without service-specific branching.
- Interface resolution and package graph active-provider metadata should drive `require("@mesh/audio")` resolution.
- Existing bundled interface TOML files are the validation source for command names and warning-level state shape checks.

</code_context>

<specifics>
## Specific Ideas

- The user wants a simpler backend structure than explicit state emission: exported state should behave like module-level reactive state.
- User wording: "backend modules export only special variable" and "when we define a state at a top level without local keyword it will automatically be treated as a module state."
- Desired consumer shape: `local module = require("@mesh/<module_name>")`, then use `module.state`.
- Preferred normal import shape is interface-oriented, for example `require("@mesh/audio")` resolves to the active provider.
- Command methods should both return result tables and update reactive state.
- Latest public state should be per interface, with provider id tracked as metadata.

</specifics>

<deferred>
## Deferred Ideas

- Lua package manager support so modules can import Lua libraries through package-managed dependencies.
- Remote module/package distribution, signing, sandboxing, and third-party dependency trust policy.
- Full Phase 5 diagnostics/reference backend proof and author-facing documentation.

</deferred>

---

*Phase: 4-Service Provider Contract*
*Context gathered: 2026-05-03*
