---
phase: 61-localized-resolution-and-override-safety
plan: 01
subsystem: shell-input
tags: [keybinds, locale, overrides, settings, manifest]

requires:
  - phase: 60-surface-keybind-dispatch-runtime
    provides: focused-surface keybind dispatch through runtime subscribers
provides:
  - Deterministic keybind resolution precedence across overrides, locales, generic triggers, and no binding
  - Override safety proof that settings cannot create undeclared actions
  - Access-key-only localized default semantics for manifest actions
  - Legacy settings fallback regression coverage
affects: [surface-keybinds, shell-input, settings-overrides, navigation-tests]

tech-stack:
  added: []
  patterns: [manifest-first resolution, access-key-localized defaults, override-does-not-create-declaration]

key-files:
  created:
    - .planning/phases/61-localized-resolution-and-override-safety/61-01-SUMMARY.md
  modified:
    - crates/core/shell/src/shell/component/input/keyboard.rs
    - crates/core/shell/src/shell/component/tests/interaction/navigation.rs

key-decisions:
  - "Localized keybind defaults apply only when the declared generic trigger kind is access_key."
  - "User overrides are applied only to existing declarations and cannot create missing action ids."

patterns-established:
  - "Shortcut actions ignore localized trigger defaults and keep the generic shortcut unless user override data exists."
  - "Resolver tests assert source metadata along with resolved key and trigger kind."

requirements-completed: [KRES-01, KRES-02, KRES-03, KRES-04]

duration: 18min
completed: 2026-05-23
---

# Phase 61: Localized Resolution And Override Safety Summary

**Surface keybind resolution now has locked precedence, action-id override safety, and access-key-only localized defaults.**

## Performance

- **Duration:** 18 min
- **Started:** 2026-05-23T12:16:00+02:00
- **Completed:** 2026-05-23T12:34:00+02:00
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- Restricted localized defaults to `access_key` actions so shortcut actions keep generic defaults unless user override data exists.
- Added a regression proving `keyboard.surface_shortcuts` overrides cannot create missing manifest/settings declarations.
- Preserved exact locale, parent locale, blank localized fallback, user override, manifest-over-legacy, and Phase 60 dispatch/input regressions.

## Task Commits

1. **Tasks 1-3: Resolver precedence, override safety, and regression proof** - `0e352b1` (fix)

**Plan metadata:** `0f1b12e` (docs: create phase plan)

## Files Created/Modified

- `crates/core/shell/src/shell/component/input/keyboard.rs` - Localized trigger candidates now apply only to access-key declarations.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` - Added unknown override safety coverage and changed shortcut-localized behavior to the KRES-03 contract.

## Decisions Made

- Localized trigger lookup is gated by the generic trigger kind being `KeybindTriggerKind::AccessKey`.
- Unknown override action ids are ignored because resolution starts from declared actions, not settings keys.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- Three focused tests were accidentally launched in parallel through `nix develop`, causing temporary Cargo/Nix lock waiting. All completed successfully; subsequent full-suite verification was run normally.

## Verification

- `nix develop -c cargo test -p mesh-core-shell keybind_locale -- --nocapture`
- `nix develop -c cargo test -p mesh-core-shell keybind_override_cannot_create_missing_manifest_declaration -- --nocapture`
- `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts_manifest_keybind_subscriber_resolves_user_override_by_id -- --nocapture`
- `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts_manifest_declaration_wins_over_legacy_settings_same_id -- --nocapture`
- `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation -- --nocapture`

Result: focused tests passed; full navigation interaction suite passed with 29 tests.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 61 is ready for Phase 62 conflict and invalid-keybind diagnostics. The resolver now has stable behavior for diagnostics to inspect without making settings canonical.

---
*Phase: 61-localized-resolution-and-override-safety*
*Completed: 2026-05-23*
