---
phase: 04-real-core-surfaces
plan: 02
subsystem: quick-settings-surfaces
tags: [rust, luau, mesh, quick-settings, audio, network, service-proxy]

# Dependency graph
requires:
  - phase: 04-real-core-surfaces
    provides: audio set_volume proxy/provider contract from Plan 01
provides:
  - Quick settings audio controls backed by live proxy state and set_volume command routing
  - Quick settings Wi-Fi state/list rendering with guarded command handlers
  - Shell-facing quick_settings regression tests for audio/network render and command behavior
affects: [04-real-core-surfaces, quick-settings, audio-surfaces, network-surfaces]

# Tech tracking
tech-stack:
  added: []
  patterns: [callback-free proxy field reads, guarded proxy command handlers, shell-facing ScriptContext regressions]

key-files:
  created:
    - .planning/phases/04-real-core-surfaces/04-02-SUMMARY.md
  modified:
    - packages/plugins/frontend/core/quick-settings/src/main.mesh
    - packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh
    - packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh
    - packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh
    - crates/core/shell/src/shell/component.rs

key-decisions:
  - "Quick settings audio uses the finalized direct proxy call `audio.set_volume(\"default\", normalized)` for slider changes."
  - "Quick settings Wi-Fi rows remain guarded and display-only when provider data lacks a non-empty network id."
  - "Unavailable and permission-denied states are visible in the surface while technical details stay in logs/diagnostics."

patterns-established:
  - "Quick settings sections derive availability and display state during onRender from service proxy fields."
  - "Mutating handlers re-check provider availability before publishing service proxy commands."

requirements-completed: [SURF-02, SURF-03, SURF-04, SURF-05]

# Metrics
duration: 6min
completed: 2026-05-03
---

# Phase 04 Plan 02: Quick Settings Real Controls Summary

**Quick settings now renders live audio and Wi-Fi proxy state, publishes audio/network commands through service proxies, and shows visible unavailable/control-denied states.**

## Performance

- **Duration:** 6 min
- **Started:** 2026-05-03T06:35:14Z
- **Completed:** 2026-05-03T06:41:11Z
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments

- Wired the quick-settings audio slider to `audio.set_volume("default", normalized)` and guarded volume/mute handlers.
- Added Wi-Fi availability, control-denied, disabled, scanning, and unsafe-connect fallback states with exact UI-SPEC copy.
- Added shell-facing `quick_settings_*` tests covering audio render, audio slider command routing, Wi-Fi toggle routing, missing-service fallback copy, and empty-id Wi-Fi row behavior.

## Task Commits

Each task was committed atomically:

1. **Task 1: Upgrade quick-settings audio controls** - `2af05c5` (feat)
2. **Task 2: Upgrade quick-settings Wi-Fi state, list, and guarded connect behavior** - `b993a27` (feat)
3. **Task 3: Add quick-settings shell integration tests** - `7a01a31` (test)

**Plan metadata:** recorded in final docs commit.

## Files Created/Modified

- `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` - Derives audio availability/live display state and routes slider changes through `audio.set_volume`.
- `packages/plugins/frontend/core/quick-settings/src/main.mesh` - Guards Wi-Fi toggle commands against unavailable/control-denied network service state.
- `packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh` - Renders network unavailable, controls unavailable, disabled, scanning, and live network-list states.
- `packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh` - Guards connect behavior for empty ids, missing providers, and denied controls.
- `crates/core/shell/src/shell/component.rs` - Adds focused quick-settings shell regression tests.

## Decisions Made

- Kept command publication in frontend handlers through named proxy methods; no service callback APIs or proxy state mutation were introduced.
- Treated empty Wi-Fi row ids as display-only state with `Connection details unavailable`.
- Used token-only styles and changed touched section-title letter spacing to `0` to satisfy the Phase 4 UI contract.

## Verification

- `cargo fmt --check --package mesh-core-shell` passed.
- `rg -n "onVolumeChange|set_volume|Audio unavailable|Audio controls unavailable" packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` passed.
- `rg -n "Network unavailable|Network controls unavailable|Connection details unavailable|set_wifi_enabled" packages/plugins/frontend/core/quick-settings/src` passed.
- `rg -n "mesh\\.service\\.bind|mesh\\.service\\.on|\\.on_change\\(" packages/plugins/frontend/core/quick-settings/src` returned no matches.
- `cargo test -p mesh-core-shell quick_settings` could not run in this environment because `smithay-client-toolkit` requires the missing system package `xkbcommon.pc`.

## Deviations from Plan

None - plan executed as written. The required cargo test command is blocked by the known environment limitation called out in the execution prompt, not by the plan implementation.

**Total deviations:** 0 auto-fixed.
**Impact on plan:** No scope changes.

## Issues Encountered

- `cargo test -p mesh-core-shell quick_settings` fails before executing tests because pkg-config cannot find `xkbcommon.pc` for `smithay-client-toolkit`. Static verification and formatting checks passed; the cargo test should be rerun in an environment with xkbcommon development files installed.

## Known Stubs

None. Empty reactive defaults in the touched `.mesh` files are initial provider-fallback state or component props, not unwired mock data.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Plan 03 can build on quick settings as the primary real control surface. Audio and Wi-Fi now use the public proxy state-and-command contract, and unavailable/permission-denied paths remain visible for final top-panel and end-to-end phase validation.

## Self-Check: PASSED

- Summary file exists.
- Key modified files exist.
- Task commits found: `2af05c5`, `b993a27`, `7a01a31`.

---
*Phase: 04-real-core-surfaces*
*Completed: 2026-05-03*
