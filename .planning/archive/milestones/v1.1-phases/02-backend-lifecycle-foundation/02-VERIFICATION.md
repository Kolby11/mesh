---
phase: 02-backend-lifecycle-foundation
status: passed
verified: 2026-05-03
requirements: [BPLUG-01, BPLUG-02, BPLUG-03, BPLUG-04, BPLUG-05]
---

# Verification: Phase 02

## Goal

Use the installed-plugin graph to make backend plugin discovery, manifest validation, runtime creation, initialization, polling, and stop/restart behavior deterministic.

## Result

Passed. Phase 02 now provides:

- Graph-driven backend launch candidate resolution from `InstalledModuleGraph::active_provider`.
- Pre-launch backend validation statuses for invalid manifests, missing entrypoints, missing binaries, no active provider, and unmet frontend backend requirements.
- Typed backend runtime lifecycle events for start, update, init failure, poll failure, terminal failure, and stop.
- `init()` gating before polling and command dispatch.
- Poll failure reporting plus stop behavior after three consecutive poll failures.
- Shell-owned runtime slots keyed by backend interface, with replacement/stop cleanup for command handlers and task handles.
- Lifecycle status and deduplicated diagnostics surfaced through debug snapshots.

## Requirement Coverage

- **BPLUG-01:** Covered by graph-driven provider validation and pre-launch `missing_entrypoint`, `missing_binary`, and `invalid_manifest` statuses in `crates/core/shell/src/shell/mod.rs`.
- **BPLUG-02:** Covered by explicit active provider selection, disabled-provider exclusion, one `backend_runtimes` slot per interface, and no graph fallback provider launch.
- **BPLUG-03:** Covered by `spawn_backend_service` calling `call_init()` before emitting `Started`, creating the poll interval, or accepting command dispatch.
- **BPLUG-04:** Covered by `run_poll()` error propagation, runtime poll interval refresh, and `MAX_CONSECUTIVE_POLL_FAILURES`.
- **BPLUG-05:** Covered by `stop_backend_runtime`, `replace_backend_runtime`, `ShellMessage::BackendLifecycle`, and cleanup tests for replacement, init failure, failure, and transient poll failure replacement.

## Checks Run

- `nix develop -c cargo test -p mesh-core-plugin installed_module_graph` - passed.
- `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` - passed.
- `nix develop -c cargo test -p mesh-core-backend spawn_backend_service` - passed.
- `nix develop -c cargo test -p mesh-core-scripting backend` - passed.
- `nix develop -c cargo test -p mesh-core-diagnostics lifecycle` - passed.
- Phase 01 regression: `nix develop -c cargo test -p mesh-core-plugin module_package` - passed.
- Phase 01 regression: `nix develop -c cargo test -p mesh-core-config -p mesh-core-theme` - passed.
- Schema drift: `gsd-sdk query verify.schema-drift 02` reported `drift_detected: false`.
- Static checks found expected lifecycle symbols: `BackendRuntimeStatus`, `BackendLifecycle`, `stop_backend_runtime`, `replace_backend_runtime`, `record_lifecycle_error`, `MAX_CONSECUTIVE_POLL_FAILURES`, `InitFailed`, `PollFailed`, and `Stopped`.

## Review Gate

`02-REVIEW.md` status is clean. The review gate caught and fixed one lifecycle status edge case before final verification: transient `poll_failed` no longer suppresses a later replacement `stopped` status.

## Residual Risk

The validation strategy lists live shell startup with real PipeWire/PulseAudio availability as manual-only because it depends on host services and installed binaries. Automated tests cover the deterministic lifecycle, provider selection, failure, cleanup, and diagnostic paths needed for this phase.
