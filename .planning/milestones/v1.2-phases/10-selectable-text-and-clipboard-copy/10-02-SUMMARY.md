---
phase: 10-selectable-text-and-clipboard-copy
plan: 02
subsystem: rendering
tags: [selection, rendering, cosmic-text, theme]
requires:
  - phase: 10-01
    provides: shell-owned selection state, modifier-aware input routing
provides:
  - wrapped selection geometry derived from `cosmic-text` hit testing and highlight spans
  - theme-owned selection foreground/background paint for selected text ranges
  - minimal shell-to-render selection annotation bridge for the active text node
affects: [10-03, clipboard, paint, theme]
tech-stack:
  added: []
  patterns: [cosmic-text selection geometry, clipped second-pass selection paint]
key-files:
  created: []
  modified:
    - crates/core/ui/render/src/surface/text.rs
    - crates/core/ui/render/src/surface/painter.rs
    - crates/core/shell/src/shell/component/rendering.rs
    - config/themes/mesh-default-dark.json
key-decisions:
  - "Selection geometry and highlight rectangles come from the same `cosmic-text` layout pass so copy and paint stay aligned."
  - "The painter renders selected foreground by clipping a second text pass to the highlighted run rectangles instead of inventing a separate glyph renderer."
patterns-established:
  - "Render-time selection metadata is attached to the selected text node as concrete attributes before paint."
  - "Phase 10 rejects clipped or ellipsized text from the selection path instead of trying to copy hidden text."
requirements-completed: [TEXT-01, TEXT-02, TEXT-04]
duration: 9 min
completed: 2026-05-06
---

# Phase 10 Plan 02: Wrapped Selection Geometry and Highlight Rendering Summary

**Wrapped single-node selection geometry with theme-owned highlight paint built on shared `cosmic-text` layout data**

## Performance

- **Duration:** 9 min
- **Started:** 2026-05-06T09:55:49Z
- **Completed:** 2026-05-06T10:04:52Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Added a reusable `TextRenderer::selection_geometry` helper that maps pointer coordinates to stable `Cursor` ranges, extracts UTF-8-safe selected text, and produces wrapped highlight rectangles from the same `cosmic-text` layout runs used for paint.
- Extended the painter to render selection background and foreground using theme-owned colors, with highlight rectangles clipped to the selected range so neighboring text remains untouched.
- Added default dark theme selection tokens and a small shell-side annotation step that passes concrete selection colors and coordinates into the selected text node before render.

## Task Commits

Each task was committed atomically within the plan work commit:

1. **Task 1: Add single-node wrapped selection geometry helpers** - `a1acd86` (`feat`)
2. **Task 2: Paint selected ranges with theme-owned selection tokens** - `a1acd86` (`feat`)

**Plan metadata:** recorded in the plan-completion docs commit for `10-02`.

## Files Created/Modified

- `crates/core/ui/render/src/surface/text.rs` - added wrapped selection geometry, UTF-8-safe extraction, and tests for multi-line highlights.
- `crates/core/ui/render/src/surface/painter.rs` - painted selected text via theme-owned colors and range-clipped highlight rectangles, with neighbor-safety tests.
- `crates/core/shell/src/shell/component/rendering.rs` - annotated the selected text node with concrete selection metadata so the painter can stay tree-driven.
- `config/themes/mesh-default-dark.json` - introduced `color.selection-background` and `color.selection-foreground`.

## Decisions Made

- Used `LayoutRun::highlight` plus a clipped second text pass instead of building a bespoke selected-glyph draw path, which keeps Phase 10 aligned with the existing renderer.
- Kept the selection handoff tree-local by annotating node attributes, avoiding a broader public render API expansion in the middle of the phase.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- The existing render tree did not expose shell-owned selection state to the painter directly. A minimal annotation hook in shell rendering resolved that gap without widening the render engine interface.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Wave 2 is complete and the render path now exposes wrapped highlight geometry and selection colors for Wave 3’s clipboard payload and proof fixture work.
- Verified commands:
  - `nix develop -c cargo test -p mesh-core-render selection_geometry`
  - `nix develop -c cargo test -p mesh-core-render selection_paint`
  - `nix develop -c cargo test -p mesh-core-shell -p mesh-core-render selection`

---
*Phase: 10-selectable-text-and-clipboard-copy*
*Completed: 2026-05-06*
