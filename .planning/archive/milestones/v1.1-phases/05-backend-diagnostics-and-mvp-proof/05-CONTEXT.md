# Phase 5: Backend Diagnostics and MVP Proof - Context

**Gathered:** 2026-05-04
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase makes backend provider failures visible through the generic runtime and diagnostics pipeline, and proves the backend MVP contract with a fresh reference backend plugin, broader lifecycle and diagnostics coverage, and stronger author-facing guidance.

This phase does not add new product capabilities such as WiFi or Bluetooth behavior, frontend shell features, remote package distribution, sandboxing/signing, or service-specific Rust branches. If the proof touches connectivity concepts, it must do so through the backend module/interface contract rather than shell-owned special cases.

</domain>

<decisions>
## Implementation Decisions

### Reference Plugin Shape
- **D-01:** Phase 5 should use a brand-new backend provider as the MVP proof target rather than upgrading an existing placeholder or legacy provider.
- **D-02:** The fresh reference plugin should stay intentionally simple; broader proof coverage should come from tests, diagnostics behavior, and documentation coverage rather than by making the reference plugin itself complex.

### Failure Visibility Contract
- **D-03:** When a backend provider fails during load, init, poll, command handling, exported state serialization, or related runtime stages, the public interface state should clear immediately to unavailable or error instead of preserving stale last-known-good state.
- **D-04:** Failure visibility should favor honest runtime state over continuity. Consumers should not keep showing previously emitted state once the active provider is known to be failing.

### Diagnostic Dedup and Escalation
- **D-05:** Repeated backend failures should deduplicate by provider plus lifecycle stage.
- **D-06:** Repeated failures update count and timestamp on the existing diagnostic entry rather than creating a new entry for each message variant or poll cycle.

### MVP Proof Scope
- **D-07:** Phase 5 should deliver a broad proof bar: a fresh reference backend plugin, stronger lifecycle and diagnostics coverage, and stronger author-facing documentation than a minimal happy-path proof.
- **D-08:** The broader proof should cover more than one behavioral path, including failure-path coverage, while preserving the existing architectural rule that Rust core remains generic wiring.

### Connectivity Scope Guard
- **D-09:** WiFi and Bluetooth are product capabilities that should live as a backend module/interface concern, not as shell-specific logic. Their detailed design is deferred to a later phase and should not expand Phase 5 scope.

### the agent's Discretion
- Planner/researcher may choose the exact reference provider domain, as long as it is a fresh backend provider and not a retrofit of an existing placeholder plugin.
- Planner/researcher may choose the exact unavailable/error public state shape, as long as stale state is cleared immediately once failure is known.
- Planner/researcher may choose the exact diagnostic storage/update model for count and timestamp tracking, as long as repeated failures collapse into a provider-plus-stage bucket.
- Planner/researcher may choose the exact mix of tests and documentation artifacts that satisfy the broad proof bar, as long as both lifecycle/diagnostic behavior and plugin-author guidance are materially stronger than a minimal proof.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Phase Scope
- `.planning/PROJECT.md` — Active v1.1 milestone goals, locked architecture, and current backend MVP priorities.
- `.planning/REQUIREMENTS.md` — Phase 5 requirements BDIAG-01 through BDIAG-04 and BREF-01 through BREF-03.
- `.planning/ROADMAP.md` — Phase 5 goal, dependencies, and success criteria.
- `.planning/STATE.md` — Accumulated decisions from Phases 1-4 and the current workflow state.
- `.planning/phases/02-backend-lifecycle-foundation/02-CONTEXT.md` — Locked lifecycle failure, no-fallback, and diagnostic visibility decisions Phase 5 builds on.
- `.planning/phases/03-backend-host-api-contract/03-CONTEXT.md` — Locked backend host API contract and poll interval/logging behavior relevant to the proof plugin.
- `.planning/phases/04-service-provider-contract/04-CONTEXT.md` — Locked service state, command result, provider metadata, and interface-level state semantics Phase 5 must preserve.

### Existing Codebase Maps
- `.planning/codebase/STACK.md` — Rust/Luau/Tokio/mlua/serde_json stack constraints for backend runtime and tests.
- `.planning/codebase/ARCHITECTURE.md` — Shell/runtime/service layering and the no-service-logic-in-core rule.
- `.planning/codebase/INTEGRATIONS.md` — Existing backend provider domains and current placeholder/provider landscape.

### Diagnostics and Backend Runtime Wiring
- `crates/core/runtime/scripting/src/backend.rs` — `BackendScriptContext`, exported `state` snapshotting, command result plumbing, and backend host API surface.
- `crates/core/runtime/backend/src/lib.rs` — Backend lifecycle loop, failure events, command results, poll failure handling, and update publication.
- `crates/core/foundation/diagnostics/src/lib.rs` — Current diagnostics collector, health states, and lifecycle error dedup shape.
- `crates/core/shell/src/shell/mod.rs` — Backend runtime orchestration, latest service state handling, lifecycle event bridging, and diagnostics integration points.
- `crates/core/shell/src/shell/types.rs` — Shell-facing service event and request types that may reflect unavailable/error state transitions.

### Interface and Provider Contracts
- `crates/core/extension/plugin/src/manifest.rs` — Backend manifest metadata, entrypoints, and provider declarations.
- `crates/core/extension/plugin/src/package.rs` — Installed module graph and active provider model.
- `crates/core/extension/service/src/contract.rs` — Interface contract parsing for methods, capabilities, and state fields.
- `crates/core/extension/service/src/interface.rs` — Interface/provider registry and canonical service naming.

### Existing Provider and Proof Surfaces
- `packages/plugins/backend/core/networkmanager-network/plugin.json` — Existing connectivity provider metadata relevant to the deferred WiFi/Bluetooth module direction.
- `packages/plugins/backend/core/networkmanager-network/src/main.luau` — Current network provider behavior and a likely comparison point for any future connectivity module work.
- `packages/plugins/backend/core/mpris-media/src/main.luau` — Existing placeholder provider that was intentionally not chosen as the Phase 5 reference proof target.
- `packages/plugins/backend/core/mock-notifications/src/main.luau` — Existing placeholder backend entrypoint that was intentionally not chosen as the Phase 5 reference proof target.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `BackendScriptContext` already supports config access, logging, poll interval control, exported `state` snapshotting, and generic command handler execution for a reference proof plugin.
- `spawn_backend_service()` already emits typed lifecycle events (`Started`, `InitFailed`, `PollFailed`, `Failed`, `CommandResult`, `Stopped`) that Phase 5 can extend or reinterpret for stronger diagnostics behavior.
- `DiagnosticsCollector` and `Diagnostics::record_lifecycle_error()` already provide a dedup-oriented base that can evolve toward provider-plus-stage count/timestamp tracking.
- Existing backend plugin fixtures under `packages/plugins/backend/core/**` provide comparison targets and failure-path fixtures without requiring service-specific Rust logic.

### Established Patterns
- Rust core remains a generic runtime and diagnostics layer; all service-specific behavior stays in Luau backend modules.
- Public service state is interface-level and should not continue serving stale provider state once the provider is known to be failing.
- Explicit active provider selection and no hidden fallback still apply when a provider fails.
- Backend proof work should validate the contract through tests and docs, not by introducing shell-special-case logic.

### Integration Points
- `BackendScriptContext::call_init()`, `run_poll()`, `run_command_with_result()`, and `take_service_state_snapshot()` are the immediate runtime points where failure visibility and exported-state clearing rules attach.
- `spawn_backend_service()` is the main bridge for lifecycle failure events, command results, and update suppression or replacement when a provider becomes unavailable/error.
- Diagnostics aggregation in `crates/core/foundation/diagnostics/src/lib.rs` and the shell integration path in `crates/core/shell/src/shell/mod.rs` are the likely places to implement provider-plus-stage dedup with count/timestamp updates.
- New proof fixtures should live alongside other backend plugins under `packages/plugins/backend/core/` and be exercised by Rust-side contract tests.

</code_context>

<specifics>
## Specific Ideas

- The user explicitly wants Phase 5 to use a fresh proof plugin rather than retrofitting an existing placeholder provider.
- The user explicitly prefers honest failure visibility: unavailable/error public state is better than stale last-known-good state.
- The user explicitly wants repeated failures grouped by provider plus stage, with count/timestamp updates instead of message-by-message spam.
- The user wants a broad proof bar rather than a minimal smoke test.
- The user noted that WiFi and Bluetooth should be a module, but chose to defer that discussion to a later phase.

</specifics>

<deferred>
## Deferred Ideas

- Detailed WiFi/Bluetooth module design and scope belong in a later dedicated phase, not Phase 5.
- Any connectivity-focused proof or new product capability work should be evaluated later as backend module/interface work rather than diagnostics scope.

</deferred>

---

*Phase: 5-Backend Diagnostics and MVP Proof*
*Context gathered: 2026-05-04*
