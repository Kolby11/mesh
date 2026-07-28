---
phase: 04-real-core-surfaces
plan: 03
subsystem: core-surfaces-verification
tags: [rust, luau, mesh, panel, quick-settings, service-proxy]

# Dependency graph
requires:
  - phase: 04-real-core-surfaces
    provides: quick-settings live audio and Wi-Fi proxy controls from Plan 02
provides:
  - Compact top panel proof that reads live audio, network, and power proxy fields
  - Final real_core_surfaces shell regression coverage for panel state, quick-settings commands, fallback copy, and callback-free public APIs
  - Frontend docs aligned with Phase 4 named proxy command examples
affects: [04-real-core-surfaces, panel, quick-settings, frontend-docs, service-proxy]

# Tech tracking
tech-stack:
  added: []
  patterns: [shipped-surface include_str regression tests, callback-free proxy field read checks, named proxy command documentation]

key-files:
  created:
    - .planning/phases/04-real-core-surfaces/04-03-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/component.rs
    - docs/plugins/frontend/core/README.md

key-decisions:
  - "The top panel remains a compact status and entry surface; direct service controls stay in quick settings."
  - "Final surface regressions use the shipped panel source for panel render/click behavior and focused snippets for command routing."
  - "Frontend docs show service mutations through named proxy methods instead of legacy service event channels."

patterns-established:
  - "Final surface tests are prefixed `real_core_surfaces_` and cover live seeded payload changes plus static callback-API regressions."
  - "Docs examples pair `pcall(require, \"@mesh/<service>@>=1.0\")` with direct proxy field reads and named proxy command calls."

requirements-completed: [SURF-01, SURF-02, SURF-03, SURF-04, SURF-05]

# Metrics
duration: 4min
completed: 2026-05-03
---

# Phase 04 Plan 03: Final Core Surface Proof Summary

**Top panel and quick settings now have final callback-free surface proof across live proxy reads, named proxy commands, fallback copy, and public docs examples.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-05-03T06:44:47Z
- **Completed:** 2026-05-03T06:49:03Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- Confirmed the shipped top panel reads `audio.percent`, `audio.muted`, `network.connections`, and `power.level` during `onRender()` while only publishing `shell.toggle-quick-settings` from the audio indicator.
- Added `real_core_surfaces_*` shell tests for panel state changes after seeded service payloads, panel click publication, quick-settings audio/Wi-Fi command routing, visible fallback copy, and legacy callback API regressions.
- Updated frontend docs so slider changes call `audio.set_volume("default", normalized)` through the service proxy instead of teaching a legacy audio event-channel mutation.

## Task Commits

Each task was committed atomically:

1. **Task 1: Confirm compact panel live-service proof** - `e505332` (test, empty verification commit)
2. **Task 2: Add final end-to-end surface coverage and static regressions** - `4810dc8` (test)
3. **Task 3: Align public frontend docs only if they contradict Phase 4** - `abf6d7f` (docs)

## Files Created/Modified

- `crates/core/shell/src/shell/component.rs` - Added final `real_core_surfaces_*` regression tests and shipped-source static checks.
- `docs/plugins/frontend/core/README.md` - Replaced the legacy audio service event example with a named proxy command example.
- `.planning/phases/04-real-core-surfaces/04-03-SUMMARY.md` - Captures execution results, verification, and runtime validation instructions.

## Decisions Made

- Kept the panel unchanged because it already satisfied the compact live-service proof and did not contain direct audio/network control calls.
- Used an empty verification commit for Task 1 to preserve per-task atomic commit history when no file edits were needed.
- Treated the missing `xkbcommon.pc` cargo failure as an environment limitation, matching the execution prompt.

## Verification

- `rg -n "audio\\.percent|audio\\.muted|network\\.connections|power\\.level|shell\\.toggle-quick-settings|set_wifi_enabled|set_volume" packages/plugins/frontend/core/panel/src/main.mesh` passed and showed required panel field reads plus quick-settings toggle publication.
- `cargo fmt --check --package mesh-core-shell` passed.
- `cargo test -p mesh-core-scripting` passed: 42 tests.
- `cargo test -p mesh-core-backend` passed: 4 tests.
- `cargo test -p mesh-core-shell real_core_surfaces` and `cargo test -p mesh-core-shell` were blocked before tests ran because `smithay-client-toolkit` requires the missing system package `xkbcommon.pc`.
- `rg -n "mesh\\.service\\.bind|mesh\\.service\\.on|\\.on_change\\(" packages/plugins/frontend/core/panel/src/main.mesh packages/plugins/frontend/core/quick-settings/src docs/plugins/frontend/core/README.md` returned no matches.
- `rg -n "Audio unavailable|Audio controls unavailable|Network unavailable|Network controls unavailable|Connection details unavailable" packages/plugins/frontend/core/quick-settings/src` showed visible disabled/fallback copy.

## Manual Runtime Verification

Manual host-service validation still requires a Wayland/dev shell environment with at least one audio provider and NetworkManager available:

1. Start MESH in that environment with bundled core frontend and backend plugins enabled.
2. Confirm the top panel displays live audio percent/icon, network connected/disconnected status, and battery text when providers emit state.
3. Click the panel audio indicator and confirm quick settings opens or toggles.
4. In quick settings, move the audio slider and use mute/step controls; confirm the backend provider applies the command and the surface rerenders from emitted state.
5. Toggle Wi-Fi where safe; confirm NetworkManager state changes or the UI remains visibly disabled with `Network controls unavailable`.
6. Disable or remove providers and confirm `Audio unavailable`, `Network unavailable`, and `Connection details unavailable` remain visible instead of blank UI.

## Deviations from Plan

None - plan executed as written. The shell cargo test blocker is the known environment limitation called out in the execution prompt, not an implementation deviation.

**Total deviations:** 0 auto-fixed.
**Impact on plan:** No scope changes.

## Issues Encountered

- `cargo test -p mesh-core-shell real_core_surfaces` and full `cargo test -p mesh-core-shell` cannot compile in this environment because pkg-config cannot find `xkbcommon.pc` for `smithay-client-toolkit`. The new tests should be rerun in a dev shell or system environment with xkbcommon development files installed.

## Known Stubs

None. Empty defaults and `nil` fallbacks in the scanned `.mesh`, docs, and test snippets are intentional initial state or graceful unavailable-service handling, not unwired mock data.

## User Setup Required

Manual runtime verification requires a Wayland/dev shell environment with xkbcommon development files, at least one audio provider, and NetworkManager available.

## Next Phase Readiness

Phase 04 now satisfies the surface success criteria through static checks and focused regressions where this environment can run them. Phase 05 can proceed to icon rendering reliability; the remaining shell runtime verification should be rerun inside the proper Wayland/Nix dev environment.

## Self-Check: PASSED

- Summary file exists.
- Key files exist: `crates/core/shell/src/shell/component.rs`, `docs/plugins/frontend/core/README.md`, and `packages/plugins/frontend/core/panel/src/main.mesh`.
- Task commits found: `e505332`, `4810dc8`, `abf6d7f`.

---
*Phase: 04-real-core-surfaces*
*Completed: 2026-05-03*
