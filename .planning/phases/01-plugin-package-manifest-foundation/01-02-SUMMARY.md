---
phase: 01-plugin-package-manifest-foundation
plan: 02
subsystem: package-management
tags: [module-graph, providers, dependencies, contributions]
requires:
  - phase: 01-plugin-package-manifest-foundation
    provides: "RootPackageManifest, ModulePackageManifest, and load_module_manifest"
provides:
  - "InstalledModuleGraph"
  - "Backend provider resolution APIs"
  - "Contribution index for layout, theme, icons, fonts, i18n, and settings"
affects: [backend-lifecycle, shell-runtime, settings-ui]
tech-stack:
  added: []
  patterns: ["Single canonical module map", "mesh.contributes resource indexing"]
key-files:
  created: []
  modified: ["crates/core/extension/plugin/src/package.rs"]
key-decisions:
  - "Keep provider alternatives and selected provider separate."
  - "Use mesh.contributes as the common resource/settings extension point."
patterns-established:
  - "Frontend requirements are indexed from module dependencies."
  - "Provider fallback sorts by priority descending with deterministic ID tie-break."
requirements-completed: [PINST-02, PINST-03, PINST-04, PINST-05]
duration: 30min
completed: 2026-05-03
---

# Phase 1 Plan 02 Summary

**Installed module graph with frontend requirements, backend provider choices, and normalized contribution indexes**

## Performance

- **Duration:** 30 min
- **Started:** 2026-05-03T17:45:00+02:00
- **Completed:** 2026-05-03T18:15:00+02:00
- **Tasks:** 3
- **Files modified:** 1

## Accomplishments

- Added `InstalledModuleGraph` and module-kind views backed by one module map.
- Indexed frontend backend requirements and multiple backend providers per interface.
- Added active/fallback provider APIs and contribution accessors for layout, themes, icons, fonts, i18n, and settings.

## Task Commits

1. **Installed module graph** - `cf7a152` (feat)

## Files Created/Modified

- `crates/core/extension/plugin/src/package.rs` - Graph construction, provider validation, requirement indexing, contribution indexing, and tests.

## Decisions Made

The graph rejects root provider selections that point at missing, disabled, non-backend, or wrong-interface modules. Contribution paths reuse the relative/no-parent path validation used by manifests.

## Deviations from Plan

None - plan scope was implemented as written, with tests grouped under the common `installed_module_graph` filter.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Plan 03 can load a repo fixture through `load_installed_module_graph` and prove shell-facing access to active providers and layout entrypoints.

---
*Phase: 01-plugin-package-manifest-foundation*
*Completed: 2026-05-03*
