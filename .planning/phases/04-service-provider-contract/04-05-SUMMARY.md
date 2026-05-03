---
phase: 04-service-provider-contract
plan: 05
subsystem: shell-runtime
tags: [rust, theme, service-state, backend-provider, tests]

requires:
  - phase: 04-service-provider-contract
    provides: Service provider latest-state storage, shell-authored mesh.theme updates, and generic backend provider routing from plans 04-01 through 04-04.
provides:
  - Shell-theme backend startup settings seeded from the resolved active theme id.
  - File-watch theme recovery synchronized through mesh.theme latest state and component service events.
  - Regression coverage for fallback startup, backend replacement, and recovered theme files.
affects: [service-provider-contract, shell-theme, backend-lifecycle, BSVC-03]

tech-stack:
  added: []
  patterns:
    - Shell-owned mesh.theme state remains authoritative when fallback or recovery changes the active theme.
    - Backend launch settings use resolved runtime state instead of raw configured settings.

key-files:
  created:
    - .planning/phases/04-service-provider-contract/04-05-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/mod.rs

key-decisions:
  - "Shell-theme backend settings are derived from ThemeEngine.active().id so provider startup and restart match the shell's resolved theme authority."
  - "Theme file-watch reload returns pending CoreRequest queues and synchronizes mesh.theme only when the active theme id actually changes."

patterns-established:
  - "Resolved runtime authority beats raw user configuration for provider seed state."
  - "Theme recovery paths update shell state, latest service state, backend command payload, and component events together."

requirements-completed: [BSVC-03]

duration: 4min
completed: 2026-05-03
---

# Phase 04 Plan 05: Shell Theme Fallback State Alignment Summary

**Shell-theme fallback startup, backend replacement, and file-watch recovery now keep mesh.theme latest state aligned with the resolved active theme id.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-05-03T22:03:54Z
- **Completed:** 2026-05-03T22:08:11Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- Seeded `@mesh/shell-theme` backend `current_theme` from `self.theme.active().id` instead of the raw configured theme id.
- Changed active theme file reload to return pending requests and call `sync_theme_service_state()` when a recovered theme changes the resolved active id.
- Added regressions for missing-theme fallback backend replacement and later file-watch recovery updating `mesh.theme.current`, `is_dark`, and component payloads.

## Task Commits

Each task was committed atomically:

1. **Task 1: Seed shell-theme backend settings from the resolved active theme** - `8e5bfab` (fix)
2. **Task 2: Synchronize mesh.theme state when file-watch reload changes the active theme** - `a034a35` (fix)
3. **Task 3: Add regressions for fallback restart and missing-theme file recovery** - `c6611ff` (test)

## Files Created/Modified

- `crates/core/shell/src/shell/mod.rs` - Runtime theme fallback synchronization and regression coverage.
- `.planning/phases/04-service-provider-contract/04-05-SUMMARY.md` - Execution summary and verification record.

## Decisions Made

- Shell-theme backend seed settings now use the resolved active theme from `ThemeEngine`, preserving fallback authority during startup and backend replacement.
- File-watch recovery synchronizes `mesh.theme` only when the active theme id changes, preserving the existing metadata short-circuit and avoiding unnecessary service churn.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## Verification

- `nix develop -c cargo test -p mesh-core-shell shell_theme_backend_candidate_receives` - passed
- `nix develop -c cargo test -p mesh-core-shell settings_theme_reload` - passed
- `nix develop -c cargo test -p mesh-core-shell theme` - passed

## Known Stubs

None.

## Auth Gates

None.

## Next Phase Readiness

BSVC-03 fallback exception paths are closed for `mesh.theme`: backend startup/replacement and file-watch recovery now preserve latest-state alignment with the shell's resolved active theme.

## Self-Check: PASSED

- Found `.planning/phases/04-service-provider-contract/04-05-SUMMARY.md`.
- Found `crates/core/shell/src/shell/mod.rs`.
- Found task commits `8e5bfab`, `a034a35`, and `c6611ff`.

---
*Phase: 04-service-provider-contract*
*Completed: 2026-05-03*
