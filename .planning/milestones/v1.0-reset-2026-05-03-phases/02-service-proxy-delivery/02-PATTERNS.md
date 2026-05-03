# Phase 2 Pattern Map: Service Proxy Delivery

**Phase:** 02 — Service Proxy Delivery
**Generated:** 2026-05-02

## Closest Existing Analogs

| Planned Area | Closest Existing Analog | Pattern to Reuse |
|--------------|-------------------------|------------------|
| Frontend `require("@mesh/<service>")` lookup | `crates/core/runtime/scripting/src/context.rs::install_host_api` | Normalize module names, resolve contract/provider, and build a Lua proxy table under controlled capability checks. |
| Proxy table behavior | `create_service_proxy(...)` in `context.rs` | Use a metatable `__index` function to expose contract methods and live field reads from `__mesh_svc_<service>`, while retiring service-specific callback semantics from the public proxy model. |
| Shell event to script state flow | `crates/core/shell/src/shell/component.rs::handle_service_event` | Apply raw payload to script state and mark the component dirty so rerender observes the latest state without a service callback API. |
| Script event to backend command flow | `crates/core/shell/src/shell/service.rs::script_events_to_requests` | Translate `PublishedEvent` channels like `mesh.audio.set_volume` into `CoreRequest::ServiceCommand`. |
| Contract/provider resolution | `crates/core/extension/service/src/interface.rs::resolve` | Pick the highest-priority provider that satisfies the requested version range. |
| Real proxy consumers | `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` | Use `require(...)`, proxy field reads, and plugin-local derived presentation logic without depending on `proxy.on_change(...)`. |

## File Roles

### `crates/core/runtime/scripting/src/context.rs`

Role: Canonical frontend proxy runtime and unit-test home.

Expected changes:
- tighten proxy diagnostics and lookup behavior
- add direct regression tests for field reads and dirty-state invalidation
- remove or de-emphasize service callback semantics from the public proxy surface
- keep proxy method publishing aligned with contract metadata

### `crates/core/shell/src/shell/component.rs`

Role: Shell-side service update delivery into frontend runtimes.

Expected changes:
- keep diagnostics and dirty-state behavior stable after every service update
- ensure service updates are treated as data invalidation, not script event dispatch

### `crates/core/shell/src/shell/service.rs`

Role: Translation layer from script-published channels to shell requests.

Expected changes:
- preserve the `mesh.<service>.<command>` to `CoreRequest::ServiceCommand` mapping
- add or tighten tests around interface/command payload routing

### `crates/core/extension/service/src/contract.rs`

Role: Parser and typed model for interface contract metadata.

Expected changes:
- load any new state-field or callback metadata needed to document contracts and inform runtime/editor diagnostics

### `packages/plugins/backend/core/*-interface/interface.toml`

Role: Source-of-truth contract packages for audio, network, power, and media.

Expected changes:
- document state fields and commands in a machine-readable form

### `packages/plugins/frontend/core/panel/src/main.mesh`

Role: Built-in top-panel proof of proxy reads and reactive rerender.

Expected changes:
- align live audio/network/power reads with the finalized proxy contract

### `packages/plugins/frontend/core/quick-settings/src/**/*.mesh`

Role: Built-in interactive proof of proxy reads, reactive rerender, and command methods.

Expected changes:
- migrate remaining legacy `mesh.service.bind/on` usage toward the proxy path
- keep plugin-local presentation logic in Luau

## Data Flow to Preserve

```text
backend main.luau
  -> mesh.service.emit(payload)
  -> BackendServiceUpdate
  -> ShellMessage::Service(ServiceEvent::Updated)
  -> ScriptContext state["service"] + __mesh_svc_service
  -> proxy field reads during rerender
  -> PublishedEvent("mesh.service.command")
  -> CoreRequest::ServiceCommand
  -> backend command handler
```

## Implementation Constraints

- Do not move service-specific audio/network/power/media logic into Rust.
- Keep `require("@mesh/<service>")` contract-first and provider-agnostic.
- Preserve existing `mesh.service.use(...)` only as a compatibility helper if needed; Phase 2 should not require proxy or service callback APIs for correctness.
- Treat runtime diagnostics, docs, and LSP knowledge as one contract surface, not three separate truths.
