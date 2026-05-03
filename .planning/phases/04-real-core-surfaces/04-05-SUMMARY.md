---
phase: 04-real-core-surfaces
plan: 05
subsystem: core-service-providers
tags:
  - gap-closure
  - backend-services
  - command-safety
requires:
  - phase: 04-real-core-surfaces
    plan: 02
provides:
  - NetworkManager connection_id command payload support
  - PipeWire and PulseAudio set_muted handlers
  - Structured audio play_sound execution
affects:
  - packages/plugins/backend/core/networkmanager-network/src/main.luau
  - packages/plugins/backend/core/pipewire-audio/src/main.luau
  - packages/plugins/backend/core/pulseaudio-audio/src/main.luau
tech-stack:
  added: []
  patterns:
    - Provider-specific command logic remains in Luau providers
key-files:
  created: []
  modified:
    - packages/plugins/backend/core/networkmanager-network/src/main.luau
    - packages/plugins/backend/core/pipewire-audio/src/main.luau
    - packages/plugins/backend/core/pulseaudio-audio/src/main.luau
key-decisions:
  - Network providers consume public `connection_id` payloads before legacy `id`.
  - Audio playback uses structured `mesh.exec("aplay", { path })` after validating path.
requirements-completed:
  - SURF-03
  - SURF-05
duration: 4 min
completed: 2026-05-03
---

# Phase 04 Plan 05: Provider Contract and Command Safety Summary

Bundled network and audio providers now consume the finalized command payloads and avoid shell concatenation for audio playback.

## Execution

- **Duration:** 4 min
- **Started:** 2026-05-03T07:18:00Z
- **Completed:** 2026-05-03T07:25:33Z
- **Tasks:** 3
- **Files modified:** 3

## What Changed

- Updated NetworkManager connect/disconnect handlers to prefer `payload.connection_id`, retain legacy `payload.id`, and reject empty IDs with `missing connection_id`.
- Added `on_command_set_muted()` to both PipeWire and PulseAudio providers.
- Replaced `mesh.exec_shell("aplay " .. path)` with validated structured `mesh.exec("aplay", { path })`.

## Commits

| Commit | Description |
|--------|-------------|
| 29f29a0 | Closed Phase 4 runtime, provider, and surface routing gaps. |

## Verification

- `cargo test -p mesh-core-service -- --nocapture` passed: 11 tests, 2 doctests ignored.
- `rg` checks confirmed `payload.connection_id`, `missing connection_id`, `on_command_set_muted`, `payload.muted`, and structured `mesh.exec("aplay", { path })`.
- `rg -n "wpctl|pactl|nmcli|bluetoothctl" crates/core || true` returned no matches, confirming service-specific commands stayed out of Rust core.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

None.

## Self-Check: PASSED

- Summary file exists.
- Key modified provider files exist.
- Required provider and service verification passed.

