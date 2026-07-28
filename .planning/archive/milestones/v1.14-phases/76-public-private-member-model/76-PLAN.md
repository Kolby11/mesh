---
phase: 76
phase_name: public-private-member-model
status: planned
created: 2026-05-26
requirements:
  - LUAMEM-01
  - LUAMEM-02
  - LUAMEM-03
  - LUAMEM-04
---

# Phase 76: Public Private Member Model - Plan

## Goal

Treat locals as private and non-local variables/functions as public object members, with lifecycle hooks reserved.

## Tasks

### 76-01 Frontend Public Member Inspection

**Files:**
- `crates/core/runtime/scripting/src/context/runtime.rs`
- `crates/core/runtime/scripting/src/context/tests.rs`

**Work:**
- Add frontend public field/function inspection helpers.
- Use existing synced globals as public fields.
- Discover non-local Lua functions as public functions while excluding builtins and reserved lifecycle hooks.
- Preserve local privacy and existing reactive global behavior.

**Validation:**
- Tests for private locals, public fields, public functions, reserved hooks, and compatibility state sync.

### 76-02 Backend Public Member Inspection

**Files:**
- `crates/core/runtime/scripting/src/backend/runtime.rs`
- `crates/core/runtime/scripting/src/backend/tests.rs`

**Work:**
- Add backend public function inspection helpers.
- Reserve backend lifecycle hooks so `start`, `stop`, and `init` do not appear as ordinary public functions.
- Keep provider state export behavior unchanged.

**Validation:**
- Tests for backend public functions and reserved lifecycle exclusion.

## Verification

Run:

```bash
nix develop -c cargo test -p mesh-core-scripting
nix develop -c cargo fmt --check
```
