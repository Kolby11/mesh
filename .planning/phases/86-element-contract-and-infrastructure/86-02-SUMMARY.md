---
phase: 86-element-contract-and-infrastructure
plan: 02
subsystem: ui
tags: [mesh-parser, frontend-compiler, luau-events]
requires:
  - phase: 86-01
    provides: Element taxonomy metadata and shared state
provides:
  - Parser recognition for planned native element tags
  - Safe compiler lowering for planned tags
  - Shared value/change handler normalization
affects: [phase-87, phase-88, phase-89, phase-90]
tech-stack:
  added: []
  patterns: [source-tag adapter, safe primitive lowering, shared handler normalization]
key-files:
  created: []
  modified:
    - crates/core/ui/component/src/template.rs
    - crates/core/ui/component/src/parser/markup.rs
    - crates/core/frontend/compiler/src/tags.rs
    - crates/core/frontend/compiler/src/render.rs
key-decisions:
  - "SourceTag remains the parser adapter while mesh-core-elements is documented as the canonical vocabulary."
  - "New planned tags lower to safe existing primitives until later phases add family-specific behavior."
patterns-established:
  - "Planned native tags parse as lowercase elements; PascalCase remains custom-component only."
requirements-completed: [ELEMCORE-02, ELEMCORE-04]
duration: 35min
completed: 2026-05-26
---

# Phase 86 Plan 02 Summary

**Parser/compiler representation for planned native tags with shared Luau value/change handler plumbing**

## Performance

- **Duration:** 35 min
- **Started:** 2026-05-26T16:15:00Z
- **Completed:** 2026-05-26T16:50:00Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- Added planned element tags to `SourceTag` and parser classification.
- Extended reserved PascalCase primitive diagnostics for new native tags.
- Lowered new source tags to safe existing runtime primitives while preserving shipped tag lowering.
- Added shared handler normalization for `oninput`, `onchange`, `onselect`, `onactivate`, and `onopenchange`.

## Task Commits

1. **Task 1-3: Parser/compiler element representation and event plumbing** - `73692b0` (feat)

**Plan metadata:** `797991c` (docs)

## Files Created/Modified

- `crates/core/ui/component/src/template.rs` - Planned native source tag variants and classification.
- `crates/core/ui/component/src/parser/markup.rs` - PascalCase primitive corrections and parser tests.
- `crates/core/frontend/compiler/src/tags.rs` - Safe lowering for planned tags and regression tests.
- `crates/core/frontend/compiler/src/render.rs` - Shared event handler normalization and binding tests.

## Decisions Made

Kept the compiler lowering layer explicit instead of forcing every planned tag to become a new runtime primitive immediately.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

Duplicate `NumberInput` enum entries were caught by compilation and removed.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Later element behavior phases can parse planned tags immediately and incrementally replace safe primitive lowering with specialized behavior.

---
*Phase: 86-element-contract-and-infrastructure*
*Completed: 2026-05-26*
