---
phase: 73-shipped-manifest-i18n-proof
created: 2026-05-24
status: complete
---

# Phase 73 Research

## Files Reviewed

- `modules/frontend/navigation-bar/module.json`
- `modules/frontend/navigation-bar/config/i18n/en.json`
- `modules/frontend/navigation-bar/config/i18n/sk.json`
- `modules/frontend/audio-popover/module.json`
- `modules/frontend/audio-popover/config/i18n/en.json`
- `modules/frontend/audio-popover/config/i18n/sk.json`
- `docs/module-system.md`
- `crates/core/extension/module/src/package/tests.rs`
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`

## Findings

`@mesh/navigation-bar` is canonical `module.json` and already owns `config/i18n/en.json` and `config/i18n/sk.json`. Its keybind metadata still uses raw dotted key strings for `label`, `description`, and `category`, so the loader should currently warn that these are suspicious raw i18n keys.

`@mesh/audio-popover` is still a legacy manifest shape. Its keybind text is plain human-readable literal text, which remains allowed. Its template i18n catalogs exist and can support shipped proof that the volume surface remains localized, but this phase does not need to canonicalize the whole legacy manifest.

`docs/module-system.md` explains `mesh.i18n.supportedLocales` and `mesh.contributes.i18n` before the keybind section. The keybind section still describes labels generically and should show `{ "t": "...", "fallback": "..." }` alongside catalog declarations.

Existing test coverage already proves:

- Parser behavior for localized keybind display text.
- Loader diagnostics for raw dotted keybind labels.
- Installed graph preservation of localized keybind/layout metadata.
- Runtime resolution and missing-key diagnostics in synthetic shell tests.

Phase 73 should add shipped-fixture tests to prove the real navigation manifest and catalogs use the contract.

## Implementation Guidance

- In `modules/frontend/navigation-bar/module.json`, add `mesh.i18n` with `defaultLocale: "en"` and `supportedLocales: ["en", "sk"]`.
- Add `mesh.contributes.i18n` entries for `config/i18n/en.json` and `config/i18n/sk.json`.
- Change navigation keybind metadata:
  - `label`: `{ "t": "keybind.mute.label", "fallback": "Mute audio" }`
  - `description`: `{ "t": "keybind.mute.description", "fallback": "Toggle audio mute" }`
  - `category`: `{ "t": "keybind.category.audio", "fallback": "Audio" }`
- Add or update shipped tests to assert:
  - Navigation manifest keybind fields parse as `LocalizedText::Translation`.
  - Navigation loader emits no suspicious raw i18n key diagnostics.
  - Installed graph preserves navigation keybind source keys.
  - Real navigation runtime/debug metadata resolves keybind labels from bundled catalogs after locale switch.
  - Template/catalog coverage still includes shipped navigation translation keys.
- Update `docs/module-system.md` keybind documentation with explicit localized text object examples and a warning that raw strings are literals.
