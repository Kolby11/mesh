---
phase: 02-backend-lifecycle-foundation
plan: 03
subsystem: backend-runtime
tags: [rust, tokio, backend-runtime, lifecycle-cleanup]
requires:
  - phase: 02-backend-lifecycle-foundation
    provides: typed backend lifecycle events
provides:
  - shell-owned backend runtime slots
  - deterministic runtime replacement cleanup
  - lifecycle-driven command handler removal
affects: [backend lifecycle, shell runtime bridge, service commands]
tech-stack:
  added: []
  patterns: [one backend runtime slot per interface]
key-files:
  created: []
  modified:
    - crates/core/shell/src/shell/mod.rs
    - crates/core/shell/src/shell/types.rs
key-decisions:
  - "Shell runtime ownership is keyed by exact backend interface string."
  - "Terminal backend failures clean up the active runtime slot without starting a fallback provider."
patterns-established:
  - "Runtime slot replacement: remove service handler, abort old task, then insert the new handler and slot."
requirements-completed: [BPLUG-02, BPLUG-05]
duration: 16 min
completed: 2026-05-03
---

# Phase 02 Plan 03: Backend Runtime Slot Ownership Summary

**The shell now owns backend runtime slots and removes stale command handlers on stop, replacement, and terminal failure**

## Performance

- **Duration:** 16 min
- **Started:** 2026-05-03T17:44:05Z
- **Completed:** 2026-05-03T18:00:10Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- Added `backend_runtimes: HashMap<String, BackendRuntimeSlot>` to `Shell`, keyed by interface.
- Added `stop_backend_runtime` and `replace_backend_runtime` helpers that remove registered service command senders before replacing runtime slots.
- Added `ShellMessage::BackendLifecycle` so backend runtime lifecycle events can update shell-owned runtime state.
- Routed `InitFailed`, terminal `Failed`, and `Stopped` lifecycle events to runtime slot cleanup without automatic fallback startup.
- Added tests covering replacement, init failure cleanup, terminal failure behavior, and exact lifecycle status names.

## Task Commits

1. **Tasks 1-3: Runtime slot ownership, replacement cleanup, and lifecycle cleanup routing** - `8d8f813` (feat)

**Plan metadata:** this SUMMARY commit

## Files Created/Modified

- `crates/core/shell/src/shell/mod.rs` - Adds backend runtime slot ownership, stop/replace helpers, lifecycle event handling, and cleanup tests.
- `crates/core/shell/src/shell/types.rs` - Adds `ShellMessage::BackendLifecycle`.

## Decisions Made

- Used Tokio `AbortHandle` for runtime task cleanup so shell state can stop a backend without waiting on cooperative shutdown.
- Preserved external sender clone behavior in tests; the contract is that the shell unregisters stale handlers, not that every external clone is forcibly closed.
- Kept failure handling explicit and non-restarting to match the phase decision that fallback providers are not automatic recovery.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- The replacement test originally asserted an external sender clone was closed after replacement. That was corrected because receiver/sender clone lifetimes outside shell ownership are not the shell's cleanup contract.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Plan 04 can expose runtime lifecycle state through diagnostics and debug snapshots using the shell-owned status map.

## Self-Check: PASSED

- `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` passed.
- `nix develop -c cargo test -p mesh-core-backend spawn_backend_service` passed.

---
*Phase: 02-backend-lifecycle-foundation*
*Completed: 2026-05-03*
