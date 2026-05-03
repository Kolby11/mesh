# Phase 1 Research: Backend Host API Contract

**Phase:** 01 — Backend Host API Contract
**Date:** 2026-05-01
**Status:** Research complete

## Research Question

What does the planner need to know to stabilize backend Luau host APIs for command execution, configuration, logging, service emission, and poll interval control?

## Current State

### Backend Luau Runtime

`crates/core/runtime/scripting/src/backend.rs` owns `BackendScriptContext`, installs the backend `mesh` global, stores pending service emissions, and exposes:

- `mesh.service.set_poll_interval(ms)`
- `mesh.service.emit(table)`
- `mesh.service.emit_json(text)`
- `mesh.service.emit_unavailable()`
- `mesh.service.payload()`
- `mesh.service.has_capability(name)`
- `mesh.exec(command_string)`
- `mesh.exec_shell(command_string)`
- `mesh.log.info(message)`
- `mesh.log.warn(message)`

Existing tests already cover script loading, `init()`, poll interval capture, service emission, unavailable state, payload access, command handlers, `exec_shell`, capabilities, and missing `init()`.

### Backend Runtime Loop

`crates/core/runtime/backend/src/lib.rs` owns `spawn_backend_service()`. It loads and initializes a backend script, reads `ctx.poll_interval_ms()` once, creates a Tokio interval, dispatches `on_poll()`, dispatches `on_command_*`, and sends `BackendServiceUpdate`.

Important gap: `set_poll_interval()` currently affects the interval captured after `init()`, but later changes will not affect the already-created Tokio interval unless the runtime loop re-reads the interval and recreates or resets the timer.

### API Contract Mismatches

Phase requirements and context call for API behavior that is not fully aligned with current code:

- `HOST-01`: `mesh.exec(cmd, args)` requires structured arguments. Current implementation accepts one command string and splits on whitespace.
- `HOST-03`: `mesh.config()` should return plugin settings as a Luau table. Current backend runtime does not inject plugin settings into `BackendScriptContext`.
- `HOST-04`: `mesh.log(level, msg)` should exist. Current code exposes `mesh.log.info()` and `mesh.log.warn()`, with comments mentioning `error()`.
- `HOST-05`: `mesh.service.emit(payload)` exists, but error behavior for non-serializable payloads should be explicit and tested.
- `HOST-06`: `mesh.service.set_poll_interval(ms)` exists, but only reliably affects initial interval setup.

## Recommended Plan Shape

Two sequential plans are enough:

1. **BackendScriptContext API contract:** implement missing public API forms and tests in `mesh-core-scripting`.
2. **Backend runtime integration:** pass plugin settings into contexts, make poll interval changes effective in the runtime loop, and add integration coverage around `spawn_backend_service()`.

This split avoids parallel edits to the same runtime files and keeps Phase 1 backend-only. Phase 2 should own frontend `require('@mesh/<service>')` delivery.

## Files to Read First

- `crates/core/runtime/scripting/src/backend.rs`
- `crates/core/runtime/scripting/src/host_api.rs`
- `crates/core/runtime/backend/src/lib.rs`
- `crates/core/shell/src/shell/mod.rs`
- `packages/plugins/backend/core/pipewire-audio/src/main.luau`
- `packages/plugins/backend/core/pulseaudio-audio/src/main.luau`
- `packages/plugins/backend/core/networkmanager-network/src/main.luau`
- `packages/plugins/backend/core/upower-power/src/main.luau`
- `packages/plugins/backend/core/shell-theme/src/main.luau`

## Implementation Guidance

### Command Execution

Keep normal nonzero command exits as returned data, not thrown errors. `mesh.exec("printf", {"hello"})` should avoid whitespace splitting and should return the same shape as `mesh.exec_shell("printf hello")`:

```lua
{
  success = true,
  stdout = "hello",
  stderr = "",
  code = 0,
}
```

Continue supporting the current single-string form for compatibility, but make the structured `(program, args)` form canonical.

### Plugin Configuration

Add settings storage to `BackendScriptContext`, with constructors or setters that allow `spawn_backend_service()` to pass a `serde_json::Value`. Expose a callable `mesh.config()` returning the whole settings table. If existing frontend config helpers stay as `mesh.config.get`, keep that separate from this backend contract.

### Logging

Support `mesh.log(level, msg)` and preserve method aliases:

- `mesh.log.info(msg)`
- `mesh.log.warn(msg)`
- `mesh.log.error(msg)`

Accepted levels should include `info`, `warn`, `warning`, `error`, `debug`, and unknown levels should not panic.

### Emission and Diagnostics

`mesh.service.emit(payload)` should fail visibly when payload conversion fails. `run_poll()` and `run_command()` currently log handler errors and return no payload; tests should assert that bad scripts do not produce misleading updates.

### Poll Interval

The backend runtime loop should honor interval changes after initialization. A practical approach is to compare `ctx.poll_interval_ms().max(50)` before each tick cycle or after command handling and recreate/reset the Tokio interval when it changes.

## Validation Architecture

### Automated Checks

- `cargo test -p mesh-core-scripting backend`
- `cargo test -p mesh-core-backend`
- `cargo test -p mesh-core-scripting`

### Required Coverage

- `mesh.exec("printf", {"hello"})` returns `success=true`, `stdout="hello"`, and `code=0`.
- `mesh.exec_shell("printf 'hello'")` preserves the existing result table shape.
- `mesh.config()` returns settings passed from Rust as a Luau table.
- `mesh.log("info", "message")`, `mesh.log.info("message")`, `mesh.log.warn("message")`, and `mesh.log.error("message")` are callable.
- `mesh.service.emit({ ... })` still emits JSON-compatible payloads.
- Non-serializable emit payloads surface as handler errors and do not emit stale payloads.
- `mesh.service.set_poll_interval(ms)` affects `spawn_backend_service()` polling cadence after the runtime starts.

## Risks

- Changing `mesh.exec` too aggressively could break existing bundled plugins. Preserve the old one-string form.
- Backend config plumbing may require touching shell plugin spawn code in addition to runtime crates.
- Tokio timer tests can be flaky if they assert exact elapsed durations. Prefer deterministic hooks or broad timing bounds.

## RESEARCH COMPLETE

Phase 1 is ready for planning.
