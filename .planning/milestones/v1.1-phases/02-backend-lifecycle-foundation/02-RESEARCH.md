# Phase 2: Backend Lifecycle Foundation - Research

**Researched:** 2026-05-03
**Phase:** 02-backend-lifecycle-foundation

## Research Complete

Phase 2 should treat the Phase 1 installed-module graph as the source of truth for backend runtime eligibility. The current shell path still discovers legacy backend plugin directories, groups candidates by service name, sorts by priority, and starts the highest-priority readable script. That behavior conflicts with Phase 2 decisions D-01 through D-06 because it can start fallback providers, ignore explicit provider choices, and register command handlers before the selected runtime has actually passed validation and `init()`.

## Current Implementation

- `crates/core/extension/plugin/src/package.rs` already provides `InstalledModuleGraph`, `active_provider(interface)`, `backend_providers_for_interface(interface)`, `requirements_for_frontend(module_id)`, and `unresolved_backend_requirements()`.
- `config/package.json` selects `mesh.audio` -> `@mesh/pipewire-audio` while keeping `@mesh/pulseaudio-audio` installed and enabled as an alternative provider.
- `crates/core/shell/src/shell/mod.rs::spawn_backend_plugins()` still uses `PluginInstance` discovery, `Manifest::primary_service()`, binary availability checks, priority sorting, `BackendServiceCandidate`, and service-name grouping.
- `spawn_backend_plugins()` currently inserts `service_handlers[interface]` before verifying the script can be read or initialized.
- `crates/core/runtime/backend/src/lib.rs::spawn_backend_service()` loads the script, calls `init()`, starts an immediate polling interval, refreshes interval changes, dispatches commands, and exits when channels close.
- `BackendScriptContext::call_init()` returns `BackendScriptError::MissingEntrypoint` or runtime errors, but `run_poll()` and `run_command()` currently log failures and return `Option<JsonValue>`, so the backend runtime cannot count failures or report lifecycle-stage-specific failure events.
- `DiagnosticsCollector` can register plugin handles and deduplicate frontend handler/icon diagnostics, but there is no backend lifecycle diagnostic record keyed by provider and lifecycle stage yet.

## Recommended Architecture

### 1. Graph-Driven Selection

Add a shell-owned resolver that loads `config/package.json` through `load_installed_module_graph()` and derives exactly one backend launch candidate for each explicit active provider entry:

- For each active provider, require an enabled backend module node.
- Do not use `fallback_provider()` for normal Phase 2 startup.
- If an interface has enabled providers but no explicit active provider, record status/diagnostic `no_active_provider` and start nothing.
- If a frontend requirement references an interface with no enabled provider or no active provider, record an unmet requirement/status and start nothing automatically.
- Keep priority fallback available only as legacy compatibility code when no package graph can be loaded, and make that branch explicit in tests/logs.

The resolver should produce a concrete candidate shape that includes:

- `module_id`
- `interface`
- `service_name`
- `entrypoint_path`
- `capabilities`
- `settings`
- provider metadata from `BackendProviderNode`

### 2. Manifest and Entrypoint Validation Before Launch

Before spawning a backend task, validate:

- Module kind is `ModuleKind::Backend`.
- Backend module is enabled.
- The module package declares at least one provided interface matching the selected active provider interface.
- A readable `mesh.entrypoints.main` exists.
- Required binaries are available using the existing `binary_exists()` helper.

Invalid candidates should not register command handlers and should not create runtime tasks. They should record lifecycle status such as `invalid_manifest`, `missing_entrypoint`, `missing_binary`, or `no_active_provider`.

### 3. Runtime Lifecycle Events

Change the runtime contract so `spawn_backend_service()` can report lifecycle transitions in addition to service payload updates. A small enum is enough:

- `BackendServiceEvent::Started`
- `BackendServiceEvent::InitFailed { message }`
- `BackendServiceEvent::PollFailed { count, message }`
- `BackendServiceEvent::Failed { stage, message }`
- `BackendServiceEvent::Stopped`
- `BackendServiceEvent::Update(BackendServiceUpdate)`

Alternatively, extend `BackendServiceUpdate` only if doing so remains typed and testable. A distinct enum is cleaner because failures are not service state payloads.

`init()` must run exactly once after script load and before the runtime reads from `cmd_rx` or polls. This is already true structurally, but tests should assert that commands sent before init completion are not dispatched when init fails.

### 4. Poll Failure Threshold and Interval Changes

Make `BackendScriptContext::run_poll()` return `Result<Option<JsonValue>, BackendScriptError>` instead of swallowing handler errors. Then the runtime can:

- Count consecutive poll failures.
- Emit `PollFailed` events with count/stage.
- Stop after a constant threshold such as `MAX_CONSECUTIVE_POLL_FAILURES: u32 = 3`.
- Reset the failure counter after a successful poll.
- Continue to honor `mesh.service.set_poll_interval(ms)` after successful poll or command calls.

The existing `spawn_backend_service_applies_runtime_poll_interval_changes` test is a good base; add a failure-threshold test with a script whose `on_poll()` errors repeatedly.

### 5. Stop/Restart Ownership

The shell should own backend runtime handles instead of fire-and-forget tasks. Introduce a `BackendRuntimeHandle`/`BackendRuntimeSlot` in shell state with:

- interface
- provider module id
- command sender
- task abort handle or join handle
- lifecycle status

Starting a provider for an interface must first close/abort the old runtime and remove the old command sender. Only after the old slot is stopped should the replacement handler be inserted. This directly proves "no stale poll loops or command receivers."

### 6. Diagnostics and Status

Add a backend lifecycle status model near shell lifecycle code, with exact statuses:

- `NoActiveProvider`
- `InvalidManifest`
- `MissingEntrypoint`
- `MissingBinary`
- `InitFailed`
- `Running`
- `PollFailed`
- `Stopped`

Statuses should include `interface`, `provider_id` where known, and `stage`. Diagnostics should be deduplicated by `(provider_id, stage, message)` and count/timestamp repeated failures instead of registering unbounded duplicate errors.

## Implementation Order

1. Add resolver/status types and graph-driven candidate derivation in shell, with tests against `config/package.json` and synthetic missing-provider cases.
2. Add typed backend lifecycle events and convert `run_poll()`/`run_command()` to preserve errors where the runtime needs to act on them.
3. Add shell-owned runtime slots with stop/restart cleanup and handler insertion only after successful validation.
4. Add diagnostics/status publication and coverage tests proving all BPLUG requirements and CONTEXT decisions remain visible.

## Validation Architecture

### Automated Checks

- `nix develop -c cargo test -p mesh-core-plugin installed_module_graph`
- `nix develop -c cargo test -p mesh-core-shell backend_lifecycle`
- `nix develop -c cargo test -p mesh-core-backend spawn_backend_service`
- `nix develop -c cargo test -p mesh-core-scripting backend`

### Required Test Targets

- Graph resolver returns only the explicit active provider from `config/package.json`.
- Graph resolver returns no candidate and records `NoActiveProvider` when an interface has providers but no explicit active provider.
- Disabled backend modules are excluded from runtime creation.
- Missing or unreadable service entrypoint creates `MissingEntrypoint` and inserts no command sender.
- `init()` is called once before poll and command dispatch.
- `init()` failure emits `InitFailed`, creates no polling loop, and leaves no active command sender.
- Poll interval changes take effect after a successful poll or command.
- Three consecutive poll failures stop the runtime and emit `PollFailed`/failed status.
- Replacing a runtime for the same interface closes the old command receiver before inserting the new sender.
- Repeated lifecycle diagnostics are deduplicated by provider and stage.

## Open Risks

- Shell tests may need small public or `pub(crate)` seams around candidate resolution and runtime slot management because `Shell::run()` is too broad for unit tests.
- Legacy fallback behavior may still be needed for repo-local startup until graph loading is fully wired. If retained, it must be visibly marked as compatibility and must not override explicit package graph provider choice.
- Backend diagnostic status can be implemented with existing `Diagnostics` handles, but lifecycle-specific count/timestamp data may need a small new structure to avoid overloading frontend handler diagnostics.

## Research Complete
