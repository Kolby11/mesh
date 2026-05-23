---
phase: 61
slug: localized-resolution-and-override-safety
status: complete
researched: 2026-05-23
---

# Phase 61 Research: Localized Resolution And Override Safety

## Question

What needs to be known to plan Phase 61 well?

## Current Runtime Shape

Effective focused-surface keybind resolution is concentrated in `crates/core/shell/src/shell/component/input/keyboard.rs`.

- `resolved_surface_shortcuts` collects declarations, looks up `keyboard.surface_shortcuts` overrides by surface id and action id, then calls `resolve_surface_shortcut_declaration`.
- `surface_shortcut_declarations` already makes manifest actions primary and appends legacy settings declarations only when an action id is missing from the manifest set.
- `resolve_surface_shortcut_declaration` already applies user override first, locale candidates second, and generic trigger last.
- `keybind_locale_candidates` already normalizes `_` to `-` and returns exact locale before parent locale.
- Existing tests in `navigation.rs` already prove exact locale, parent locale, user override, generic fallback, blank localized fallback, manifest-over-legacy precedence, modifier matching, and real navigation dispatch.

## Known Gaps

Two requirements need explicit Phase 61 hardening:

1. **Overrides cannot create declarations.** The current code naturally ignores unknown overrides because it starts from declarations, but there should be a regression test proving an override for an undeclared action id does not resolve or dispatch.
2. **Localized defaults are access-key scoped.** Current tests include `keybind_locale_shortcut_uses_localized_trigger`, which allows a localized trigger to override a shortcut action. KRES-03 requires the opposite: shortcut actions keep generic defaults unless a user override exists.

## Planning Implications

The implementation should be small and test-led:

1. Add/update focused tests in `navigation.rs` for:
   - unknown override action id does not create a resolved shortcut;
   - exact locale beats parent locale for access-key actions;
   - parent locale beats generic for access-key actions;
   - blank localized trigger falls through;
   - shortcut localized triggers do not override generic shortcut defaults;
   - user override still wins for shortcut and access-key actions.
2. Adjust `resolve_surface_shortcut_declaration` so localized trigger candidates are only considered when the generic trigger kind is `AccessKey`.
3. Preserve event/source metadata, modifier matching, manifest-over-legacy precedence, and Phase 60 dispatch behavior.

## Files To Plan Around

| Area | File | Notes |
|------|------|-------|
| Effective resolver | `crates/core/shell/src/shell/component/input/keyboard.rs` | Main implementation target. |
| Resolution tests | `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` | Existing focused keybind test suite. |
| Settings schema | `crates/core/foundation/config/src/lib.rs` | User override data shape. |
| Manifest model | `crates/core/extension/module/src/manifest/model.rs` | Trigger kinds and localized trigger model. |

## Validation Architecture

Phase 61 can be validated with focused Rust tests:

- `nix develop -c cargo test -p mesh-core-shell keybind_locale`
- `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts_manifest_keybind_subscriber_resolves_user_override_by_id`
- `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts_manifest_declaration_wins_over_legacy_settings_same_id`
- `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation`

No manual-only verification is required for Phase 61.

## Planning Recommendation

Use one plan with three tasks:

1. Add/adjust resolution tests to encode KRES-01 through KRES-04.
2. Tighten resolver behavior in `input/keyboard.rs`.
3. Run focused navigation/keybind suites and verify Phase 60 dispatch tests still pass.

Avoid moving Phase 62 diagnostics, Phase 63 metadata, Phase 64 shipped audio proof, settings UI, or compositor-global shortcut work into Phase 61.
