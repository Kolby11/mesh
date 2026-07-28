---
phase: 14-profiling-data-model-and-timing-hooks
plan: 03
subsystem: runtime-stage-instrumentation
tags: [profiling, runtime, render, input]

requires:
  - phase: 14-01
    provides: Debug-only profiling contract and toggle path
  - phase: 14-02
    provides: Collector/session storage and snapshot wiring

provides:
  - Real input/runtime/render stage hooks feeding the profiling collector
  - Component-local stage records for tree build, style/restyle, layout, and paint
  - Shell-wide timing hooks for input handling, runtime updates, present/commit, redraw count, and total surface render time

affects:
  - phase-14-snapshot-proof

tech-stack:
  added: []
  patterns:
    - "Fine-grained per-surface stage timing is recorded locally and harvested by the shell render loop"
    - "Shell-wide stage timing stays in runtime entrypoints rather than being inferred after the fact"

key-files:
  created:
    - .planning/phases/14-profiling-data-model-and-timing-hooks/14-03-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/runtime/mod.rs
    - crates/core/shell/src/shell/runtime/wayland.rs
    - crates/core/shell/src/shell/runtime/request.rs
    - crates/core/shell/src/shell/runtime/render.rs
    - crates/core/shell/src/shell/component/rendering.rs
    - crates/core/shell/src/shell/component/shell_component.rs

key-decisions:
  - "Frontend surfaces collect `tree_build`, `style_restyle`, `layout`, and `paint` records locally so the shell can roll them up without global mutable state."
  - "The shell render loop owns `present_commit`, `redraw_count`, and `total_surface_render` accounting."
  - "Wayland event routing and request/message handling now emit shell-wide `input_handling` and `runtime_update_handling` timings when profiling is enabled."

requirements-completed: [TIME-01, PROF-03]

duration: 1 session
completed: 2026-05-08
---

# Phase 14 Plan 03: Real Stage Instrumentation Across Input and Render Paths

**Phase 14 now measures the real runtime seams for the required top-level profiling stages instead of relying on coarse outer render timing only.**

## Accomplishments

- Added component-local profiling records for `tree_build`, `style_restyle`, `layout`, and `paint`.
- Updated the shell render loop to harvest per-surface stage records and roll them into shell-wide and per-surface summaries, including `present_commit`, `redraw_count`, and `total_surface_render`.
- Added shell-wide timing hooks around Wayland input dispatch, request application, and runtime message processing.

## Task Commits

Pending commit in this workspace run. The implementation is validated and committed immediately after this summary.

## Verification

- `grep -n 'profil\|TreeBuild\|StyleRestyle\|Layout\|Paint\|PresentCommit\|TotalSurfaceRender\|RedrawCount' crates/core/shell/src/shell/runtime/mod.rs crates/core/shell/src/shell/runtime/wayland.rs crates/core/shell/src/shell/runtime/request.rs crates/core/shell/src/shell/runtime/render.rs crates/core/shell/src/shell/component/rendering.rs crates/core/shell/src/shell/component/shell_component.rs`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_stage`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_disabled`

## Deviations From Plan

None. The instrumentation work stayed in the planned runtime and component seams.

## Self-Check: PASSED

- Summary file exists.
- The required stage hooks are present in the planned runtime/component files.
- Focused profiling stage and disabled-mode tests passed.

---
*Phase: 14-profiling-data-model-and-timing-hooks*
*Completed: 2026-05-08*
