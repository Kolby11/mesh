# Phase 2: Backend Lifecycle Foundation - Context

**Gathered:** 2026-05-03
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase consumes the Phase 1 installed module graph to make backend provider lifecycle deterministic. It decides which backend providers are eligible to launch, validates backend service entrypoints before launch, creates exactly one runtime for each explicit active provider, runs `init()` before polling or command dispatch, manages poll loops, and guarantees stop/failure cleanup does not leave stale tasks or command receivers behind.

This phase does not implement package download, auto-install, generic provider fallback, lazy backend activation, frontend UI for selecting providers, or the later host API/service contract/diagnostics phases beyond the lifecycle status and diagnostics needed to make runtime decisions visible.

</domain>

<decisions>
## Implementation Decisions

### Provider Selection and Runtime Eligibility
- **D-01:** There is no product concept of a generic fallback service. Fallback/priority selection is only legacy compatibility behavior and should not be treated as the normal Phase 2 lifecycle model.
- **D-02:** If no explicit active provider is configured for an interface, Phase 2 should start no backend provider for that interface and surface "no active provider selected" through diagnostics/runtime status.
- **D-03:** Explicit provider choice is strict. If the configured provider cannot launch, the shell must not silently fall back to another provider.
- **D-04:** Disabled backend modules are invisible to runtime creation. If a frontend or package requirement depends on a disabled provider/interface, that becomes an unmet requirement/status problem, not an auto-enable trigger.
- **D-05:** Frontend modules declare required backend interfaces. Backend modules declare provided interfaces. Users should not be able to treat a frontend module as complete unless its required backend interface has an installed, enabled, explicitly active provider.
- **D-06:** Phase 2 should validate required backend interfaces and provider selections, but must not auto-install, auto-select, or lazy-load providers. Lazy activation and install flows are deferred.

### Backend Stability Contract
- **D-07:** Backend modules are contract-first. Every backend module must declare a service entrypoint and provided interface(s), and the runtime validates these declarations before launch.
- **D-08:** Custom backend/frontend module flexibility comes from named interface contracts, not ad hoc runtime conventions. A custom frontend should depend on a named interface such as `com.author.feature`; a backend provider supplies that interface.
- **D-09:** Phase 2 should prepare dependency/provider metadata so future package-manager work can install or activate providers, but runtime lifecycle in this phase remains explicit and deterministic.

### Lifecycle Ordering
- **D-10:** Backend script loading must succeed before `init()` runs.
- **D-11:** `init()` runs exactly once per runtime before polling or command dispatch.
- **D-12:** `init()` may emit initial service state. The lifecycle should support plugins using `init()` to prepare or publish initial state.
- **D-13:** Commands may be accepted after successful `init()`, even before the first successful poll, because `init()` is responsible for preparing initial state.
- **D-14:** If `init()` fails, the provider does not poll and does not accept commands.

### Poll Failure and Stop Semantics
- **D-15:** Repeated `on_poll()` failures should stop the backend runtime after a threshold and mark the provider failed.
- **D-16:** Phase 2 should not implement automatic restart. A failed provider stops cleanly and explicit restart behavior belongs to later work unless already available as a manual shell operation.
- **D-17:** The strongest stop/restart guarantee is "no stale tasks or receivers." Stopping or replacing a backend must close its poll loop and command receiver before another runtime for the same provider/interface can start.
- **D-18:** Deterministic cleanup is more important than preserving old service state or optimizing restart speed in Phase 2.

### Failure Visibility
- **D-19:** Provider startup and lifecycle failures should be visible both through diagnostics and through module/provider runtime status.
- **D-20:** Runtime status should be lifecycle-stage-specific, with states such as `invalid_manifest`, `missing_entrypoint`, `init_failed`, `poll_failed`, `stopped`, or equivalent planner-chosen names.
- **D-21:** Repeated lifecycle diagnostics should be deduplicated by provider and lifecycle stage, updating count/timestamp rather than spamming identical messages.

### the agent's Discretion
- Planner may choose the exact Rust type names for lifecycle/runtime status as long as status distinguishes validation, missing entrypoint, init failure, poll failure, stopped, and unavailable/no active provider cases.
- Planner may choose the poll failure threshold and where to store the counter, but repeated failures must eventually stop the runtime and mark the provider failed.
- Planner/researcher should decide how much legacy priority fallback remains for existing fixtures/tests, but it must be clearly transitional and should not override explicit provider strictness.
- Planner may decide whether frontend modules are blocked from mounting or marked degraded when backend requirements are unmet, but the unmet requirement must be explicit and diagnostic-visible.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Phase Scope
- `.planning/PROJECT.md` — Active v1.1 milestone intent and Phase 1 completion state.
- `.planning/REQUIREMENTS.md` — BPLUG-01 through BPLUG-05 requirements and traceability.
- `.planning/ROADMAP.md` — Phase 2 goal, dependencies, and success criteria.
- `.planning/phases/01-plugin-package-manifest-foundation/01-CONTEXT.md` — Locked package graph and module naming decisions that Phase 2 consumes.
- `.planning/phases/01-plugin-package-manifest-foundation/01-VERIFICATION.md` — Verified Phase 1 outputs and residual risk.

### Existing Codebase Maps
- `.planning/codebase/STACK.md` — Rust/Luau/Tokio/mlua stack constraints.
- `.planning/codebase/ARCHITECTURE.md` — Shell/runtime/service layering and the current backend lifecycle path.
- `.planning/codebase/INTEGRATIONS.md` — Existing backend provider categories and system service integrations.

### Phase 1 Package Graph Code
- `crates/core/extension/plugin/src/package.rs` — `RootPackageManifest`, `InstalledModuleGraph`, active providers, backend provider nodes, and graph validation.
- `config/package.json` — Repo-local installed module graph fixture, including explicit active provider selection.
- `config/modules/@mesh/pipewire-audio/package.json` — Example backend module package with `mesh.audio` provider declaration.
- `config/modules/@mesh/pulseaudio-audio/package.json` — Alternative backend module package for the same interface.

### Current Backend Lifecycle Code
- `crates/core/shell/src/shell/mod.rs` — Current backend plugin discovery/spawn path, binary availability skip, priority candidate selection, and service handler channel setup.
- `crates/core/runtime/backend/src/lib.rs` — Current `spawn_backend_service()` lifecycle loop, `init()`, polling, command dispatch, interval refresh, and task shutdown behavior.
- `crates/core/runtime/scripting/src/backend.rs` — Backend Luau context, `init()`, poll, command, and service emit behavior.
- `crates/core/extension/plugin/src/manifest.rs` — Existing backend manifest, service entrypoint, capabilities, dependencies, and provider metadata model.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `InstalledModuleGraph` in `crates/core/extension/plugin/src/package.rs` already exposes enabled backend modules, provider lists per interface, explicit active providers, and unresolved backend requirements.
- Current `spawn_backend_plugins()` in `crates/core/shell/src/shell/mod.rs` already groups backend candidates by service and creates command channels; Phase 2 should replace/feed this with graph-driven explicit active provider selection.
- `spawn_backend_service()` in `crates/core/runtime/backend/src/lib.rs` already loads script, calls `init()`, polls on an interval, dispatches commands, emits state, and exits when channels close.
- Backend package fixtures under `config/modules/@mesh/*/package.json` provide graph-level test material separate from legacy `plugin.json` fixtures.

### Established Patterns
- Rust core is a lifecycle/wiring layer. It must not add service-specific logic or system commands.
- Backend service logic lives in Luau modules under `packages/plugins/backend/core/**/src/main.luau`.
- Provider declarations and service contracts are manifest/interface metadata, not hard-coded shell branches.
- Existing priority fallback exists in shell startup, but Phase 2 should treat it as compatibility rather than the intended provider model.
- Current backend runtime exits on script load/init failure and currently continues polling unless the task/channel exits; Phase 2 needs explicit lifecycle status and stop semantics.

### Integration Points
- Shell startup should load the installed module graph and derive eligible backend provider runtimes from explicit active providers.
- Runtime creation should validate backend module kind, enabled state, service entrypoint readability, required binaries, and provided interface match before `spawn_backend_service()`.
- Service handler registration should align with the active provider runtime so commands cannot target disabled, missing, or failed providers.
- Poll failure counting/status should live near the backend runtime loop or the shell task wrapper that receives `BackendServiceUpdate` and lifecycle events.
- Diagnostics/status should include module/provider identity, interface, and lifecycle stage.

</code_context>

<specifics>
## Specific Ideas

- The user explicitly rejected relying on fallback as the normal provider model.
- The user wants enough best-practice structure that custom frontend/backend modules do not scatter functionality across incompatible approaches.
- A custom frontend module should declare a required backend interface; a custom backend module should provide that interface.
- Runtime lifecycle should not implement lazy backend provider activation in Phase 2, but the metadata should be suitable for a future lazy activation or package install flow.
- `init()` is expected to handle initial state, so commands can be accepted after successful `init()`.

</specifics>

<deferred>
## Deferred Ideas

- Lazy backend provider activation when a frontend module is enabled or mounted.
- Auto-installing backend providers for frontend module dependencies.
- UI/UX for choosing active providers.
- Automatic restart or restart backoff after provider failure.
- Remote package manager behavior that fetches a provider for a required interface.

</deferred>

---

*Phase: 2-Backend Lifecycle Foundation*
*Context gathered: 2026-05-03*
