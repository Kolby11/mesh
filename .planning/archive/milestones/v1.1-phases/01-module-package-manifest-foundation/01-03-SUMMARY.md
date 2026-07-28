---
phase: 01-plugin-package-manifest-foundation
plan: 03
subsystem: package-management
tags: [fixtures, shell, settings, themes]
requires:
  - phase: 01-plugin-package-manifest-foundation
    provides: "InstalledModuleGraph and package manifest loader"
provides:
  - "Repo fixture mirroring ~/.mesh/package.json"
  - "load_installed_module_graph"
  - "Shell proof test for active provider and layout entrypoint"
affects: [backend-lifecycle, shell-runtime, documentation]
tech-stack:
  added: []
  patterns: ["Repo fixture fallback before ~/.mesh", "Shell-facing package graph proof without lifecycle migration"]
key-files:
  created:
    - "config/package.json"
    - "config/modules/@mesh/panel/package.json"
    - "config/modules/@mesh/quick-settings/package.json"
    - "config/modules/@mesh/pipewire-audio/package.json"
    - "config/modules/@mesh/pulseaudio-audio/package.json"
    - "config/modules/@mesh/shell-theme/package.json"
  modified:
    - "crates/core/extension/plugin/src/package.rs"
    - "crates/core/foundation/config/src/lib.rs"
    - "crates/core/foundation/theme/src/lib.rs"
    - "crates/core/shell/src/shell/mod.rs"
    - "docs/settings/README.md"
    - "docs/theming/themes.md"
key-decisions:
  - "Phase 1 proves graph visibility in shell code but leaves backend lifecycle consumption to Phase 2."
  - "Development keeps repo config/theme fallbacks while conceptual user paths are ~/.mesh."
patterns-established:
  - "Root package resolves module package paths relative to modulesDir."
  - "Theme modules advertise selectable modes through mesh.contributes.themes."
requirements-completed: [PINST-01, PINST-04, PINST-05, PINST-06]
duration: 25min
completed: 2026-05-03
---

# Phase 1 Plan 03 Summary

**Repo-local ~/.mesh package fixture with shell graph loading proof and ~/.mesh settings/theme path direction**

## Performance

- **Duration:** 25 min
- **Started:** 2026-05-03T18:15:00+02:00
- **Completed:** 2026-05-03T18:40:00+02:00
- **Tasks:** 3
- **Files modified:** 12

## Accomplishments

- Added `config/package.json` plus five module `package.json` fixtures matching the target `~/.mesh/modules` shape.
- Added `load_installed_module_graph` and package/shell tests proving active audio provider and base layout entrypoint resolution.
- Updated config/theme helpers and docs toward `~/.mesh/settings.json` and `~/.mesh/themes/`.

## Task Commits

1. **Module package shell fixtures** - `555d110` (feat)
2. **Quick settings fixture requirements** - `43dca56` (fix)

## Files Created/Modified

- `config/package.json` - Repo fixture for `~/.mesh/package.json`.
- `config/modules/@mesh/*/package.json` - Module fixtures for panel, quick settings, audio providers, and shell theme.
- `crates/core/extension/plugin/src/package.rs` - Root package graph loader and fixture test.
- `crates/core/shell/src/shell/mod.rs` - Shell proof test.
- `crates/core/foundation/config/src/lib.rs` - `~/.mesh/settings.json` fallback path.
- `crates/core/foundation/theme/src/lib.rs` - `~/.mesh/themes/` fallback path.
- `docs/settings/README.md` and `docs/theming/themes.md` - Module/package path documentation updates.

## Decisions Made

The shell runtime still discovers legacy plugin directories. This plan intentionally adds proof coverage only, because replacing backend lifecycle discovery is Phase 2 work and depends on the completed graph.

## Deviations from Plan

The quick-settings fixture initially omitted `mesh.network` and `mesh.power`; the review gate caught this acceptance gap and `43dca56` added both requirements.

## Issues Encountered

The first theming doc patch had stale context; it was reapplied as a narrower patch. The quick-settings fixture acceptance gap was corrected before phase verification.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 2 can consume `InstalledModuleGraph` in backend lifecycle startup instead of relying only on implicit plugin directory scanning and priority fallback.

---
*Phase: 01-plugin-package-manifest-foundation*
*Completed: 2026-05-03*
