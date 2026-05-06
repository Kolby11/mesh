---
phase: 11-keyboard-navigation-and-shortcuts
plan: 01
subsystem: ui-runtime
tags: [rust, shell, focus, focus-visible, tabindex, keyboard]

requires:
  - phase: 09-01
    provides: Stable shell-owned runtime annotation through `_mesh_key`
  - phase: 10-01
    provides: Selection ownership rules that keyboard traversal must preserve

provides:
  - Shell-owned `focus_visible` state separate from logical focus
  - Deterministic visual-order Tab traversal with wraparound and `tabindex` overrides
  - Pointer/keyboard focus coherence for text inputs and non-text controls
  - Accessibility focusability alignment for checkbox and switch

affects:
  - phase-11-keyboard-activation
  - phase-11-surface-shortcuts
  - phase-13-navigation-bar-proof

tech-stack:
  added: []
  patterns:
    - "Focus-visible is derived from shell modality state, not aliased from focused"
    - "Tab traversal is collected from the final laid-out tree, not template order"

key-files:
  created:
    - .planning/phases/11-keyboard-navigation-and-shortcuts/11-01-SUMMARY.md
  modified:
    - crates/core/ui/elements/src/tree.rs
    - crates/core/ui/elements/src/style.rs
    - crates/core/ui/render/src/render.rs
    - crates/core/shell/src/shell/component/runtime_tree.rs
    - crates/core/shell/src/shell/layout.rs
    - crates/core/shell/src/shell/component/input.rs
    - crates/core/shell/src/shell/component/tests.rs

key-decisions:
  - "Logical focus and visible focus now live as separate shell-owned states so `:focus-visible` can follow modality rules instead of mirroring `:focus`."
  - "Tab traversal is derived from the post-layout tree, sorted by visual geometry, and only uses `tabindex` as an override path."
  - "Pointer-focused text inputs keep visible focus while pointer-focused non-text controls clear the stronger keyboard-style focus hint."

requirements-completed: [KEY-01, KEY-02]

duration: 1 session
completed: 2026-05-06
---

# Phase 11 Plan 01: Traversal Order And Focus-Visible Foundations

**Phase 11 now has a shell-owned focus model with real `:focus-visible` state, visual-order Tab traversal, and deterministic `tabindex` behavior.**

## Accomplishments

- Added `focus_visible` to `ElementState`, updated selector matching, and stopped aliasing `:focus-visible` to plain logical focus.
- Rehydrated visible-focus state through the runtime tree so rebuilt nodes preserve keyboard modality and pointer-text-entry rules.
- Added visual-order traversal collection, wraparound Tab behavior, `tabindex` / `tabindex=-1` handling, and blur/focus dispatch during keyboard traversal.
- Aligned accessibility focusability so `switch` and `checkbox` are advertised as focusable alongside the controls the shell already treats as keyboard targets.

## Task Commits

Not created in this workspace run. The implementation is validated but currently uncommitted.

## Verification

- `nix develop -c cargo test -p mesh-core-elements focus_visible`
- `nix develop -c cargo test -p mesh-core-render accessibility_for_tag`
- `nix develop -c cargo test -p mesh-core-shell keyboard_navigation`

## Deviations From Plan

None. The implementation followed the planned shell-owned focus and traversal shape.

## Self-Check: PASSED

- Summary file exists.
- Targeted validation commands passed.
- `:focus-visible`, traversal order, and accessibility metadata are covered by automated tests.

---
*Phase: 11-keyboard-navigation-and-shortcuts*
*Completed: 2026-05-06*
