---
phase: 03-frontend-reactivity-and-events
reviewed: 2026-05-02T19:11:38Z
depth: standard
files_reviewed: 9
files_reviewed_list:
  - crates/core/foundation/diagnostics/src/lib.rs
  - crates/core/runtime/scripting/src/context.rs
  - crates/core/shell/src/shell/component.rs
  - crates/core/shell/src/shell/layout.rs
  - crates/core/shell/src/shell/mod.rs
  - crates/core/shell/src/shell/types.rs
  - crates/core/ui/render/src/render.rs
  - docs/plugins/frontend/core/README.md
  - packages/plugins/frontend/core/navigation-bar/src/components/volume-button.mesh
findings:
  critical: 1
  warning: 2
  info: 0
  total: 3
status: issues_found
---

# Phase 03: Code Review Report

**Reviewed:** 2026-05-02T19:11:38Z
**Depth:** standard
**Files Reviewed:** 9
**Status:** issues_found

## Summary

Re-reviewed the listed Phase 03 files after commit `11afe87`. The previous critical findings for `onRender()` invocation, popover positioning, and the navigation volume payload are fixed. One remaining blocker still leaves valid service-state updates unable to trigger a repaint for components that use the documented raw service state path, and two warning-level robustness issues remain around diagnostics visibility and hot reload.

## Critical Issues

### CR-01: BLOCKER - Raw service-state updates do not schedule a repaint unless proxy fields were previously tracked

**File:** `crates/core/shell/src/shell/component.rs:1471`

**Issue:** `handle_service_event()` always calls `apply_service_update()`, which mutates reactive `ScriptState` (`last_service_update` and `state[service_name]`) and can mark that runtime dirty. The component ignores that dirty flag and only sets `self.dirty` when `tracked_service_fields_changed(...)` returns true. For any frontend that binds directly to the documented raw service state (`{audio.percent}`, `{last_service_update.name}`, or other `state[service]` values) without reading the Lua proxy first, `tracked_fields` is empty, so the service state changes but no repaint is scheduled. The UI remains stale until some unrelated event dirties the component.

**Fix:**
```rust
let previous = runtime.script_ctx.state().get(&service_name);
let tracked_fields = runtime.script_ctx.tracked_fields_for_service(&service_name);
apply_service_update(
    runtime.script_ctx.state_mut(),
    has_read,
    service,
    source_plugin,
    payload.clone(),
);

let state_changed = runtime.script_ctx.state().is_dirty();
if has_read {
    runtime.script_ctx.apply_service_payload(&service_name, payload);
}

if state_changed
    || (has_read && tracked_service_fields_changed(previous.as_ref(), payload, &tracked_fields))
{
    self.render_hooks_pending = true;
    self.dirty = true;
}
```

If the goal is to avoid repainting on every backend emission, add explicit template dependency tracking instead of dropping the existing `ScriptState` dirty signal.

## Warnings

### WR-01: WARNING - Service lookup diagnostics captured inside `pcall` are never published to component diagnostics

**File:** `crates/core/shell/src/shell/component.rs:1210`

**Issue:** `ScriptContext` records interface lookup failures in `shared_diagnostics`, and `drain_diagnostics()` exists to expose them, including when Lua catches the failure with `pcall`. `call_namespaced_handler()` drains only published events after a successful handler, and `call_render_hooks()` also never drains script diagnostics after `onRender()`. The README says missing interfaces are visible to operators even when `pcall` catches the Lua error, but those diagnostics remain inside the script context and never update the shell diagnostics collector.

**Fix:** Drain script diagnostics after every handler attempt and record them on the component diagnostics handle, for example:
```rust
for diagnostic in runtime.script_ctx.drain_diagnostics() {
    if let Some(diagnostics) = &self.diagnostics {
        diagnostics.error(format!(
            "interface '{}' unavailable for '{}': {}",
            diagnostic.interface, diagnostic.plugin_id, diagnostic.reason
        ));
    }
}
```

Apply the same drain in the `onRender` path and in error branches so diagnostics are not stranded when a handler returns an error.

### WR-02: WARNING - Hot reloaded frontends skip their first `onRender()` pass

**File:** `crates/core/shell/src/shell/component.rs:1697`

**Issue:** `reload_source()` clears runtimes, creates a fresh root runtime, and marks the component dirty, but it does not reset `render_hooks_pending`. If the component had already rendered before reload, `render_hooks_pending` is usually false, so the newly loaded script's `onRender()` will not run before the first rebuilt tree. Components that derive template globals in `onRender()` can show default or stale values until a later service event or other trigger sets the flag.

**Fix:**
```rust
self.runtimes.lock().unwrap().clear();
self.init_root_runtime()?;
self.render_hooks_pending = true;
self.dirty = true;
```

---

_Reviewed: 2026-05-02T19:11:38Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
