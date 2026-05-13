---
phase: 28-incremental-paint-command-retention
plan: 01
subsystem: rendering
tags: [retained-rendering, display-list, dirty-subtree, profiling, cpu-render]
requires:
  - phase: 26-cpu-render-profiling-and-baseline-proof
    provides: retained paint profiling and invalidation snapshot pipeline
  - phase: 27-viewport-culling-and-visibility-elision
    provides: clipped-subtree omission and aggregate retained paint counters
provides:
  - dirty-subtree retained paint-command ownership inside mesh-core-render
  - local subtree refresh for transform, scroll, and reorder updates
  - aggregate subtree reuse and fallback proof on the existing invalidation payload
affects: [29, 30, 31, retained-rendering, paint-traversal]
tech-stack:
  added: []
  patterns: [subtree command cache, ancestor-path rebuild with sibling reuse, aggregate-only fallback proof]
key-files:
  created:
    - .planning/phases/28-incremental-paint-command-retention/28-01-SUMMARY.md
    - .planning/phases/28-incremental-paint-command-retention/28-VERIFICATION.md
  modified:
    - crates/core/frontend/render/src/render_object.rs
    - crates/core/frontend/render/src/display_list.rs
    - crates/core/foundation/debug/src/lib.rs
    - crates/core/shell/src/shell/component.rs
    - crates/core/shell/src/shell/component/shell_component.rs
    - crates/core/shell/src/shell/runtime/debug.rs
    - crates/core/shell/src/shell/tests.rs
key-decisions:
  - "Retained paint-command ownership now lives in per-node subtree caches inside mesh-core-render while the painter still consumes one flat command slice."
  - "Dirty-node IDs from render-object sync drive local subtree refresh; ambiguous or broad dirty summaries take a conservative full fallback path."
  - "Phase 28 proof stays aggregate-only and extends the existing invalidation.paint payload instead of introducing per-command tracing."
patterns-established:
  - "Rebuild the dirty subtree and its ancestor composition path, but reuse unrelated sibling subtree command payloads verbatim."
  - "Scroll offsets count as geometry-affecting retained paint changes so subtree-local refresh stays driven by render-object dirty signals."
requirements-completed: ["PIPE-01", "PIPE-02"]
duration: unknown
completed: 2026-05-11
---

# Phase 28 Plan 01: Dirty-subtree retained paint command ownership and local reuse proof

**Retained paint-command ownership now uses dirty-subtree caches inside `mesh-core-render`, so local transform, scroll, and reorder updates can rebuild only the affected subtree path while preserving unrelated sibling command payloads and exposing aggregate reuse or fallback proof through `mesh.debug`.**

## Performance

- **Completed:** 2026-05-11
- **Tasks:** 3
- **Files modified:** 7 runtime files, plus phase tracking artifacts

## Accomplishments

- Added dirty-subtree retained paint-command caches keyed by `NodeId` inside `crates/core/frontend/render/src/display_list.rs` while preserving the painter’s flat `paint_commands()` boundary.
- Added a local subtree refresh path that rebuilds dirty subtrees plus required ancestors and reuses unrelated sibling subtree command payloads for transform, scroll, and local reorder updates.
- Added conservative fallback accounting for ambiguous or broad dirty summaries instead of attempting unsafe local reuse.
- Extended `RenderObjectTree` paint-affecting geometry tracking so scroll offset changes surface through `dirty_node_ids()`.
- Extended `RetainedPaintSnapshot` and the shell debug JSON with aggregate subtree reuse, subtree rebuild, rebuilt-command, and fallback counters.
- Added focused render tests for transform-only, scroll-only, reorder-only sibling reuse and for ambiguous-dirty fallback behavior, plus a shell profiling assertion for the new debug payload fields.

## Task Commits

No phase-scoped task commits were created during this inline Phase 28 execution. Verification and closeout were performed against the current working tree at `9633dbb`.

## Files Created/Modified

- `crates/core/frontend/render/src/render_object.rs` now treats scroll offsets and content extents as geometry-affecting retained paint inputs so local retained refresh sees scroll dirtiness through `dirty_node_ids()`.
- `crates/core/frontend/render/src/display_list.rs` now owns per-node retained paint subtrees, local reuse or fallback decisions, aggregate subtree metrics, and focused local-reuse tests.
- `crates/core/foundation/debug/src/lib.rs`, `crates/core/shell/src/shell/component.rs`, and `crates/core/shell/src/shell/runtime/debug.rs` now propagate subtree reuse and fallback proof through `invalidation.paint`.
- `crates/core/shell/src/shell/component/shell_component.rs` now passes render-object dirty summaries and dirty-node IDs into retained display-list updates.
- `crates/core/shell/src/shell/tests.rs` now locks the new aggregate counters in the serialized `mesh.debug` profiling payload.

## Decisions Made

- Dirty-subtree reuse stays local to `mesh-core-render`; shell code only forwards aggregate proof counters.
- Local refresh is conservative: broad or ambiguous dirty summaries increment fallback proof and rebuild the full surface path.
- The reuse boundary is sibling-subtree preservation, not global per-command diffing or Phase 29 damage filtering.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None.

## Next Phase Readiness

- Phase 29 can now build damage-indexed execution on top of stable dirty-subtree paint-command ownership instead of a whole-surface recollection step.
- Phase 30 can rely on the new subtree reuse counters when proving raster-cache wins against retained paint churn.

## Self-Check: PASSED

- Verified `.planning/phases/28-incremental-paint-command-retention/28-01-SUMMARY.md` and `.planning/phases/28-incremental-paint-command-retention/28-VERIFICATION.md` exist on disk.
- Verified `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check` passed.
- Verified `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render render_object` passed.
- Verified `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` passed.
- Verified `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` passed.
