---
phase: 37-concept-inventory-and-vocabulary-lock
plan: 01
subsystem: docs
tags: [modules, vocabulary, manifests, planning]
requires:
  - phase: 37-context
    provides: locked hard-replacement vocabulary decisions
provides:
  - Canonical module vocabulary source
  - Old-term inventory with replacement dispositions
  - Reconciled provider and keybind concept boundaries
affects: [phase-38, phase-39, phase-40, module-docs, manifest-normalization]
tech-stack:
  added: []
  patterns: [canonical-vocabulary-doc, old-term-inventory]
key-files:
  created: [docs/module-vocabulary.md]
  modified: [.planning/REQUIREMENTS.md, .planning/ROADMAP.md]
key-decisions:
  - "Public module vocabulary uses hard replacement: old names are replacement debt, not public aliases."
  - "Temporary manifest loaders are internal migration details, not author-facing synonyms."
  - "Provider selection and keybind declarations are preserved as interface/provider and contribution concepts."
patterns-established:
  - "Vocabulary rows include developer wording, end-user wording, and forbidden interpretations."
  - "Old terminology is classified only as replace, remove, or internal-only migration."
requirements-completed: [CONC-01, CONC-02, CONC-03]
duration: 7 min
completed: 2026-05-17
---

# Phase 37 Plan 01: Canonical Module Vocabulary Source Summary

**Canonical module vocabulary with old-name replacement inventory and reconciled provider/keybind model**

## Performance

- **Duration:** 7 min
- **Started:** 2026-05-17T18:22:18Z
- **Completed:** 2026-05-17T18:29:06Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Created `docs/module-vocabulary.md` as the canonical v1.7 vocabulary source.
- Updated CONC-02 and Phase 37 roadmap wording so old names are replacement/internal migration debt, not public compatibility aliases.
- Added v1.1 provider selection and v1.6 keybind declaration reconciliation so later phases preserve prior behavior under the module model.

## Task Commits

Each task was committed atomically:

1. **Task 1: Create canonical terms** - `632d6b0`
2. **Task 2: Add old-term inventory and correct stale planning wording** - `441bcc7`
3. **Task 3: Reconcile provider and keybind decisions** - `46ffc3f`

## Files Created/Modified

- `docs/module-vocabulary.md` - Canonical terms, public naming rules, old-term inventory, prior decision reconciliation, and innovation rules.
- `.planning/REQUIREMENTS.md` - CONC-02 rewritten to remove compatibility-alias target wording.
- `.planning/ROADMAP.md` - Phase 37 goal and success criteria rewritten around replacement/internal migration.

## Decisions Made

- Followed the user correction that old names should be replaced, not maintained as public aliases.
- Kept runtime loader migration possible only as an internal sequencing detail for later phases.

## Deviations from Plan

None - plan executed exactly as written.

**Total deviations:** 0 auto-fixed.
**Impact on plan:** No scope change.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Wave 2 can now update author docs and add the runtime/future-phase handoff against the canonical vocabulary source.

## Self-Check: PASSED

Plan-level verification passed:

- `test -f docs/module-vocabulary.md`
- `rg -n "A module is the installable MESH unit|Old names are replacement debt|Prior Decision Reconciliation|Innovation Rules" docs/module-vocabulary.md`
- Negative check for `explicit compatibility aliases|compatibility aliases` in `.planning/REQUIREMENTS.md` and `.planning/ROADMAP.md`

---
*Phase: 37-concept-inventory-and-vocabulary-lock*
*Completed: 2026-05-17*
