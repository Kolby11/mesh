---
phase: 31-smoothness-proof-and-cpu-render-tuning
plan: 03
subsystem: ui
tags: [shell, audio, popover, focus, service-state]
requires:
  - phase: 31-02
    provides: Initial audio popover gap-closure implementation and regression coverage
provides:
  - Pointer-open audio popover activation without focus theft
  - Idempotent mute requests using set_muted when supported
  - Shell optimistic mute-state reconciliation across stale backend updates
affects: [navigation-bar, audio-popover, shell-service-state, live-uat]
tech-stack:
  added: []
  patterns:
    - Pointer activation registers popovers without forced focus transfer
    - Shell holds pending service state for idempotent audio mute requests
key-files:
  created:
    - .planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-03-SUMMARY.md
  modified:
    - modules/frontend/navigation-bar/src/main.mesh
    - modules/frontend/audio-popover/src/main.mesh
    - crates/core/shell/src/shell/runtime/service_state.rs
    - crates/core/shell/src/shell/runtime/request.rs
    - crates/core/shell/src/shell/component/tests/interaction/navigation.rs
    - crates/core/shell/src/shell/component/tests/interaction/policy.rs
    - crates/core/shell/src/shell/tests.rs
key-decisions:
  - "Pointer-open popovers should not steal keyboard focus; keyboard-open popovers still focus the first popover control."
  - "Mute UI should use idempotent set_muted when supported instead of chaining non-idempotent toggle_mute calls."
patterns-established:
  - "Optimistic service-state reconciliation: shell broadcasts requested mute state and suppresses stale backend flips until confirmation."
requirements-completed: ["PERF-03", "SMTH-01", "SMTH-02"]
duration: 16min
completed: 2026-05-13
---

# Phase 31 Plan 03: Close Remaining Live Audio Interaction Gaps Summary

**Audio popover first-input fixes with idempotent mute reconciliation across navigation and popover UI**

## Performance

- **Duration:** 16 min
- **Started:** 2026-05-13T17:32:00Z
- **Completed:** 2026-05-13T17:48:18Z
- **Tasks:** 4
- **Files modified:** 14

## Accomplishments

- Changed pointer-triggered audio popover opens to register the popover without stealing focus, while preserving keyboard activation focus transfer.
- Replaced frontend mute actions with idempotent `set_muted("default", desired)` when the audio interface supports it, retaining `toggle_mute` fallback.
- Added shell-level optimistic mute state so stale backend `muted` events cannot flip navigation/popover UI while a requested mute state is pending.
- Reset Phase 31 UAT tests 2, 3, and 5 for live retest with updated fix evidence.

## Task Commits

1. **Tasks 31-03-01 through 31-03-03: audio interaction fixes and regressions** - `bfc6cd4` (fix)
2. **Task 31-03-04: UAT and verification artifacts** - docs commit following implementation

## Files Created/Modified

- `modules/frontend/navigation-bar/src/main.mesh` - Pointer opens no longer force popover focus; mute shortcut uses `set_muted` when available.
- `modules/frontend/audio-popover/src/main.mesh` - Mute button sends requested muted state via `set_muted` when available.
- `crates/core/shell/src/shell/runtime/service_state.rs` - Adds optimistic pending mute-state normalization for audio service updates.
- `crates/core/shell/src/shell/runtime/request.rs` - Applies optimistic mute state after successful `set_muted` service command dispatch.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` - Covers pointer-open no-focus behavior and updated idempotent mute command.
- `crates/core/shell/src/shell/component/tests/interaction/policy.rs` - Covers true -> false mute request sequence and stale backend protection in the popover.
- `crates/core/shell/src/shell/tests.rs` - Covers shell optimistic mute broadcast and stale backend suppression.
- `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-UAT.md` - Resets tests 2, 3, and 5 for live retest.
- `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-VERIFICATION.md` - Records 31-03 verification status and commands.

## Decisions Made

- Pointer activation should leave focus ownership alone until the user interacts with the popover; this avoids the first click after opening being consumed by focus transfer.
- Keyboard activation should still move focus into the popover for accessibility and existing keyboard traversal behavior.
- `set_muted` is preferred over `toggle_mute` because it is idempotent and cannot double-flip when stale backend events arrive out of order.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- Existing navigation shortcut tests expected `toggle_mute`; updated them to assert the safer `set_muted` request and payload.
- Parallel cargo test invocations briefly contended on Cargo/Nix locks; reran the filters sequentially.

## Verification

- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell audio_popover`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell navigation_bar`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell real_surfaces`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell activate_popover`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell set_muted_command_broadcasts_optimistic_audio_state_until_backend_confirms`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling`

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 31 is ready for live UAT retest of tests 2, 3, and 5. Do not mark final acceptance until the running shell confirms the audio popover first-click, first-drag, and second mute-toggle behavior.

## Self-Check: PASSED

---
*Phase: 31-smoothness-proof-and-cpu-render-tuning*
*Completed: 2026-05-13*
