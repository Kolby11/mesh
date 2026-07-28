---
phase: 74
phase_name: scripting-context-core
status: planned
created: 2026-05-26
requirements:
  - LUACTX-01
  - LUACTX-02
  - LUACTX-03
  - LUACTX-04
---

# Phase 74: Scripting Context Core - Plan

## Goal

Inject runtime-provided `self` into frontend/backend lifecycle hooks and expose stable `self.meta` identity while preserving legacy entrypoints.

## Tasks

### 74-01 Frontend Current Instance Context

**Files:**
- `crates/core/runtime/scripting/src/context/runtime.rs`
- `crates/core/runtime/scripting/src/context/tests.rs`
- `crates/core/shell/src/shell/component/runtime.rs`

**Work:**
- Add a runtime-owned frontend `self` table with `self.meta`.
- Include module/component identity, runtime kind, instance identity, and diagnostics identity in `self.meta`.
- Pass `self` to `init`, `render`, `mount`, and `unmount` lifecycle hooks when present.
- Keep legacy `init`, `onRender`, existing event handlers, global `mesh`, and `module` behavior compatible.
- Prefer canonical `render(self)` during render hook dispatch, falling back to legacy `onRender`.

**Validation:**
- Add scripting tests proving `init(self)` and `render(self)` receive `self.meta`.
- Add compatibility tests proving legacy no-arg `init()` and `onRender()` still work.

### 74-02 Backend Current Provider Context

**Files:**
- `crates/core/runtime/scripting/src/backend/runtime.rs`
- `crates/core/runtime/scripting/src/backend/tests.rs`
- `crates/core/runtime/backend/src/lib.rs`

**Work:**
- Add a runtime-owned backend provider `self` table with `self.meta`.
- Support canonical `start(self)` and `stop(self)` lifecycle hooks.
- Preserve legacy `init()` as the startup fallback.
- Pass `self` to `start`, `stop`, `on_poll`, and command handlers where compatible with Lua extra-argument semantics.
- Call `stop(self)` when the backend runtime loop exits.

**Validation:**
- Add backend tests proving `start(self)` and `stop(self)` receive provider metadata.
- Add compatibility tests proving legacy `init()` remains accepted.

## Verification

Run:

```bash
cargo test -p mesh-core-scripting
cargo test -p mesh-core-runtime-backend
```
