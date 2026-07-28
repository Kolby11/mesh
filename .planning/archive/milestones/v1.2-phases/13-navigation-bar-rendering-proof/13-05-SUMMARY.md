---
phase: 13-navigation-bar-rendering-proof
plan: 05
subsystem: real-surface-proof
tags: [tests, docs, navigation-bar]

requires:
  - phase: 13-01
    provides: Final shipped nav-bar layout and selection boundary
  - phase: 13-02
    provides: Mounted control cluster and component inventory
  - phase: 13-03
    provides: Motion proof and focus contract
  - phase: 13-04
    provides: Explicit compact-state behavior

provides:
  - Real-surface shell tests for selectable status copy, control behavior, constrained width, and nav-bar keyframe metadata
  - Author docs aligned with the shipped passive-selection proof

affects:
  - phase-13-verification

tech-stack:
  added: []
  patterns:
    - "Real shipped shell modules should be tested directly rather than replaced by synthetic fixtures"
    - "Rendered-node animation metadata can be asserted across rebuilds when the generic engine tests already cover playback internals"

key-files:
  created:
    - .planning/phases/13-navigation-bar-rendering-proof/13-05-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/component/tests.rs
    - docs/frontend/mesh-syntax.md

key-decisions:
  - "The constrained-width proof uses the real navigation-bar module at both wide and compact widths."
  - "The nav-bar keyframe regression asserts persistent rendered-node animation metadata across rebuilds while the generic keyframe suite continues to verify the engine internals."
  - "Author docs now explicitly position `selectable=\"true\"` as passive shell-status copy behavior."

requirements-completed: [NAV-02, NAV-03, NAV-04, NAV-05]

duration: 1 session
completed: 2026-05-08
---

# Phase 13 Plan 05: Real-Surface Shell Proof and Responsive Test Coverage

**Phase 13 now has real-surface proof coverage for the richer navigation bar, including selection, keyboard control behavior, compact-state behavior, and nav-bar animation metadata.**

## Accomplishments

- Extended `crates/core/shell/src/shell/component/tests.rs` with real-surface navigation-bar checks for passive selectable status text, compact-width status collapse, and persisted nav-bar animation metadata.
- Updated the existing navigation-bar control tests to match the new shipped layout rather than the earlier focus-diagnostic assumptions.
- Narrowly updated `docs/frontend/mesh-syntax.md` so `selectable="true"` guidance matches the shipped shell-status proof surface.

## Task Commits

Not created in this workspace run. The implementation is validated but currently uncommitted.

## Verification

- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell navigation_bar`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell keyframe_animation`
- `grep -n 'selectable="true"' docs/frontend/mesh-syntax.md`

## Deviations From Plan

The nav-bar keyframe regression ended up asserting persisted rendered-node animation metadata rather than reading the internal `keyframe_animations` map directly, because the generic keyframe suite already covers that lower-level runtime bookkeeping and the real shipped-module path exposes the rendered metadata more reliably.

## Self-Check: PASSED

- Summary file exists.
- Real-surface shell tests for the richer navigation bar passed.
- Docs now match the shipped passive-selection proof model.

---
*Phase: 13-navigation-bar-rendering-proof*
*Completed: 2026-05-08*
