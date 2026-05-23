---
spike: 004
name: manifest-localized-text-contract
type: standard
validates: "Given module.json contains user-facing keybind, layout, and settings text, when authors declare localized labels and descriptions, then the manifest shape makes literal text, localized text, and translation asset ownership explicit."
verdict: VALIDATED
related: [003]
tags: [module-json, i18n, keybinds, manifest, localization]
---

# Spike 004: Manifest Localized Text Contract

## Question

`module.json` currently allows user-facing fields like keybind labels to look
like this:

```json
{
  "keybinds": {
    "mute": {
      "label": "keybind.mute.label",
      "description": "keybind.mute.description",
      "category": "keybind.category.audio",
      "trigger": {
        "kind": "shortcut",
        "key": "m"
      }
    }
  }
}
```

That shape is compact, but it is ambiguous. A plain JSON string might be literal
text, a translation key, a debug-only identifier, or an untranslated fallback.
The manifest does not show where the translation comes from, whether the field
will actually be localized, or what should happen when the key is missing.

## Verdict

Use a structured, function-like `LocalizedText` object in manifest fields that
are shown to users:

```json
{
  "keybinds": {
    "mute": {
      "label": { "t": "keybind.mute.label", "fallback": "Mute" },
      "description": {
        "t": "keybind.mute.description",
        "fallback": "Toggle mute for the active audio output."
      },
      "category": { "t": "keybind.category.audio", "fallback": "Audio" },
      "trigger": {
        "kind": "shortcut",
        "key": "m"
      }
    }
  }
}
```

Rules:

- A raw JSON string in a user-facing manifest text field means literal text.
- Localized text must use `{ "t": "...", "fallback": "..." }`.
- `t` resolves in the declaring module's i18n namespace by default.
- `fallback` is required for shell diagnostics, accessibility metadata, and
  early startup before catalogs are loaded.
- The manifest loader should warn when a raw string looks like a dotted
  translation key, such as `keybind.mute.label`.

This gives authors the function-like intent of `t("key")` without embedding a
mini expression language in JSON.

## Translation Source

The text reference is explicit at the use site, while the files remain explicit
in the module's i18n metadata:

```json
{
  "mesh": {
    "i18n": {
      "defaultLocale": "en",
      "supportedLocales": ["en", "sk"]
    },
    "contributes": {
      "i18n": [
        { "locale": "en", "path": "config/i18n/en.json" },
        { "locale": "sk", "path": "config/i18n/sk.json" }
      ]
    }
  }
}
```

`mesh.i18n.supportedLocales` declares what the module can support.
`mesh.contributes.i18n` declares the concrete bundled catalog files. The
`LocalizedText` field declares that this specific label or description expects
catalog lookup.

Future cross-module references can extend the same object without changing the
manifest field kind:

```json
{ "t": "@mesh/common:keybind.category.audio", "fallback": "Audio" }
```

## Alternatives Considered

| Shape | Example | Result |
| --- | --- | --- |
| Plain key string | `"label": "keybind.mute.label"` | Rejected. Compact, but indistinguishable from literal text and does not prove localization is wired. |
| Function string | `"label": "t('keybind.mute.label')"` | Rejected. Familiar from `.mesh`, but requires string parsing and escaping rules inside JSON. |
| Tagged string | `"label": "@i18n:keybind.mute.label"` | Rejected for primary schema. Better than plain keys, but still stringly and has no room for required fallback text. |
| Structured object | `"label": { "t": "keybind.mute.label", "fallback": "Mute" }` | Accepted. Explicit, parseable with serde, extensible, and diagnosable. |
| Literal object | `"label": { "text": "Mute" }` | Useful as an optional canonical form, but raw strings can remain the shorthand for literals. |

## Implementation Impact

Add a reusable manifest type:

```rust
pub enum LocalizedText {
    Literal(String),
    Translation {
        key: String,
        fallback: String,
        namespace: Option<String>,
    },
}
```

Deserialize user-facing manifest fields through `LocalizedText`:

- `mesh.keybinds.*.label`
- `mesh.keybinds.*.description`
- `mesh.keybinds.*.category`
- `mesh.contributes.layout[].label`
- settings schema `label`, `description`, and enum value display text when
  those fields become first-class
- provider and interface labels if they are ever rendered to users

Backwards compatibility:

- Existing strings continue to load as `LocalizedText::Literal`.
- Strings matching likely translation key syntax should produce a warning with
  a concrete fix:

```text
mesh.keybinds.mute.label looks like an i18n key. Use
{ "t": "keybind.mute.label", "fallback": "Mute" } to localize this field.
```

Resolution:

- The installed graph should preserve `LocalizedText`, not flatten it to
  `String`.
- Shell/component runtime resolves `Translation` against the active locale,
  then fallback locale, then `fallback`.
- Debug payloads should expose both the resolved string and the source key so
  missing catalogs are easy to trace.

## Repo Evidence

- `modules/frontend/navigation-bar/module.json` currently uses
  `keybind.mute.label`, `keybind.mute.description`, and
  `keybind.category.audio` as plain strings.
- `docs/module-system.md` documents `mesh.i18n.supportedLocales` and
  `mesh.contributes.i18n`, but keybind text fields are still described only as
  `label`, `description`, and `category`.
- `docs/frontend/mesh-syntax.md` already teaches `t("greeting", ...)` inside
  `.mesh` files, so authors will reasonably expect manifest text to expose a
  comparable localization affordance.
- `crates/core/extension/module/src/manifest/model.rs` currently stores
  keybind `label` and `description` as `Option<String>`, which forces the
  installed graph to lose the difference between literal and localized text.

## Follow-Up Work

- Add `LocalizedText` to the module manifest model and JSON deserializer.
- Preserve localized text objects through installed-graph contribution records.
- Resolve localized keybind metadata in the shell before accessibility and
  debug payload publication.
- Update author docs and migrate `@mesh/navigation-bar` keybind metadata.
- Add diagnostics for suspicious dotted-key raw strings in localized-capable
  fields.
