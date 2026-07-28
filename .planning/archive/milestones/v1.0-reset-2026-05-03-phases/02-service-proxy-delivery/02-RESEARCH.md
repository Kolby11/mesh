# Phase 2 Research: Service Proxy Delivery

**Phase:** 02 — Service Proxy Delivery
**Date:** 2026-05-02
**Status:** Research complete
**Context note:** No `02-CONTEXT.md` exists for this phase. This research assumes the default workflow choice of continuing from roadmap, requirements, and codebase evidence only.

## Research Question

What does the planner need to know to make `require("@mesh/<service>")` the reliable frontend/backend bridge for live state, commands, automatic reactive invalidation, and visible contract diagnostics without service-specific callback APIs?

## Current State

### Frontend Proxy Runtime

`crates/core/runtime/scripting/src/context.rs` already contains most of the proxy skeleton:

- `require("@mesh/<service>@range")` normalizes interface names, checks read capabilities, resolves contract/provider pairs, and returns a Lua proxy table.
- `mesh.service.use(name)` builds the same proxy shape without the `require(...)` import path.
- Proxy tables currently support:
  - field reads through the live `__mesh_svc_<service>` payload table
  - `:bind(field, alias)` reactive bindings
  - `:on_change(fn_or_name)` subscriptions
  - contract method calls that publish `PublishedEvent` channels such as `mesh.audio.set_volume`

Existing tests already cover:

- requiring a proxy successfully
- missing contract/provider failure via `ScriptError::InterfaceUnavailable`
- proxy method publishing for contract methods
- `:on_change(...)` and `:bind(...)` legacy behavior

Important architectural note: frontend and backend scripts are hosted in separate Luau runtimes. They do not share direct Lua references with each other. The safe shared boundary is Rust-owned state that Rust injects into frontend script state on service updates.

### Shell Update Flow

`crates/core/shell/src/shell/component.rs` handles `ServiceEvent::Updated` by:

1. copying the raw payload into script state via `apply_service_update(...)`
2. applying explicit bound globals via `apply_service_bindings(...)`
3. marking the component dirty after the service event
4. calling `call_service_handlers(service_name)` for explicit subscriptions only

This means the codebase already has the backbone of the simpler model:

- backend emits payload
- Rust stores/routes the payload
- frontend state is updated
- the component rerenders

Important gap in the old design: Phase 2 was previously framed around adding more callback APIs (`proxy.on_change(...)` and `on_<service>_update()`), but that cuts against the cleaner separation where element events belong to template elements and service updates are just data invalidation.

### Interface Contracts and Providers

`crates/core/extension/service/src/interface.rs` resolves the highest-priority provider plus matching contract version. Audio, network, power, and media contracts already declare methods and some type shapes in:

- `packages/plugins/backend/core/audio-interface/interface.toml`
- `packages/plugins/backend/core/network-interface/interface.toml`
- `packages/plugins/backend/core/power-interface/interface.toml`
- `packages/plugins/backend/core/media-interface/interface.toml`

Current limitation: those contracts describe methods and events, but they do not yet act as a clear source of truth for the backend-emitted state fields frontend authors actually read (`audio.percent`, `network.connections`, `power.level`, `media.players`, `source_plugin`, unavailable markers, and so on).

### Built-In Frontend Usage

The shipped frontend surfaces prove the transition is incomplete:

- `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` already uses `require("@mesh/audio@>=1.0")` but still derives local UI state through `audio.on_change(...)`
- `packages/plugins/frontend/core/panel/src/main.mesh` uses `require(...)` but still does pull-style reads like `audio.default_output()`
- `packages/plugins/frontend/core/quick-settings/src/main.mesh` and several children still rely on legacy `mesh.service.bind(...)` / `mesh.service.on(...)`

This means the runtime and bundled plugin examples are not yet aligned around one reliable proxy pattern. Today they mix:

- direct proxy reads
- legacy service subscriptions
- proxy callback subscriptions
- method-style pull reads such as `audio.default_output()`

Phase 2 should collapse that into one model: direct proxy reads plus automatic rerender on service updates.

### Diagnostics Story

The runtime throws `ScriptError::InterfaceUnavailable` and `ScriptError::CapabilityDenied` when a contract/provider lookup fails, which is visible if the script does not catch the error.

Important gap: Phase 2 requires visible diagnostics for missing or invalid service contracts. Today, a frontend can hide the failure by wrapping `require(...)` in `pcall(...)`, and there is no guaranteed plugin-visible diagnostic emitted alongside that failure path.

### Reactive Architecture Direction

The cleaner architecture for this phase is:

1. Backend services emit JSON-compatible state into Rust-owned service state.
2. Rust injects the latest service payload into each readable frontend runtime.
3. Frontend components that consume service state are marked dirty.
4. Templates or script helper functions read proxy state on rerender.
5. Service proxies expose reads and commands, not event APIs.

This keeps event semantics narrow:

- `onclick`, `onhover`, `onchange`, and similar handlers belong to template elements
- service updates are not script event APIs; they are reactive state invalidations

Open ergonomic question: some built-in surfaces currently derive local labels/icons from callbacks. Phase 2 should either move those derivations into direct template/script helpers or introduce a small computed-state helper that is not itself a service event API.

## Recommended Plan Shape

Three sequential plans are enough:

1. **Reactive proxy runtime and diagnostics** in `mesh-core-scripting` and shell component update flow.
2. **Contract metadata and editor/runtime contract alignment** in `mesh-core-service`, interface TOMLs, docs, and LSP knowledge.
3. **Bundled surface adoption and end-to-end proof** in panel and quick-settings plugins with shell-facing integration coverage.

This split keeps the first plan focused on removing service event semantics from the public proxy model, the second on source-of-truth metadata and docs, and the third on proving that real built-in surfaces use the finalized reactive proxy path.

## Files to Read First

- `crates/core/runtime/scripting/src/context.rs`
- `crates/core/shell/src/shell/component.rs`
- `crates/core/shell/src/shell/service.rs`
- `crates/core/extension/service/src/interface.rs`
- `crates/core/extension/service/src/contract.rs`
- `crates/tools/lsp/src/knowledge/mesh_api.rs`
- `packages/plugins/backend/core/audio-interface/interface.toml`
- `packages/plugins/backend/core/network-interface/interface.toml`
- `packages/plugins/backend/core/power-interface/interface.toml`
- `packages/plugins/backend/core/media-interface/interface.toml`
- `packages/plugins/frontend/core/panel/src/main.mesh`
- `packages/plugins/frontend/core/quick-settings/src/main.mesh`
- `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh`
- `packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh`
- `packages/plugins/frontend/core/quick-settings/src/components/bluetooth-section.mesh`

## Implementation Guidance

### Proxy Runtime

- Preserve `require("@mesh/<service>@range")` as the canonical interface lookup path.
- Keep `mesh.service.use(name)` working, but plan Phase 2 success around `require(...)`.
- Add direct regression coverage for:
  - raw proxy field reads after service updates
  - component invalidation/rerender after service updates
  - visible diagnostics when contract/provider resolution fails
- Stop planning around `proxy.on_change(...)` and `on_<service>_update()` as required public APIs for service updates.

### Service State Exposure

- Continue exposing the full backend payload as a Lua table under `__mesh_svc_<service>` so field reads stay generic.
- Favor tests and docs that prove the expected field names for real services (`percent`, `muted`, `connections`, `devices`, `available`, `level`, `players`, `source_plugin`).
- Treat unavailable emissions as part of the observable proxy contract, not an implementation detail.
- Prefer Rust-owned shared service state over any design that implies direct Lua references across backend and frontend runtimes.

### Commands

- Keep proxy method calls translating to `CoreRequest::ServiceCommand` through `PublishedEvent` channels.
- Add end-to-end tests proving shell dispatch still reaches the backend command channel for declared contract methods.
- Use contract metadata as the source of truth for which methods appear on the proxy.

### Diagnostics

- Missing provider, missing contract, invalid version match, and invalid contract metadata should surface visible diagnostics even when a script handles the Lua error path with `pcall(...)`.
- The diagnostic must include enough detail to identify the requested interface, requested version range, and calling plugin.

### Built-In Surfaces

- Migrate bundled panel and quick-settings scripts toward direct proxy reads plus automatic rerender.
- Keep plugin-local labels, icons, and formatting logic inside the `.mesh` scripts; Rust should remain the generic bridge.
- Avoid adding new service-specific callback APIs just to support bundled surface ergonomics.

## Validation Architecture

### Automated Checks

- `cargo test -p mesh-core-scripting context`
- `cargo test -p mesh-core-service`
- `cargo test -p mesh-core-shell`

### Required Coverage

- `require("@mesh/audio@>=1.0")` returns a proxy when both contract and provider exist.
- Proxy field reads reflect the latest emitted payload after a service update.
- Service updates mark consuming frontend components dirty so rerender sees the latest proxy state.
- Proxy contract methods publish or dispatch the expected backend command.
- Missing or invalid contract/provider lookups surface visible diagnostics.
- Audio, network, power, and media contracts document their state fields and commands in a form the runtime/docs/LSP can all reuse.

## Risks

- Removing service callback expectations from the public API may expose gaps in current surface ergonomics for derived labels/icons.
- Contract metadata can drift again if runtime, docs, and LSP each keep their own field lists.
- Surface migrations may accidentally break current quick-settings behavior if proxy docs and actual payload shapes still disagree.

## RESEARCH COMPLETE

Phase 2 is ready for planning.
