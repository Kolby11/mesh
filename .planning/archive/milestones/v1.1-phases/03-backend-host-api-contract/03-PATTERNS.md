---
phase: 03-backend-host-api-contract
status: complete
created: 2026-05-03
---

# Phase 03 Pattern Map

## Closest Existing Analogs

| Planned Area | Target Files | Closest Existing Analog | Pattern to Reuse |
|--------------|--------------|-------------------------|------------------|
| Backend host API registration | `crates/core/runtime/scripting/src/backend.rs` | Existing `install_host_api()` implementation | Register host functions directly on the `mesh` table, keep runtime state behind `Arc<Mutex<BackendRuntime>>`, and cover public surface in `#[cfg(test)]`. |
| Structured exec results | `crates/core/runtime/scripting/src/backend.rs` | `ExecOutcome`, `exec_result_to_lua()`, `exec_outcome_to_lua()` | Keep one conversion path for stdout/stderr/code/success results; avoid special cases in provider scripts. |
| Runtime poll interval behavior | `crates/core/runtime/backend/src/lib.rs` | `bounded_poll_interval_ms()`, `refresh_interval()` | Read interval after callbacks and replace the Tokio interval only when the effective value changes. |
| Bundled provider command execution | `packages/plugins/backend/core/*/src/main.luau` | Existing `mesh.exec("nmcli", {...})` command handlers in `networkmanager-network` | Pass dynamic values as structured args; parse command stdout in Luau helper functions. |
| Host API contract tests | `crates/core/runtime/scripting/src/backend.rs` | Existing backend tests under `mod tests` | Use small inline Luau scripts and inspect emitted JSON payloads. |
| Async lifecycle tests | `crates/core/runtime/backend/src/lib.rs` | Existing `spawn_backend_service_*` Tokio tests | Spawn the backend service task with test scripts, read lifecycle/update events from channels, and bound waits with `tokio::time::timeout`. |

## Concrete Patterns

### Host API Registration

Use the existing shape:

```rust
let mesh = self.lua.create_table()?;
mesh.set("exec", self.lua.create_function(... )?)?;
globals.set("mesh", mesh)?;
```

Do not introduce a new scripting abstraction for Phase 03 unless it removes real duplication in `backend.rs`.

### Structured Process Result

Keep the existing table keys:

```lua
{
  success = boolean,
  stdout = string,
  stderr = string,
  code = number | nil,
}
```

Plans should require tests for spawn failure and non-zero process exit so this stays stable.

### Provider Migration

Prefer this provider shape:

```lua
local result = mesh.exec("nmcli", { "-t", "-f", "DEVICE,TYPE,STATE", "device" })
if not result.success then return nil end
for line in result.stdout:gmatch("([^\n]+)") do
  -- parse in Luau
end
```

Avoid:

```lua
mesh.exec_shell("nmcli -t -f DEVICE,TYPE,STATE device")
```

### Poll Interval Timing

The runtime already uses a safe pattern:

```rust
refresh_interval(&ctx, &mut interval_ms, &mut tick);
```

Keep refreshes after callback returns. Do not reset the interval from inside the Lua host function itself.

## Integration Warnings

- Removing `mesh.exec_shell` before migrating providers will break bundled script tests. Plan dependencies must reflect that.
- `BHOST-02` is intentionally context-overridden. Plans should still list the ID for traceability, but the work is removal/migration rather than preserving a public `exec_shell` API.
- Avoid adding service-specific Rust parsing for PipeWire, PulseAudio, NetworkManager, or UPower.
- Grep checks for `exec_shell` should exclude planning artifacts when used as execution verification.

## Recommended File Ownership

- Plan 01 owns the host API surface in `crates/core/runtime/scripting/src/backend.rs` and docs/comments in `host_api.rs`.
- Plan 02 owns bundled Luau provider migration files under `packages/plugins/backend/core/**/src/main.luau`.
- Plan 03 owns config/log contract tests and logging behavior in `backend.rs`.
- Plan 04 owns poll interval clamping/timing in `backend.rs` and `crates/core/runtime/backend/src/lib.rs`.
