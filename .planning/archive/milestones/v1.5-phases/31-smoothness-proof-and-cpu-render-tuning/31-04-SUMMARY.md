---
phase: 31-smoothness-proof-and-cpu-render-tuning
plan: 04
subsystem: ui
tags: [shell, audio, popover, mute, live-uat]
requires:
  - phase: 31-03
    provides: Pointer-open no-focus behavior and shell optimistic mute normalization
provides:
  - Same-hover audio trigger close without pointer leave/re-enter
  - Audio popover mute display driven by shell-normalized service state
  - Focused live-UAT reset for tests 2 and 5
affects: [navigation-bar, audio-popover, shell-service-state, live-uat]
tech-stack:
  added: []
  patterns:
    - Popover trigger close publishes an explicit shell hide request
    - Audio mute UI renders from shell-normalized service state
key-files:
  created:
    - .planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-04-SUMMARY.md
  modified:
    - modules/frontend/navigation-bar/src/main.mesh
    - modules/frontend/audio-popover/src/main.mesh
    - crates/core/shell/src/shell/component/tests/interaction/navigation.rs
    - crates/core/shell/src/shell/component/tests/interaction/policy.rs
    - .planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-04-PLAN.md
    - .planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-UAT.md
    - .planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-VERIFICATION.md
requirements-completed: ["PERF-03", "SMTH-01", "SMTH-02"]
duration: 15min
completed: 2026-05-13
---

# Phase 31 Plan 04: Close Retest Audio Trigger and Mute Consistency Gaps Summary

## Accomplishments

- Added an immediate `mesh.popover.hide(audio_surface_id)` request on the navigation volume trigger close path.
- Added a same-hover regression proving the second click on the still-hovered volume trigger emits `HideSurface` without pointer leave/re-enter.
- Removed the audio popover's private pending mute state so popover text follows shell-normalized `mesh.audio.muted`.
- Updated popover mute coverage to expect shell-normalized optimistic service state rather than local pending state.
- Reset UAT tests 2 and 5 for focused live retest; test 3 remains passed from the latest live retest.

## Task Commits

1. **Tasks 31-04-01 through 31-04-03: trigger close and mute consistency fixes** - `9497b8d` (fix)
2. **Task 31-04-04: UAT and verification reset** - docs commit following implementation

## Verification

- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell navigation_bar_same_hover_volume_trigger_closes_popover_immediately`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell audio_popover_mute_renders_shell_normalized_state`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell set_muted_command_broadcasts_optimistic_audio_state_until_backend_confirms`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell audio_popover`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell navigation_bar`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell real_surfaces`

## Deviations from Plan

None - plan executed exactly as written.

## Next Phase Readiness

Phase 31 still requires live UAT confirmation for tests 2 and 5 before final acceptance.

## Self-Check: PASSED
