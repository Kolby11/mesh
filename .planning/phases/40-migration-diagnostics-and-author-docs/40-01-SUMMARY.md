---
phase: 40-migration-diagnostics-and-author-docs
plan: 01
subsystem: module-runtime
tags: [manifest-diagnostics, module-json, migration, author-docs]
requires:
  - phase: 37-concept-inventory-and-vocabulary-lock
    provides: replacement/removal vocabulary and runtime inventory
  - phase: 38-canonical-manifest-normalization
    provides: canonical module.json manifest loader
  - phase: 39-contribution-and-interface-extension-index
    provides: contribution index behavior to preserve
provides:
  - Manifest diagnostic severity regression coverage
  - Author-facing migration diagnostics contract
  - No-alias wording for old manifest inputs
affects: [phase-40, phase-41, module-authors, manifest-loader]
tech-stack:
  added: []
  patterns: [structured-diagnostic-assertions, replacement-removal-docs]
key-files:
  created:
    - .planning/phases/40-migration-diagnostics-and-author-docs/40-01-SUMMARY.md
  modified:
    - crates/core/extension/module/src/package/tests.rs
    - docs/module-system.md
    - docs/module-vocabulary.md
key-decisions:
  - "Old manifest inputs remain migration debt, not public aliases."
  - "Author docs now mirror the runtime diagnostic severity contract."
patterns-established:
  - "Manifest loader tests assert ModuleManifestDiagnosticSeverity and suggested_action instead of only display text."
  - "Migration docs use replacement/removal wording for every old manifest input."
requirements-completed: [MIGR-01]
duration: 16 min
completed: 2026-05-18
---

# Phase 40 Plan 01: Migration Diagnostic Contract Summary

**Manifest migration diagnostics now have test coverage for warning/error severity and author docs with exact replacement/removal actions**

## Performance

- **Duration:** 16 min
- **Started:** 2026-05-18T06:23:00Z
- **Completed:** 2026-05-18T06:39:47Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Added regression tests that assert structured diagnostic severity and suggested actions for `plugin.json`, ambiguous manifests, legacy `package.json`, legacy `module.json`, and `mesh.toml`.
- Added a `Migration Diagnostics` author-doc table that distinguishes migration warnings from blocking load errors.
- Preserved the Phase 37 vocabulary rule that old manifest names are replacement debt and internal migration inputs, not public aliases.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add severity-focused manifest diagnostic regression tests** - `5ae7a9f`
2. **Task 2: Document the migration diagnostics contract** - `524df16`
3. **Task 3: Prove old manifest names are not documented as aliases** - `95be906`

## Files Created/Modified

- `crates/core/extension/module/src/package/tests.rs` - Asserts diagnostic `Error` vs `Warning` severity and exact suggested actions.
- `docs/module-system.md` - Documents migration diagnostics in an author-facing table.
- `docs/module-vocabulary.md` - Points Phase 40 handoff to the replacement/removal diagnostics contract.
- `.planning/phases/40-migration-diagnostics-and-author-docs/40-01-SUMMARY.md` - Records this plan outcome.

## Decisions Made

- Kept old manifest names documented only as replacement/removal targets.
- Treated `plugin.json` and ambiguous manifests as blocking errors, while legacy accepted manifests remain warnings.

## Deviations from Plan

None - plan scope executed as written.

**Total deviations:** 0 auto-fixed.
**Impact on plan:** No scope change.

## Issues Encountered

The task 1 verification command `cargo test -p mesh-core-module package::tests diagnostic` is not valid Cargo syntax because it passes two test filters. I ran the focused `diagnostic` filter and the broader `cargo test -p mesh-core-module package::tests`; the full package test suite passed.

## Verification

- `cargo test -p mesh-core-module diagnostic` - passed.
- `cargo test -p mesh-core-module package::tests` - passed, 43 tests.
- `rg -n "Migration Diagnostics|replace package.json with module.json|replace legacy module.json fields with name/version/mesh|replace mesh.toml with module.json|remove plugin.json or replace it with module.json|keep canonical module.json and remove the old manifest file" docs/module-system.md docs/module-vocabulary.md` - passed.
- `rg -n "internal migration input|replacement debt" docs/module-system.md docs/module-vocabulary.md` - passed.
- `rg -n "package.json.*alias|plugin.json.*alias|mesh.toml.*alias|package.json.*synonym|compatible name" docs/module-system.md docs/module-vocabulary.md crates/core/extension/module/src/package/tests.rs` - exited 1 as expected.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Ready for `40-02` author documentation migration. The runtime contract and author-facing diagnostic table are established.

## Self-Check: PASSED

Plan-level verification passed with the Cargo filter correction noted above.

---
*Phase: 40-migration-diagnostics-and-author-docs*
*Completed: 2026-05-18*
