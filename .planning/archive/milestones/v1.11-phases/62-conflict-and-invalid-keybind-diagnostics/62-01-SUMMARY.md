---
phase: 62-conflict-and-invalid-keybind-diagnostics
plan: 01
subsystem: shell-input
tags: [keybinds, diagnostics, overrides, conflicts]

requires:
  - phase: 61-localized-resolution-and-override-safety
    provides: deterministic keybind resolution and override safety
provides:
  - Non-fatal keybind diagnostics through component diagnostics
  - Stable action-id ordering for declaration resolution and duplicate dispatch
  - Duplicate effective binding diagnostics
  - Unsafe override rejection for shell-owned traversal, activation, and copy chords
  - Missing subscriber, malformed trigger, unsupported modifier, and unresolved override diagnostics
affects: [surface-keybinds, shell-input, component-diagnostics, navigation-tests]

tech-stack:
  added: []
  patterns: [component diagnostics, stable keybind resolution order, unsafe override fallback]

key-files:
  created:
    - .planning/phases/62-conflict-and-invalid-keybind-diagnostics/62-01-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/component/diagnostics.rs
    - crates/core/shell/src/shell/component/input/keyboard.rs
    - crates/core/shell/src/shell/component/tests/interaction/navigation.rs

key-decisions:
  - "Keybind author/runtime issues use degraded component diagnostics because they are non-fatal."
  - "Duplicate bindings preserve deterministic first-match action-id order and emit diagnostics for later conflicting actions."
  - "Unsafe user overrides are ignored and resolution falls back to the safe module/locale default."

patterns-established:
  - "Keybind diagnostic messages include module id, surface id, action id, and reason."
  - "Runtime keybind tests assert diagnostics health alongside dispatch behavior."

requirements-completed: [KDIAG-01, KDIAG-02, KDIAG-03, KDIAG-04]

duration: 20min
completed: 2026-05-23
---

# Phase 62: Conflict And Invalid-Keybind Diagnostics Summary

**Surface keybinds now report invalid, conflicting, unresolved, and unsafe bindings through non-fatal component diagnostics.**

## Performance

- **Duration:** 20 min
- **Started:** 2026-05-23T12:37:00+02:00
- **Completed:** 2026-05-23T12:57:00+02:00
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Added `record_keybind_diagnostic` with module id, surface id, action id, and reason in the diagnostic message.
- Made declaration resolution order stable by action id before duplicate detection and dispatch matching.
- Diagnosed unresolved override ids, malformed/empty trigger keys, unsupported modifiers, duplicate effective bindings, and missing runtime subscribers.
- Ignored unsafe user overrides for shell-owned traversal/cancel/activation keys and reserved selection-copy shortcuts, falling back to safe defaults.

## Task Commits

1. **Tasks 1-3: Diagnostics, resolver hardening, and regression proof** - `7be5fbe` (fix)

**Plan metadata:** `e28884c` (docs: plan keybind diagnostics)

## Files Created/Modified

- `crates/core/shell/src/shell/component/diagnostics.rs` - Added component keybind diagnostic helper.
- `crates/core/shell/src/shell/component/input/keyboard.rs` - Added diagnostics, stable order, duplicate detection, and unsafe override fallback.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` - Added diagnostics regression coverage.

## Decisions Made

- Diagnostics are degraded health, not errors, because invalid keybinds should be observable without crashing a surface.
- Duplicate bindings keep deterministic first-match behavior; later conflicting actions get diagnostics.
- Unsafe overrides are ignored rather than allowed to steal shell-owned keyboard behavior.

## Deviations from Plan

None.

## Verification

- `nix develop -c cargo test -p mesh-core-shell keybind_diagnostic -- --nocapture`
- `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation -- --nocapture`

Result: focused diagnostics tests passed; full navigation interaction suite passed with 35 tests.

## User Setup Required

None.

## Next Phase Readiness

Phase 62 is ready for Phase 63 accessibility and debug/profiling metadata work.

---
*Phase: 62-conflict-and-invalid-keybind-diagnostics*
*Completed: 2026-05-23*
