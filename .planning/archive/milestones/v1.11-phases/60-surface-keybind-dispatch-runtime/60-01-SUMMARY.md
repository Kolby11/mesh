---
phase: 60-surface-keybind-dispatch-runtime
plan: 01
subsystem: shell-input
tags: [keybinds, input, navigation, manifest, luau]

requires:
  - phase: 32-keybind-manifest-schema
    provides: canonical manifest-owned keybind action declarations
  - phase: 33-localized-keybind-defaults
    provides: deterministic localized trigger resolution rules
provides:
  - Manifest-owned focused-surface keybind dispatch through runtime keybind subscribers
  - Focused text input protection from bare printable keybinds
  - No-subscriber shortcut matches that fall through to normal focused keyboard dispatch
  - Shipped navigation-bar manifest keybind regression proof
affects: [surface-keybinds, shell-input, navigation-bar, accessibility-shortcuts]

tech-stack:
  added: []
  patterns: [focused-surface shortcut dispatch, keybind subscriber no-op fallback, input-owned printable key guard]

key-files:
  created:
    - .planning/phases/60-surface-keybind-dispatch-runtime/60-01-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/component/input/mod.rs
    - crates/core/shell/src/shell/component/input/keyboard.rs
    - crates/core/shell/src/shell/component/tests/interaction/navigation.rs

key-decisions:
  - "Bare printable keys are input-owned when a text input is focused, even if the focused node also subscribes to a matching surface keybind."
  - "A resolved keybind with no runtime subscribers is treated as unhandled so normal focused keydown/default handling can continue."

patterns-established:
  - "Keybind dispatch returns Some(requests) only when runtime subscribers actually consume the shortcut."
  - "Text-input precedence is enforced before surface shortcut dispatch without changing Tab, Escape, or Ctrl+C ordering."

requirements-completed: [KDISP-01, KDISP-02, KDISP-03, KDISP-04]

duration: 25min
completed: 2026-05-23
---

# Phase 60: Surface Keybind Dispatch Runtime Summary

**Manifest-owned surface keybinds now dispatch through runtime subscribers while preserving focused text input and existing keyboard precedence.**

## Performance

- **Duration:** 25 min
- **Started:** 2026-05-23T11:18:00+02:00
- **Completed:** 2026-05-23T11:43:48+02:00
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Routed matched manifest actions through subscribed runtime `keybind` handlers without swallowing events when no subscriber exists.
- Preserved focused text input ownership for bare printable keys, so an `m` shortcut cannot steal normal typing from an input.
- Added focused regression tests for subscriber dispatch, no-subscriber fallthrough, text input protection, modifier gating, and the shipped navigation surface proof.

## Task Commits

1. **Tasks 1-3: Runtime dispatch, text input precedence, and shipped navigation proof** - `dda5b60` (fix)

**Plan metadata:** `2978b0d` (docs: create phase plan)

## Files Created/Modified

- `crates/core/shell/src/shell/component/input/mod.rs` - Checks focused text input ownership before surface shortcut dispatch and lets bare printable keys continue to focused keydown/text input handling.
- `crates/core/shell/src/shell/component/input/keyboard.rs` - Treats matched shortcuts with no runtime subscribers as unhandled instead of returning an empty consumed request list.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` - Adds dispatch/no-op/text-input regression coverage and keeps the real navigation-bar manifest keybind proof active.

## Decisions Made

- Bare printable text-input precedence is based on the focused runtime node, not on where the keybind subscriber is declared.
- Empty subscriber sets are a no-op, not consumption, because otherwise declared-but-unrendered actions block normal focused keyboard handling.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- Direct `cargo test` outside the dev shell failed because the host environment lacked `xkbcommon.pc`. Rerunning the same test command through `nix develop` provided the required native dependency and passed.

## Verification

- `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation -- --nocapture`

Result: 28 navigation interaction tests passed, including the new keybind dispatch/input protections and the real navigation-bar shortcut/theme activation proof.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 60 runtime dispatch is ready for Phase 61 localized resolution and override safety. The existing dirty audio popover files were intentionally left untouched for later shipped-surface/audio proof work.

---
*Phase: 60-surface-keybind-dispatch-runtime*
*Completed: 2026-05-23*
