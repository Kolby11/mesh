---
phase: 33
plan: 01
title: Locale-aware access-key resolution
status: complete
completed_at: "2026-05-13T22:21:03+02:00"
requirements:
  - LOCL-01
  - LOCL-02
  - LOCL-03
---

# Plan 33-01 Summary: Locale-Aware Access-Key Resolution

## Outcome

Implemented locale-aware keybind resolution for frontend module keybind actions.

- Added typed per-action `localized_triggers` to normalized manifest keybind declarations.
- Kept localized trigger validation intentionally lightweight: locale ids must be nonblank, while blank or incomplete localized trigger bodies remain manifest-valid and fall back at runtime.
- Updated shell resolution to apply precedence: user override, exact locale, parent locale, generic module trigger, then no binding.
- Limited localization to `access_key` defaults; generic shortcut defaults ignore localized triggers unless a user override is present.
- Preserved existing legacy `settings.keyboard.shortcuts` compatibility and stable override identity by surface id plus action id.

## Commits

- `d319504 feat(33): add localized keybind trigger declarations`
- `55ec612 feat(33): resolve keybinds by locale precedence`

## Deviations

- No functional deviation from the plan.
- Added `#[cfg(test)]` re-exports for `KeybindResolutionSource` so component tests can assert the resolver source without widening production API use.
- Ran a small acceptance-name cleanup before closeout so test names match the plan acceptance criteria.

## Verification

All required Phase 33 verification passed:

- `nix develop -c cargo fmt --check`
- `nix develop -c cargo test -p mesh-core-module localized_keybind`
- `nix develop -c cargo test -p mesh-core-shell keybind_locale`
- `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts`
- `nix develop -c cargo test -p mesh-core-shell navigation_bar_keyboard_shortcut`

Known warnings are pre-existing dead-code warnings in test/support and render/presentation code.

