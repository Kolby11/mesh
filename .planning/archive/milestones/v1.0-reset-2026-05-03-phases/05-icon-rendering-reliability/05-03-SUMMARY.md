---
phase: 05-icon-rendering-reliability
plan: 03
subsystem: diagnostics-plugin
tags: [diagnostics, manifests, icon-requirements, shell]
requires:
  - phase: 05-icon-rendering-reliability
    provides: missing IconResolution behavior from plan 01
provides:
  - missing icon diagnostics dedupe
  - manifest semantic icon requirements
  - non-fatal degraded health hook
affects: [plugin-manifest, shell-diagnostics, surface-health]
tech-stack:
  added: []
  patterns: [dedupe-hashset, nonfatal-degraded-health, dedicated-manifest-section]
key-files:
  created: []
  modified:
    - crates/core/foundation/diagnostics/src/lib.rs
    - crates/core/extension/plugin/src/lib.rs
    - crates/core/extension/plugin/src/manifest.rs
    - crates/core/shell/src/shell/component.rs
    - crates/core/shell/src/shell/mod.rs
    - crates/core/ui/render/src/lib.rs
    - crates/core/ui/render/src/render.rs
key-decisions:
  - "Missing icons degrade plugin health without incrementing error count."
  - "Semantic icon requirements live in `icon_requirements`, separate from `assets.icons` and `dependencies.icon_packs`."
patterns-established:
  - "Diagnostics record missing icons once per plugin plus semantic icon name."
  - "Manifest initializers carry a default `IconRequirementsSection`."
requirements-completed: [ICON-04]
duration: 8 min
completed: 2026-05-03
---

# Phase 05 Plan 03: Missing Icon Diagnostics Summary

**Non-fatal missing-icon diagnostics with manifest-level semantic icon declarations and degraded plugin health**

## Performance

- **Duration:** 8 min
- **Started:** 2026-05-03T14:13:00Z
- **Completed:** 2026-05-03T14:21:00Z
- **Tasks:** 4
- **Files modified:** 7

## Accomplishments
- Added `Diagnostics::record_missing_icon()` with dedupe and degraded health semantics.
- Added `IconRequirementsSection` to normalized plugin manifests and JSON/TOML paths.
- Added shell-level proof that a missing required icon degrades health without unloading the surface.

## Task Commits

1. **Missing icon diagnostics** - `0a59e6b` (feat)

## Files Created/Modified
- `crates/core/foundation/diagnostics/src/lib.rs` - missing icon diagnostic state and tests.
- `crates/core/extension/plugin/src/manifest.rs` - `icon_requirements` schema and parsing tests.
- `crates/core/shell/src/shell/component.rs` - non-fatal missing icon diagnostic hook and shell test.
- `crates/core/shell/src/shell/mod.rs`, `crates/core/ui/render/src/lib.rs`, `crates/core/ui/render/src/render.rs` - manifest initializer updates.

## Decisions Made
- Missing icons are not fatal plugin errors and do not increment `error_count`.
- Manifest `assets.icons` remains asset-path metadata and is not reused for semantic icon requirements.

## Deviations from Plan

Manifest struct changes required updating existing test helper initializers in shell/render modules. This was necessary compile fallout from the new normalized manifest field.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Core surface manifests can now declare semantic icon requirements and missing proof cases can be surfaced through diagnostics.

## Self-Check: PASSED

- `nix develop -c cargo test -p mesh-core-diagnostics -p mesh-core-plugin -p mesh-core-shell` passed.

---
*Phase: 05-icon-rendering-reliability*
*Completed: 2026-05-03*

