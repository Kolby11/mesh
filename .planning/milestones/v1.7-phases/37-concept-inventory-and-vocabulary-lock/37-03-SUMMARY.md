---
phase: 37-concept-inventory-and-vocabulary-lock
plan: 03
subsystem: docs
tags: [runtime-inventory, roadmap, manifests, contributions]
requires:
  - phase: 37-01
    provides: canonical module vocabulary source
provides:
  - Runtime inventory for old and canonical terms
  - Phase 38-41 vocabulary handoff
  - Roadmap wording aligned to hard replacement
affects: [phase-38, phase-39, phase-40, phase-41]
tech-stack:
  added: []
  patterns: [runtime-vocabulary-inventory, future-phase-handoff]
key-files:
  created: []
  modified:
    - docs/module-vocabulary.md
    - .planning/ROADMAP.md
key-decisions:
  - "Runtime rename work is deferred to later phases with behavior-preservation notes."
  - "Phase 38 targets module.json, with old loaders only as internal migration paths."
  - "Phase 40 diagnostics should say replace with or remove, not alias."
patterns-established:
  - "Runtime inventory rows include location, current term, target term, disposition, follow-up phase, and behavior to preserve."
  - "Future phase handoffs explicitly name the vocabulary obligations they inherit."
requirements-completed: [CONC-02, CONC-03]
duration: 3 min
completed: 2026-05-17
---

# Phase 37 Plan 03: Runtime Inventory And Future-Phase Handoff Summary

**Runtime terminology inventory and Phase 38-41 handoff for module.json, typed contributions, diagnostics, and shipped proof**

## Performance

- **Duration:** 3 min
- **Started:** 2026-05-17T18:32:44Z
- **Completed:** 2026-05-17T18:35:13Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- Added a runtime inventory covering key package-era structs, manifest shapes, provider fields, keybind migration inputs, and installed-graph contribution structures.
- Added Phase 38-41 handoff rules for `module.json`, internal-only migration loaders, typed contribution indexes, replacement/removal diagnostics, and shipped proof.
- Updated the v1.7 roadmap so future phases inherit hard replacement wording instead of `package.json.mesh` or compatibility-alias framing.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add runtime inventory** - `c5dd3bc`
2. **Task 2: Add Phase 38-41 handoff rules** - `3285ada`
3. **Task 3: Align future roadmap wording** - `66fe3e8`

## Files Created/Modified

- `docs/module-vocabulary.md` - Runtime inventory and future-phase handoff sections.
- `.planning/ROADMAP.md` - Phase 38 and Phase 40 wording aligned to `module.json` and replacement/removal guidance.

## Decisions Made

- Kept Phase 37 as an inventory and handoff phase rather than performing runtime renames prematurely.
- Preserved existing provider and keybind behavior as behavior-to-preserve constraints for Phase 38-40.

## Deviations from Plan

None - plan executed exactly as written.

**Total deviations:** 0 auto-fixed.
**Impact on plan:** No scope change.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 38 can now plan manifest normalization from explicit `module.json`, internal migration loader, runtime type rename, and behavior-preservation guidance.

## Self-Check: PASSED

Plan-level verification passed:

- `Runtime Inventory`, `Future-Phase Handoff`, `ModulePackageManifest`, `RootPackageManifest`, `localized_triggers`, and `typed contribution indexes` are present in `docs/module-vocabulary.md`.
- `module.json`, `replacement/removal guidance`, and `old public names are replacement debt` are present in `.planning/ROADMAP.md`.
- `.planning/ROADMAP.md` no longer contains `package.json.mesh` or `compatibility aliases`.

---
*Phase: 37-concept-inventory-and-vocabulary-lock*
*Completed: 2026-05-17*
