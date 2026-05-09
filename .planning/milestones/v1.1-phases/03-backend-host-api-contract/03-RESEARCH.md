---
phase: 03-backend-host-api-contract
status: complete
researched: 2026-05-03
requirements: [BHOST-01, BHOST-02, BHOST-03, BHOST-04, BHOST-05]
---

# Phase 03 Research: Backend Host API Contract

## Research Question

What must be known to plan Phase 03 so backend Luau host APIs become a stable MVP contract without adding service-specific Rust behavior?

## Current State

Most host APIs already exist in `BackendScriptContext`:

- `mesh.exec(program, args?)` returns `{ success, stdout, stderr, code }`.
- `mesh.exec_shell(cmd)` exists and is used heavily by bundled providers.
- `mesh.config()` returns the full plugin settings table.
- `mesh.log(level, msg)` and `mesh.log.info/warn/error/debug(msg)` exist.
- `mesh.service.set_poll_interval(ms)` stores a poll interval that the backend runtime refreshes after callbacks.

Phase 03 is therefore a contract hardening and migration phase, not a greenfield API build.

## Locked Context Decisions

- `mesh.exec` is strict structured-only: `mesh.exec(program, args)`.
- Legacy single-string command splitting must be removed or rejected.
- Process spawn failures and non-zero exits return result tables; scripts branch on `success`.
- `mesh.exec_shell` is not part of the MVP API and bundled providers should migrate away from it.
- `mesh.config()` returns the whole settings table; no lookup helpers this phase.
- `mesh.log` supports `debug`, `info`, `warn`, `error`.
- Invalid log levels warn, not crash.
- Both `mesh.log("info", "msg")` and `mesh.log.info("msg")` styles are public.
- `mesh.service.set_poll_interval(ms)` clamps low values to 50ms and warns when clamped.
- Interval changes take effect after the current callback returns.

## Implementation Research

### `mesh.exec`

`run_exec()` currently accepts `args: Option<&[String]>`. When `args` is `None`, it splits the `program` string by whitespace and spawns the first token. That conflicts with the strict structured-only decision.

The safest plan is:

- Keep the Lua function signature `(program, args): (String, Vec<String>)` or equivalent required-args shape.
- Return a structured failure table when `args` is omitted or malformed if that can be expressed cleanly from the Lua binding, or allow mlua argument/type errors for malformed API usage.
- Preserve structured process outcomes for real process failures: `{ success = false, stdout = "", stderr = "...", code = nil }`.
- Add tests proving `mesh.exec("printf", {"hello"})` works, missing executable returns `success=false`, non-zero exit returns `success=false`, and `mesh.exec("printf hello")` is rejected.

### `mesh.exec_shell`

Bundled providers currently depend on shell pipelines:

- `pipewire-audio`: `wpctl status | awk ...`, `wpctl get-volume`, and shell-combined update commands.
- `pulseaudio-audio`: grouped `pactl` commands and chained update commands.
- `networkmanager-network`: `nmcli`, `bluetoothctl`, and Wi-Fi radio commands.
- `upower-power`: `upower -i ... | awk ...`.

Removing `mesh.exec_shell` requires migrating parsing into Luau and issuing direct commands through `mesh.exec`. The provider migration should keep behavior generic and avoid Rust-side service branches.

Practical rewrites:

- PipeWire: run `mesh.exec("wpctl", {"status"})`, parse sink ids in Lua, then use `mesh.exec("wpctl", {"get-volume", sink_id})`, `mesh.exec("wpctl", {"set-volume", sink_id, "5%+"})`, and `mesh.exec("wpctl", {"set-mute", sink_id, "0"})`.
- PulseAudio: run `mesh.exec("pactl", {"get-sink-volume", "@DEFAULT_SINK@"})` and `mesh.exec("pactl", {"get-sink-mute", "@DEFAULT_SINK@"})` separately; command handlers use structured `pactl` invocations.
- NetworkManager: run `mesh.exec("nmcli", {"-t", "-f", "...", ...})`; Bluetooth remains `mesh.exec("bluetoothctl", {"devices", "Connected"})`.
- UPower: run `mesh.exec("upower", {"-i", "/org/freedesktop/UPower/devices/DisplayDevice"})` and parse text in Lua.

### Config

The existing `mesh.config()` implementation already returns the full settings JSON as a Luau table. Planning should mainly add contract tests and remove references to `mesh.config.get` / `mesh.config.get_all` from backend API docs if any exist.

### Logging

Existing logging accepts `info`, `warn`, `warning`, `error`, and `debug`, with unknown levels emitted as warnings. Phase 03 should lock public levels to `debug`, `info`, `warn`, and `error`. It can keep `warning` as a compatibility alias only if not documented, but tests should assert the four public levels and invalid-level non-fatal warning behavior.

### Poll Interval

The backend runtime already clamps effective poll intervals through `bounded_poll_interval_ms(ctx).max(50)`. This satisfies runtime safety but hides corrections from authors. Phase 03 should move the minimum constant into a named contract and warn when `set_poll_interval(ms)` receives a value below 50.

Interval refresh timing already happens after callbacks in `spawn_backend_service()`: after init when interval is first read, after poll callbacks, and after command handlers. Tests should explicitly prove this timing so future changes do not drift.

## Planning Implications

- Start with the host API Rust contract before migrating providers. Removing `exec_shell` first will break provider tests until migration, so the provider migration should depend on the host API plan.
- Provider migration can be separate from config/log/poll interval work because it touches Luau provider files, not core Rust host API internals.
- Config/log work and poll interval work both touch `backend.rs`; they should not run in parallel unless plan ownership is split very carefully. Sequential waves are safer.
- Since `BHOST-02` conflicts with the new context decision, plans should explicitly address `BHOST-02` by removing `mesh.exec_shell` from the MVP contract and migrating callers.

## Validation Architecture

### Automated Test Strategy

Use Rust unit tests in the existing test harness:

- `nix develop -c cargo test -p mesh-core-scripting backend`
- `nix develop -c cargo test -p mesh-core-backend spawn_backend_service`

Use static grep checks to prove API removal and provider migration:

- No `mesh.exec_shell` registration in `crates/core/runtime/scripting/src/backend.rs`.
- No `mesh.exec_shell` calls in `packages/plugins/backend/core/*/src/main.luau`.
- Host API tests no longer expect `mesh.exec_shell`.

### Validation Dimensions

1. `mesh.exec` accepts structured args and returns structured process results.
2. Legacy string-splitting `mesh.exec("program args")` is rejected or fails as API misuse.
3. `mesh.exec_shell` is not exposed as public backend API and bundled providers do not call it.
4. `mesh.config()` returns nested plugin settings as a table.
5. `mesh.log` supports both call styles and the four public levels.
6. Invalid log levels are non-fatal and visible as warnings.
7. `set_poll_interval` clamps low values to 50ms and warns.
8. Interval changes made in init, poll, and command handlers affect the next interval after the current callback.

### Suggested Feedback Sampling

- After host API changes: `nix develop -c cargo test -p mesh-core-scripting backend`.
- After provider migration: `nix develop -c cargo test -p mesh-core-scripting bundled_backend_scripts_expose_required_host_api_surface` plus grep for `exec_shell`.
- After poll interval runtime changes: `nix develop -c cargo test -p mesh-core-backend spawn_backend_service`.
- Full phase gate: `nix develop -c cargo test -p mesh-core-scripting backend && nix develop -c cargo test -p mesh-core-backend spawn_backend_service`.

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Removing `exec_shell` breaks bundled provider scripts | Migrate providers in a dedicated plan and add bundled script host API tests. |
| Shell pipeline rewrites lose parsing behavior | Preserve parser tests or assert emitted payload shapes for representative scripts. |
| Malformed `mesh.exec` arguments become confusing | Add explicit tests for the rejected single-string form and document the structured call shape in comments. |
| Poll interval clamping remains invisible | Emit a warning in `set_poll_interval` when clamping occurs. |
| Requirement traceability conflicts with `BHOST-02` wording | Plans should list `BHOST-02` and state that it is satisfied by context-approved removal from the MVP API. |

## Research Complete

Phase 03 is ready for planning.
