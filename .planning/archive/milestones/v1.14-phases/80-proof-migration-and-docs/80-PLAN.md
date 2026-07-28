---
phase: 80
phase_name: proof-migration-and-docs
status: planned
created: 2026-05-26
requirements:
  - LUAPROOF-01
  - LUAPROOF-02
  - LUAPROOF-03
  - LUAPROOF-04
  - LUAPROOF-05
---

# Phase 80: Proof Migration And Docs - Plan

## Tasks

### 80-01 Shipped Frontend Migration

**Files:**
- `modules/frontend/navigation-bar/**`
- `modules/frontend/audio-popover/src/main.mesh`

**Work:**
- Use Luau `require(...)` for shipped navigation component imports.
- Use `render(self)` for migrated render hooks.
- Use direct named service event channels, for example `audio.VolumeChanged:on(fn)`.

### 80-02 Shipped Backend Migration

**Files:**
- `modules/backend/**/src/main.luau`

**Work:**
- Use `start(self)` for backend startup hooks.
- Pass `self` through poll/command handlers.
- Use named provider event channels, for example `self.VolumeChanged:fire(payload)`.

### 80-03 Docs And Proof

**Files:**
- `docs/module-system.md`
- `docs/llm-context.md`
- `docs/modules/backend/core/README.md`

**Work:**
- Teach require, self, public/private members, named events, automatic rerendering, and storage deferral.
- Run regression tests covering the v1.14 contract and shipped real surfaces.

## Verification

Run:

```bash
nix develop -c cargo test -p mesh-core-scripting
nix develop -c cargo test -p mesh-core-component
nix develop -c cargo test -p mesh-core-frontend
nix develop -c cargo test -p mesh-core-backend
nix develop -c cargo test -p mesh-core-shell real_surfaces
nix develop -c cargo fmt --check
```
