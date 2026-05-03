---
phase: 04-real-core-surfaces
reviewed: 2026-05-03T07:32:00Z
depth: quick
files_reviewed: 12
files_reviewed_list:
  - crates/core/foundation/capability/src/lib.rs
  - crates/core/runtime/scripting/src/context.rs
  - crates/core/shell/src/shell/component.rs
  - crates/core/shell/src/shell/mod.rs
  - crates/core/shell/src/shell/service.rs
  - crates/core/shell/src/shell/types.rs
  - docs/plugins/frontend/core/README.md
  - packages/plugins/backend/core/networkmanager-network/src/main.luau
  - packages/plugins/backend/core/pipewire-audio/src/main.luau
  - packages/plugins/backend/core/pulseaudio-audio/src/main.luau
  - packages/plugins/frontend/core/panel/src/main.mesh
  - packages/plugins/frontend/core/quick-settings/src/main.mesh
findings:
  critical: 0
  warning: 0
  info: 0
  total: 0
status: clean
---

# Phase 04: Post-Gap Code Review

**Reviewed:** 2026-05-03T07:32:00Z  
**Depth:** quick  
**Status:** clean

## Findings

No blocking issues found in the Phase 4 gap-closure changes.

## Review Notes

- The stale network-list blocker is closed by full `serde_json::Value` equality for reactive state.
- Read-only service proxies now deny contract command methods before publishing events.
- Script-published events and `CoreRequest::ServiceCommand` carry source plugin and capabilities, and shell dispatch re-checks `service.<name>.control`.
- NetworkManager connect/disconnect now consume `connection_id` and reject empty IDs.
- Audio `set_muted` exists in both bundled providers.
- Audio `play_sound` validates path and uses structured `mesh.exec("aplay", { path })`.
- Panel and quick-settings route through `shell.toggle-surface` and `shell.hide-surface` with `surface_id = "@mesh/quick-settings"`.
- Public frontend docs now teach the supported shell surface events.

## Verification Reviewed

- `cargo test -p mesh-core-scripting -- --nocapture` passed.
- `cargo test -p mesh-core-service -- --nocapture` passed.
- `cargo test -p mesh-core-backend -- --nocapture` passed.
- Shell crate tests remain blocked by missing `xkbcommon.pc` for `smithay-client-toolkit`; static routing and authorization checks passed.

