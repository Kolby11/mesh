---
phase: 12-theme-animation-tokens-and-css-animations
plan: 01
subsystem: theme-token-contract
tags: [theme, animation, diagnostics, docs]

requires:
  - phase: 08-01
    provides: Practical CSS token and transition parsing baseline

provides:
  - Canonical `animation.*` theme token namespace with no `motion.*` carryover
  - Default animation recipe tokens separated from primitive duration and curve tokens
  - Strict animation-token diagnostic coverage in style resolution

affects:
  - phase-12-keyframe-parsing
  - phase-12-docs-and-proof
  - phase-13-navigation-bar-proof

tech-stack:
  added: []
  patterns:
    - "Primitive animation tokens and default animation recipes stay separate in theme JSON"
    - "Invalid `token(animation.*)` references fail visibly instead of falling back silently"

key-files:
  created:
    - .planning/phases/12-theme-animation-tokens-and-css-animations/12-01-SUMMARY.md
  modified:
    - config/themes/mesh-default-dark.json
    - config/themes/mesh-default-light.json
    - docs/theming/themes.md
    - crates/core/ui/elements/src/style/resolve.rs
    - crates/core/ui/elements/src/style/types.rs
    - crates/core/ui/elements/src/style.rs

key-decisions:
  - "The theme contract stays on `animation.*`; no long-term `motion.*` alias is preserved."
  - "Theme files now show recipe-style defaults such as `animation.default.border-radius` with explicit `token(...)` references."
  - "Style resolution keeps animation-token failures diagnostic-only and skips the invalid declaration rather than guessing a fallback."

requirements-completed: [ANIM-01, ANIM-02, ANIM-05]

duration: 1 session
completed: 2026-05-08
---

# Phase 12 Plan 01: Animation Token Namespace And Strict Token Resolution

**Phase 12 now exposes a clean `animation.*` theme token contract with explicit default recipes and strict invalid-token diagnostics.**

## Accomplishments

- Confirmed the shipped style resolver already enforces hard-fail diagnostics for invalid `token(animation.*)` references in animation declarations.
- Added explicit default animation recipe tokens to both default themes so primitive durations/curves and full property recipes are no longer conflated.
- Updated theme author docs to describe the primitive-versus-recipe split and to show `animation.default.border-radius` using explicit `token(...)` references.
- Revalidated the targeted animation-token tests after the theme/doc contract changes.

## Task Commits

Not created in this workspace run. The implementation is validated but currently uncommitted.

## Verification

- `nix develop -c cargo test -p mesh-core-elements animation_token`
- `grep -R '"motion\.' config/themes/mesh-default-dark.json config/themes/mesh-default-light.json`
- `grep -R "animation.default.border-radius\|token(animation.duration.fast)\|animation.curves.bezier.standard" config/themes docs/theming/themes.md`

## Deviations From Plan

None. The resolver-side strictness was already present; this execution pass filled the remaining theme/doc contract gap and revalidated the behavior.

## Self-Check: PASSED

- Summary file exists.
- Targeted tests passed.
- Theme files and docs now expose the canonical `animation.*` contract and default recipe examples.

---
*Phase: 12-theme-animation-tokens-and-css-animations*
*Completed: 2026-05-08*
