---
phase: 61-localized-resolution-and-override-safety
status: planned
created: 2026-05-23
---

# Phase 61 Validation Strategy

## Validation Architecture

Phase 61 validates deterministic resolver semantics through focused Rust unit/integration tests in the existing shell component navigation suite.

## Dimensions

| Dimension | Evidence |
|-----------|----------|
| Requirement coverage | PLAN frontmatter includes KRES-01, KRES-02, KRES-03, KRES-04. |
| Resolver behavior | Tests assert exact locale, parent locale, generic fallback, blank localized fallback, and user override ordering. |
| Override safety | Tests assert unknown override action ids do not create resolved shortcuts. |
| Legacy fallback | Existing manifest-over-legacy and legacy settings shortcut tests remain passing. |
| Regression safety | Full `shell::component::tests::interaction::navigation` suite passes. |

## Required Commands

- `nix develop -c cargo test -p mesh-core-shell keybind_locale`
- `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts_manifest_keybind_subscriber_resolves_user_override_by_id`
- `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts_manifest_declaration_wins_over_legacy_settings_same_id`
- `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation`

## Human Verification

None required.
