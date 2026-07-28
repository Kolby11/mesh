---
phase: 05-backend-diagnostics-and-mvp-proof
status: complete
researched: 2026-05-04
requirements: [BDIAG-01, BDIAG-02, BDIAG-03, BDIAG-04, BREF-01, BREF-02, BREF-03]
---

# Phase 05 Research: Backend Diagnostics and MVP Proof

## Research Question

What must be known to plan Phase 05 so backend plugin failures become visible without stale state or diagnostic spam, and a fresh reference provider proves the backend MVP contract end to end?

## Current State

Phase 4 already established the generic backend service contract:

- `BackendScriptContext` snapshots top-level Luau `state` and produces generic command result tables.
- `spawn_backend_service()` emits `Started`, `Update`, `InitFailed`, `PollFailed`, `Failed`, `CommandResult`, and `Stopped` events.
- `Shell` stores `latest_service_state` keyed by canonical interface and tracks runtime statuses per `(interface, provider_id)`.
- `DiagnosticsCollector::record_lifecycle_error()` deduplicates lifecycle failures by exact `(provider_id, stage, message)` tuple.

That is enough for generic runtime wiring, but not enough for Phase 5's visibility bar.

## Key Gaps Found

### 1. Failure visibility does not clear stale public state

`Shell::record_latest_service_state()` only changes public state when a `ServiceEvent::Updated` arrives. `handle_backend_lifecycle()` records runtime status, but it does not replace or clear `latest_service_state` for the active provider when init, poll, command, or load-adjacent failures happen.

Implication: after a provider emits healthy state once, later failures can leave consumers reading stale last-known-good data unless another explicit update replaces it.

### 2. Diagnostic dedup is too granular for repeated poll or command failures

`Diagnostics` currently uses `HashSet<(provider_id, stage, message)>` for lifecycle failures. Repeated failures with changing message text or repeated timestamps create new entries and increment `error_count`, which conflicts with the Phase 5 context decision to dedup by provider plus lifecycle stage and roll up count and timestamp.

### 3. Command result failures are not promoted into a stronger author-facing diagnostic surface

`BackendServiceEvent::CommandResult` is currently traced in the shell bridge, but there is no shell-owned aggregation that ties repeated command failures back to a provider/stage bucket with clear identity and health effects.

### 4. There is no fresh reference backend provider

Existing bundled providers are real integrations (`pipewire-audio`, `networkmanager-network`, `upower-power`) or placeholders (`mpris-media`, `mock-notifications`). The current tree does not contain a purpose-built provider that demonstrates config, logging, polling, exported-state snapshots, and command handling as a clean MVP reference.

### 5. Backend author docs are drifted from the locked architecture

`docs/plugins/backend/core/README.md` still describes:

- `mesh.exec_shell(...)` as an active backend API,
- `mesh.service.emit(...)` as the primary state mechanism,
- provider fallback after init failure,
- runtime-authored `source_plugin` in public state.

Those statements conflict with Phase 2 through Phase 4 decisions and should be corrected as part of the MVP proof note.

## Recommended Reference Provider

Use a **fresh media provider** such as `@mesh/reference-media`.

Why media is the best proof target here:

- `mesh.media` already has an interface contract with stable state fields (`available`, `title`, `artist`, `album`, `state`) and simple mutating commands (`play`, `pause`, `next`, `previous`).
- The existing `mpris-media` backend is still a placeholder, so a new `reference-media` provider is clearly a fresh proof plugin rather than a retrofit of legacy behavior.
- A reference media provider can stay fully local and deterministic: seeded state from config, in-memory track list, poll-driven heartbeat/state refresh, and command handlers that mutate state and return result tables without OS-specific dependencies.
- This keeps Rust generic and makes the reference provider useful to plugin authors because it demonstrates the full MVP contract in a small script.

## Recommended Implementation Approach

### 1. Normalize backend failure stages around state snapshots and runtime lifecycle

Keep the current generic event flow, but make failure reporting sharper:

- Treat load, init, poll, command, and exported-state snapshot/serialization failures as distinct lifecycle stages.
- Where `take_service_state_snapshot()` or command result conversion fails, preserve the generic error path but propagate the stage explicitly so diagnostics can bucket it accurately.
- Preserve non-crashing behavior where recovery is possible: repeated poll failures degrade health first, then stop the runtime after the existing threshold.

### 2. Clear public interface state immediately when the active provider is known failing

The shell should own the public-state truth transition:

- When the active provider hits a terminal or visibility-relevant failure (`init_failed`, terminal `failed`, repeated `poll_failed`, command/state-snapshot failure if it invalidates exported state), replace `latest_service_state[interface]` with an unavailable/error payload instead of keeping stale data.
- The replacement payload should be interface-safe and JSON-compatible. The exact shape can stay planner discretion, but it must at least set `available = false` where that field exists and avoid inventing service-specific Rust behavior.
- `mesh.theme` remains a special shell-authored interface already handled inside shell state sync; Phase 5 should not regress that.

### 3. Replace lifecycle failure `HashSet` dedup with provider-plus-stage buckets

The diagnostics layer should move from exact-message dedup to bucketed aggregation:

- Key repeated failures by `(provider_id, stage)`.
- Store latest message, occurrence count, and last-seen timestamp.
- Only the first occurrence should increment the unique diagnostic count; repeats should update metadata rather than create new entries.
- Keep plugin identity in the bucket and health message so authors can trace the failing provider quickly.

### 4. Surface failures through debug and verification-friendly structures

The existing debug/runtime status machinery in the shell is close to sufficient:

- `backend_runtime_statuses` already tracks `(interface, provider_id) -> status/message`.
- Extend debug and verification-facing records so repeated failure counts and last-seen timing are observable in tests.
- Prefer augmenting the existing runtime status snapshot path over inventing a second shell-only diagnostics structure.

### 5. Build the reference proof around a config-driven in-memory media provider

Recommended provider behavior:

- Top-level `state` initialized from config defaults or a seeded track list.
- `init()` logs provider startup and poll interval selection.
- `on_poll()` refreshes or rotates a simple deterministic state heartbeat without external commands.
- `on_command_play`, `on_command_pause`, `on_command_next`, and `on_command_previous` mutate `state`, log the transition, and return `{ ok = true }` or `{ ok = false, error = "..." }`.
- A small config block controls initial playlist/title/artist or poll cadence so `mesh.config()` is exercised visibly.

This proves config, logging, polling, exported-state snapshots, and command handling in one provider without coupling the proof to real desktop services.

## Planning Implications

Recommended plan order:

1. Tighten runtime failure-stage reporting in scripting/backend runtime.
2. Update shell diagnostics aggregation and stale-state clearing semantics.
3. Add the fresh `reference-media` backend provider and automated proof tests.
4. Write the backend MVP reference note and correct drifted backend plugin docs.

The shell state-clearing work should depend on the sharper runtime failure signals. The reference plugin should depend on that diagnostics work so failure-path tests can assert the finalized behavior.

## Validation Architecture

### Automated Test Areas

- `mesh-core-scripting`: exported-state snapshot failures, command-result failure conversion, bundled script host API proof.
- `mesh-core-backend`: runtime event sequencing, repeated poll failure behavior, reference provider command/result paths.
- `mesh-core-shell`: active-provider stale-state clearing, backend lifecycle status propagation, debug snapshot visibility.
- `mesh-core-diagnostics`: provider-plus-stage dedup buckets with count/timestamp updates.
- Static docs/fixture checks: reference provider docs exist and backend docs stop advertising removed behavior such as `mesh.exec_shell` and hidden provider fallback.

### Suggested Commands

- `nix develop -c cargo test -p mesh-core-scripting backend_command_result`
- `nix develop -c cargo test -p mesh-core-backend backend_command_result`
- `nix develop -c cargo test -p mesh-core-shell backend_lifecycle`
- `nix develop -c cargo test -p mesh-core-diagnostics lifecycle`

### Landmines

- Do not reintroduce provider fallback. Phase 2 locked explicit active-provider selection with no hidden fallback.
- Do not keep stale public service state after a known provider failure just because the last payload was valid.
- Do not branch on specific services in Rust when synthesizing unavailable/error state. The logic must stay contract-driven and generic.
- Do not prove the MVP contract by expanding an existing placeholder backend. The context explicitly chose a fresh provider.
- Do not leave docs claiming `mesh.exec_shell` or `mesh.service.emit` as the preferred author path.

## Research Complete

Phase 5 is ready for planning. No external research is required; the remaining work is project-local runtime semantics, diagnostics aggregation, and a fresh bundled proof provider.
