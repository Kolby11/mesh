---
phase: 01-backend-host-api-contract
reviewed: 2026-05-01T16:13:50Z
depth: standard
files_reviewed: 4
files_reviewed_list:
  - crates/core/runtime/scripting/src/backend.rs
  - crates/core/runtime/scripting/src/host_api.rs
  - crates/core/runtime/backend/src/lib.rs
  - crates/core/shell/src/shell/mod.rs
findings:
  critical: 1
  warning: 3
  info: 0
  total: 4
status: issues_found
---

# Phase 01: Code Review Report

**Reviewed:** 2026-05-01T16:13:50Z
**Depth:** standard
**Files Reviewed:** 4
**Status:** issues_found

## Summary

The Phase 01 implementation mostly wires the backend contract through cleanly, and the targeted crate tests pass, but the public `mesh.service.emit_json(...)` contract regressed in a way that breaks documented compatibility. I also found one silent-failure path in that API, a runtime deduplication gap for command-triggered updates, and a remaining host API documentation ambiguity around `mesh.config`.

## Critical Issues

### CR-01: `mesh.service.emit_json(...)` lost its documented compatibility forms

**Classification:** BLOCKER
**File:** `crates/core/runtime/scripting/src/backend.rs:187-193`
**Issue:** The host API now registers `emit_json` as `create_function(move |_lua, text: String| { ... })`, which means Luau can only call it with a string. That breaks the documented/public contract for Phase 01 in two ways: `mesh.service.emit_json({ ... })` can no longer emit a Lua table directly, and `mesh.service.emit_json()` / `mesh.service.emit_json(nil)` can no longer fall back to the current command payload. The surrounding comments still advertise `value?`, and the repo’s backend API docs/LSP data still describe both compatibility forms. Any backend plugin using either legacy form now errors at runtime instead of emitting state.
**Fix:**
```rust
service.set(
    "emit_json",
    self.lua.create_function(move |lua, value: Option<LuaValue>| {
        let payload = match value {
            None | Some(LuaValue::Nil) => runtime.lock().unwrap().current_payload.clone(),
            Some(LuaValue::String(text)) => serde_json::from_str::<JsonValue>(text.to_str()?.trim())
                .map_err(mlua::Error::external)?,
            Some(other) => lua.from_value::<JsonValue>(other)?,
        };
        runtime.lock().unwrap().pending_emit = Some(payload);
        Ok(())
    })?,
)?;
```

## Warnings

### WR-01: Invalid JSON passed to `emit_json` is silently discarded

**Classification:** WARNING
**File:** `crates/core/runtime/scripting/src/backend.rs:188-193`
**Issue:** When `serde_json::from_str` fails, the closure returns `Ok(())` and leaves `pending_emit` untouched. That violates the phase success criterion that backend API failures must be surfaced as diagnostics or explicit Luau errors rather than failing silently. A malformed backend payload currently disappears with no emitted state and no plugin-scoped diagnostic.
**Fix:**
```rust
let payload = serde_json::from_str::<JsonValue>(text.trim())
    .map_err(mlua::Error::external)?;
runtime.lock().unwrap().pending_emit = Some(payload);
```
If silent tolerance is required, log a plugin-scoped warning before returning.

### WR-02: Command-triggered emits bypass runtime payload deduplication

**Classification:** WARNING
**File:** `crates/core/runtime/backend/src/lib.rs:68-77`
**Issue:** The poll branch suppresses duplicate payloads with `if Some(&payload) == last_payload.as_ref() { continue; }`, but the command branch always forwards emitted state. That means `on_command_*` handlers can generate redundant `BackendServiceUpdate`s for unchanged payloads, which regresses the “preserve payload deduplication” behavior called out in Plan 02 and will spuriously fire downstream update handlers in later phases.
**Fix:**
```rust
if let Some(payload) = ctx.run_command(&msg.command, &msg.payload) {
    refresh_interval(&ctx, &mut interval_ms, &mut tick);
    if Some(&payload) == last_payload.as_ref() {
        continue;
    }
    last_payload = Some(payload.clone());
    if tx.send(BackendServiceUpdate { ... payload }).is_err() {
        break;
    }
}
```

### WR-03: `mesh.config` is still documented as two incompatible shapes without context

**Classification:** WARNING
**File:** `crates/core/runtime/scripting/src/host_api.rs:16-18`
**Issue:** The host API comment block documents `mesh.config.get(key)`, `mesh.config.get_all()`, and `mesh.config()` together without saying they are runtime-specific variants. In the reviewed backend implementation, `mesh.config` is a function, not a table, so this still leaves plugin authors guessing which form exists in which runtime, which is the exact ambiguity Phase 01 was meant to remove.
**Fix:** Mark each form explicitly as frontend-only or backend-only in the comment block, or split the docs into separate frontend/backend API sections so `mesh.config` is never described with two incompatible shapes in one list.

---

_Reviewed: 2026-05-01T16:13:50Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
