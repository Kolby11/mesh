---
phase: 12-theme-animation-tokens-and-css-animations
plan: 04
subsystem: shell-runtime-integration
tags: [shell, diagnostics, rebuilds, keyframes]

requires:
  - phase: 12-01
    provides: Strict animation token diagnostics
  - phase: 12-02
    provides: Validated parsed keyframe rules
  - phase: 12-03
    provides: Renderer playback primitives

provides:
  - Active keyframe state preserved by stable `_mesh_key` plus animation name
  - Runtime diagnostics for unresolved animation names and invalid animation token references
  - Dirty-frame control for finite versus infinite keyframe animations

affects:
  - phase-12-docs-and-proof
  - phase-13-navigation-bar-proof

tech-stack:
  added: []
  patterns:
    - "Stable `_mesh_key` preserves in-flight animation timelines across rebuilds"
    - "Runtime animation diagnostics surface missing names and invalid token references instead of failing silently"

key-files:
  created:
    - .planning/phases/12-theme-animation-tokens-and-css-animations/12-04-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/component.rs
    - crates/core/shell/src/shell/component/animation.rs
    - crates/core/shell/src/shell/component/shell_component.rs
    - crates/core/shell/src/shell/component/tests.rs
    - crates/core/ui/render/src/animation/keyframes.rs

key-decisions:
  - "Shell-owned keyframe identity is `_mesh_key` plus animation name so rebuilds do not restart stable animations."
  - "Finite animations must stop requesting redraw once their final frame is complete, while infinite animations stay render-active."
  - "Runtime diagnostics remain part of the surface-component contract for unresolved animation names and unresolved animation tokens."

requirements-completed: [ANIM-02, ANIM-04, ANIM-05]

duration: 1 session
completed: 2026-05-08
---

# Phase 12 Plan 04: Shell Keyframe Integration And Runtime Diagnostics

**Live shell components now preserve keyframe timelines across rebuilds, stop finite redraw churn correctly, and surface runtime animation diagnostics when author references are invalid.**

## Accomplishments

- Verified the shell component already stores active keyframe state alongside the existing style-transition state.
- Verified rebuild continuity, animation-name restart behavior, finite redraw shutdown, infinite redraw persistence, unresolved animation-name diagnostics, and invalid animation-token diagnostics through targeted shell tests.
- Confirmed `wants_render()` now accounts for active keyframes without getting stuck permanently true after finite completion.
- Re-ran the shell animation slices to validate the runtime contract end-to-end.

## Task Commits

Not created in this workspace run. The implementation was already present; this execution pass validated it against the plan’s runtime guarantees.

## Verification

- `nix develop -c cargo test -p mesh-core-shell keyframe_animation`
- `nix develop -c cargo test -p mesh-core-shell animation`

## Deviations From Plan

None. No shell-runtime changes were required during this execution pass because the implementation already matched the plan.

## Self-Check: PASSED

- Summary file exists.
- Targeted shell tests passed.
- Rebuild continuity, runtime diagnostics, and redraw-stop behavior are covered by automated tests.

---
*Phase: 12-theme-animation-tokens-and-css-animations*
*Completed: 2026-05-08*
