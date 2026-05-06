---
phase: 11-keyboard-navigation-and-shortcuts
plan: 04
subsystem: proof-surface-and-docs
tags: [docs, navigation-bar, audio-popover, focus-visible, keyboard]

requires:
  - phase: 11-02
    provides: Focused control activation and keyboard handler routing
  - phase: 11-03
    provides: Keyboard settings and focused-surface shortcut resolution

provides:
  - Real navigation-bar keyboard proof covering traversal, activation, and mute shortcut routing
  - Real audio-popover slider proof for arrow-key stepping
  - Author docs for `tabindex`, focused keyboard handlers, and focused-surface shortcut scope
  - Guidance and shipped styling for modality-aware `:focus-visible`

affects:
  - phase-13-navigation-bar-proof
  - docs-frontend-mesh-syntax
  - docs-css-coverage

tech-stack:
  added: []
  patterns:
    - "Shipped proof surfaces rely on shell-owned keyboard defaults instead of ad hoc module logic"
    - "Docs describe `:focus-visible` as a heuristic visible-focus state, not a focused alias"

key-files:
  created:
    - .planning/phases/11-keyboard-navigation-and-shortcuts/11-04-SUMMARY.md
  modified:
    - modules/frontend/navigation-bar/src/main.mesh
    - modules/frontend/navigation-bar/src/components/settings-button.mesh
    - modules/frontend/navigation-bar/src/components/theme-button.mesh
    - modules/frontend/navigation-bar/src/components/volume-button.mesh
    - modules/frontend/audio-popover/src/main.mesh
    - docs/frontend/mesh-syntax.md
    - docs/css-coverage.md
    - docs/modules/frontend/core/navigation-bar/README.md
    - crates/core/shell/src/shell/component/tests.rs

key-decisions:
  - "The shipped navigation-bar and audio-popover modules are the milestone proof surfaces for keyboard-capable shell chrome."
  - "Proof-surface styles now use `:focus-visible` directly so author-facing guidance matches the runtime contract."
  - "Phase 11 closes with dedicated real-surface tests plus the broader `keyboard_` suite from the validation plan."

requirements-completed: [KEY-02, KEY-03, KEY-04]

duration: 1 session
completed: 2026-05-06
---

# Phase 11 Plan 04: Navigation-Bar Proof, Docs, And Regression Coverage

**The shipped navigation bar and audio popover now exercise the new keyboard contract directly, and the author docs describe the same `tabindex` and `:focus-visible` behavior the runtime implements.**

## Accomplishments

- Updated the real navigation-bar and audio-popover surfaces to show visible-focus styling on shipped controls and rely on shell-owned keyboard defaults.
- Added real-surface tests proving the navigation-bar mute shortcut and theme activation work from the keyboard and that the audio-popover slider responds to arrow-key stepping.
- Documented `tabindex`, focused `onkeydown` / `onkeyup`, focused-surface shortcut scope, and the modality-aware `:focus-visible` guidance for authors.
- Added regression guards so the broad `keyboard_` suite now covers pointer/keyboard coherence, real proof surfaces, and shell-global shortcut precedence.

## Task Commits

Not created in this workspace run. The implementation is validated but currently uncommitted.

## Verification

- `nix develop -c cargo test -p mesh-core-shell navigation_bar_keyboard`
- `nix develop -c cargo test -p mesh-core-shell keyboard_regression`
- `nix develop -c cargo test -p mesh-core-config keyboard_ && nix develop -c cargo test -p mesh-core-elements focus_visible && nix develop -c cargo test -p mesh-core-render accessibility_for_tag && nix develop -c cargo test -p mesh-core-shell keyboard_`

## Deviations From Plan

None. The proof-surface and doc updates stayed inside Phase 11 scope.

## Self-Check: PASSED

- Summary file exists.
- Targeted and full validation commands passed.
- Real proof surfaces, docs, and regression coverage align with the runtime contract.

---
*Phase: 11-keyboard-navigation-and-shortcuts*
*Completed: 2026-05-06*
