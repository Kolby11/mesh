---
phase: 40-migration-diagnostics-and-author-docs
plan: 02
subsystem: author-docs
tags: [module-json, author-docs, frontend-docs, migration]
requires:
  - phase: 40-migration-diagnostics-and-author-docs
    provides: migration diagnostics contract
provides:
  - Canonical module.json installation examples
  - Resource/theme/locale/settings author-doc migration
  - Refreshed frontend and LLM author context
affects: [phase-40, phase-41, module-authors, future-agents]
tech-stack:
  added: []
  patterns: [canonical-module-docs, migration-only-old-terms]
key-files:
  created:
    - .planning/phases/40-migration-diagnostics-and-author-docs/40-02-SUMMARY.md
  modified:
    - docs/installation.md
    - docs/font-system.md
    - docs/theming/themes.md
    - docs/theming/locales.md
    - docs/settings/README.md
    - docs/llm-context.md
    - docs/modules/frontend/core/README.md
    - docs/modules/frontend/examples/README.md
    - docs/frontend/html-css-transition.md
    - docs/module-system.md
key-decisions:
  - "Author docs now treat module.json as the canonical author-facing manifest."
  - "Remaining old manifest references are explicit migration or historical inventory context."
patterns-established:
  - "Docs examples use top-level name/version and nested mesh.apiVersion, mesh.kind, mesh.dependencies, and mesh.contributes."
  - "AI-facing context must name module.json as the canonical author-facing manifest before describing frontend anatomy."
requirements-completed: [MIGR-01]
duration: 9 min
completed: 2026-05-18
---

# Phase 40 Plan 02: Author Documentation Migration Sweep Summary

**Author-facing docs now teach canonical module.json manifests across installation, resources, settings, frontend examples, and LLM context**

## Performance

- **Duration:** 9 min
- **Started:** 2026-05-18T06:40:00Z
- **Completed:** 2026-05-18T06:48:59Z
- **Tasks:** 3
- **Files modified:** 10

## Accomplishments

- Rewrote installation docs around `module.json`, `mesh.apiVersion`, `mesh.kind`, `mesh.implements`, and `mesh.interface`.
- Migrated font, theme, locale, and settings docs away from old manifest examples.
- Refreshed frontend author docs and `docs/llm-context.md` so future work starts from canonical manifest vocabulary.

## Task Commits

Each task was committed atomically:

1. **Task 1: Rewrite installation docs around canonical module.json** - `54e7856`
2. **Task 2: Update resource, theme, locale, and settings docs** - `7c2eb8d`
3. **Task 3: Refresh frontend examples and LLM context for future authors** - `535037e`

## Files Created/Modified

- `docs/installation.md` - Canonical installation manifest examples and migration note.
- `docs/font-system.md` - Font-pack manifest wording updated to `module.json`.
- `docs/theming/themes.md` - Theme module wording updated to `module.json`.
- `docs/theming/locales.md` - Locale and language-pack examples updated to `module.json`.
- `docs/settings/README.md` - Settings schema docs updated to `module.json` and `settings.schema.json`.
- `docs/llm-context.md` - AI-facing manifest and frontend anatomy guidance refreshed.
- `docs/modules/frontend/core/README.md` - Frontend module declaration guidance updated.
- `docs/modules/frontend/examples/README.md` - Example composition snippets updated to `mesh.dependencies` and `mesh.contributes`.
- `docs/frontend/html-css-transition.md` - Custom-tag guidance points to `module.json` contribution metadata.
- `docs/module-system.md` - Corrected a stale backend-kit layout example found by the old-term sweep.

## Decisions Made

- Kept old manifest names only in migration notes, diagnostics tables, and vocabulary inventory.
- Treated `exports.component.tag` as legacy migration wording in frontend docs until a more specific canonical component-export field is finalized.

## Deviations from Plan

None - plan scope executed as written.

**Total deviations:** 0 auto-fixed.
**Impact on plan:** No scope change.

## Issues Encountered

The plan-level old-term sweep found one stale `package.json` layout example in `docs/module-system.md`, which was not listed in the plan's task files but was included in the verification set. I updated it to `module.json` as part of the docs sweep.

## Verification

- Task-level grep checks for installation docs - passed.
- Task-level grep checks for resource/theme/locale/settings docs - passed.
- Task-level grep checks for frontend and LLM context docs - passed.
- Old-term sweep across installation, resource, settings, LLM, module-system, and vocabulary docs - remaining hits are migration guidance or vocabulary inventory.
- `cargo test -p mesh-core-module package::tests` - passed, 43 tests.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Ready for `40-03` keybind migration continuity. The author docs and AI context no longer point new module authors at stale manifest targets.

## Self-Check: PASSED

Plan-level verification passed.

---
*Phase: 40-migration-diagnostics-and-author-docs*
*Completed: 2026-05-18*
