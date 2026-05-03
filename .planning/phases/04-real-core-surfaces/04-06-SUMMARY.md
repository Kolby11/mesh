---
phase: 04-real-core-surfaces
plan: 06
subsystem: frontend-shell-surfaces
tags:
  - gap-closure
  - shell-routing
  - docs
requires:
  - phase: 04-real-core-surfaces
    plan: 04
provides:
  - Supported quick-settings toggle event
  - Supported quick-settings close event
  - Frontend docs aligned with shipped routing
affects:
  - packages/plugins/frontend/core/panel/src/main.mesh
  - packages/plugins/frontend/core/quick-settings/src/main.mesh
  - docs/plugins/frontend/core/README.md
  - crates/core/shell/src/shell/component.rs
tech-stack:
  added: []
  patterns:
    - Shell UI transitions use shell.*-surface events with surface_id
key-files:
  created: []
  modified:
    - packages/plugins/frontend/core/panel/src/main.mesh
    - packages/plugins/frontend/core/quick-settings/src/main.mesh
    - docs/plugins/frontend/core/README.md
    - crates/core/shell/src/shell/component.rs
key-decisions:
  - Panel quick-settings entry uses `shell.toggle-surface` with `surface_id`.
  - Quick settings close uses `shell.hide-surface` with `surface_id`.
requirements-completed:
  - SURF-01
  - SURF-02
  - SURF-03
  - SURF-04
  - SURF-05
duration: 4 min
completed: 2026-05-03
---

# Phase 04 Plan 06: Shell Surface Routing and Docs Summary

Panel and quick-settings now use supported shell surface events, and docs teach the same routing contract.

## Execution

- **Duration:** 4 min
- **Started:** 2026-05-03T07:21:00Z
- **Completed:** 2026-05-03T07:25:33Z
- **Tasks:** 3
- **Files modified:** 4

## What Changed

- Changed panel volume click to publish `shell.toggle-surface` with `surface_id = "@mesh/quick-settings"`.
- Changed quick-settings close to publish `shell.hide-surface` with the same `surface_id`.
- Updated frontend docs to teach supported shell surface events while preserving named service proxy mutation examples.
- Updated final surface regressions to expect `ToggleSurface`/`HideSurface` routing and keep callback-free checks.

## Commits

| Commit | Description |
|--------|-------------|
| 29f29a0 | Closed Phase 4 runtime, provider, and surface routing gaps. |

## Verification

- Static `rg` checks confirmed shipped surfaces, docs, and final surface tests use `shell.toggle-surface`/`shell.hide-surface` with `surface_id = "@mesh/quick-settings"`.
- `rg -n "shell\\.(toggle|close)-quick-settings" packages/plugins/frontend/core/panel/src/main.mesh packages/plugins/frontend/core/quick-settings/src/main.mesh docs/plugins/frontend/core/README.md crates/core/shell/src/shell/component.rs` returned no matches.
- `cargo test -p mesh-core-shell real_core_surfaces -- --nocapture` was blocked before shell tests ran because `smithay-client-toolkit` requires missing `xkbcommon.pc`.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- Shell crate tests cannot compile in this host environment until the `xkbcommon.pc` development package is available.

## Self-Check: PASSED

- Summary file exists.
- Key modified files exist.
- Static routing and docs checks passed.
- Shell runtime test is pending the known host package blocker.

