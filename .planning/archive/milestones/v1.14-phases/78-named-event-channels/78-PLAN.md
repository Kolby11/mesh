---
phase: 78
phase_name: named-event-channels
status: planned
created: 2026-05-26
requirements:
  - LUAEVT-01
  - LUAEVT-02
  - LUAEVT-03
  - LUAEVT-04
  - LUAEVT-05
  - LUAEVT-06
---

# Phase 78: Named Event Channels - Plan

## Tasks

### 78-01 Direct Interface Event Channels

**Files:**
- `crates/core/runtime/scripting/src/context/proxy.rs`
- `crates/core/runtime/scripting/src/context/tests.rs`

**Work:**
- Expose declared interface events directly on service proxies.
- Keep `proxy.events.Name` compatibility.
- Add `:on(fn)` and `:fire(payload)` aliases to event channels.

### 78-02 Local Frontend Self Events

**Files:**
- `crates/core/runtime/scripting/src/context/runtime.rs`
- `crates/core/runtime/scripting/src/context/tests.rs`

**Work:**
- Add named `self.EventName` channels to frontend runtime `self`.
- Reuse persistent per-runtime channel registry so subscriptions survive within the runtime lifetime.

### 78-03 Backend Provider Self Events

**Files:**
- `crates/core/runtime/scripting/src/backend/runtime.rs`
- `crates/core/runtime/scripting/src/backend/tests.rs`

**Work:**
- Add backend `self.EventName:fire(payload)` channels.
- Route fired backend events into the same typed event queue as `mesh.service.emit_event(...)`.

## Verification

Run:

```bash
nix develop -c cargo test -p mesh-core-scripting
nix develop -c cargo fmt --check
```
