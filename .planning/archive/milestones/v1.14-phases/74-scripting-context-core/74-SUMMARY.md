---
phase: 74
phase_name: scripting-context-core
status: complete
completed: 2026-05-26
requirements:
  - LUACTX-01
  - LUACTX-02
  - LUACTX-03
  - LUACTX-04
---

# Phase 74: Scripting Context Core - Summary

## Delivered

- Frontend `ScriptContext` now creates a runtime-owned current instance table with `self.meta`.
- Frontend lifecycle calls pass `self` to `init`, canonical `render`, and other lifecycle names while preserving legacy no-argument behavior.
- Frontend render dispatch now prefers canonical `render(self)` and falls back to legacy `onRender`.
- Backend scripts now support canonical `start(self)` as the startup lifecycle and preserve legacy `init()` fallback.
- Backend scripts now support optional `stop(self)`, and the backend runtime loop calls it before publishing `Stopped`.
- Backend poll and command handlers receive the same current-provider context using Luau-compatible extra arguments.
- Runtime tests prove frontend and backend `self.meta` metadata plus legacy compatibility paths.

## Files Changed

- `crates/core/runtime/scripting/src/context/runtime.rs`
- `crates/core/runtime/scripting/src/context/tests.rs`
- `crates/core/runtime/scripting/src/backend/runtime.rs`
- `crates/core/runtime/scripting/src/backend/tests.rs`
- `crates/core/runtime/backend/src/lib.rs`
- `crates/core/shell/src/shell/component/runtime.rs`

## Verification

Passed:

```bash
nix develop -c cargo test -p mesh-core-scripting
nix develop -c cargo test -p mesh-core-backend
nix develop -c cargo fmt --check
```

Additional integration check:

```bash
nix develop -c cargo test -p mesh-core-shell component
```

The shell component suite compiled and ran, with 179 passing and 3 failing. The failures were pre-existing broader component assertions outside the Phase 74 lifecycle path:

- `shell::component::tests::interaction::diagnostics::icon_reliability_core_surfaces_proof` — unmapped `language` icon assertion.
- `shell::component::tests::interaction::reflow::container_size_restyle_preserves_runtime_and_local_state` — layout clamp panic.
- `shell::component::tests::restyle::preservation::state_preservation_restyle_user_input_state_survives_focus_restyle` — layout clamp panic.
