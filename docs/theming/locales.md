# Localization

MESH localization is **module-authored and user-extensible**. Every module
ships translations for its own strings; third-party **language packs** layer
additional or replacement translations on top; the user picks a locale and a
fallback chain.

Like the rest of the shell, localization uses a contract + registry, not a
hardcoded code path.

## Model

1. **Language packs layer on top.** A language pack is a module that
   provides translations *for other modules*. Multiple packs can be active
   at once.
2. **The user picks a locale + fallback chain.** Missing keys walk the
   chain.
3. **Lookups go through `mesh.locale`.** Surfaces, widgets, and services
   resolve keys through the interface registry — so localization is
   extensible the same way services and icons are.

## The `mesh.locale` contract

```
interface: mesh.locale
version:   1.0
methods:
  current() -> string                           # e.g. "sk-SK"
  chain() -> [string]                           # resolution order
  translate(module_id: string, key: string, args: map?) -> string
  format_number(n: number, options?: map) -> string
  format_date(d: datetime, pattern: string) -> string
  format_duration(seconds: number) -> string
  pluralize(count: number, forms: map<string,string>) -> string
  has(module_id: string, key: string) -> bool
events:
  LocaleChanged(locale: string, chain: [string])
  TranslationsReloaded(module_id: string)
```

Translations are **scoped by module ID**. A key `results.files` in
`@mesh/launcher` never collides with `results.files` in
`@community/file-manager`. This is what makes third-party language packs
safe: they target modules individually.

## Module-bundled translations

The simplest case: a module declares the locales it supports in
`module.json` and ships its own locale files next to the manifest.

```
@mesh/launcher/
  module.json
  config/
    i18n/
      en.json
      sk.json
```

```json
{
  "name": "@mesh/launcher",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "i18n": {
      "defaultLocale": "en",
      "supportedLocales": ["en", "sk"]
    },
    "contributes": {
      "i18n": [
        { "id": "en", "locale": "en", "path": "config/i18n/en.json" },
        { "id": "sk", "locale": "sk", "path": "config/i18n/sk.json" }
      ]
    }
  }
}
```

```json
// config/i18n/sk.json
{
  "results.files":   "Súbory",
  "results.browser": "Prehliadač",
  "search.placeholder": "Hľadať aplikácie"
}
```

At load, the core indexes these under `@mesh/launcher`.

## Language packs

A language pack is a module that provides translations for other modules.
Declare it with `mesh.kind = "language-pack"` in `module.json`.

```
@community/cs-language-pack/
  module.json
  translations/
    @mesh/launcher/cs.json
    @mesh/panel/cs.json
    @mesh/quick-settings/cs.json
```

```json
{
  "name": "@community/cs-language-pack",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "language-pack",
    "contributes": {
      "i18n": [
        {
          "id": "launcher-cs",
          "locale": "cs",
          "path": "translations/@mesh/launcher/cs.json"
        },
        {
          "id": "panel-cs",
          "locale": "cs",
          "path": "translations/@mesh/panel/cs.json"
        },
        {
          "id": "quick-settings-cs",
          "locale": "cs",
          "path": "translations/@mesh/quick-settings/cs.json"
        }
      ]
    }
  }
}
```

Packs implement `mesh.locale.source` — a narrower contract than
`mesh.locale` itself. Multiple sources coexist as an ordered chain (same
deliberate divergence as icon packs). The active-single `mesh.locale`
implementation aggregates all sources into one lookup path.

## Translation file format

Flat JSON, key → string. Keys may be dotted for organizational purposes;
the lookup treats them as opaque.

```json
{
  "search.placeholder": "Hľadať aplikácie",
  "results.files":      "Súbory",

  "battery.charging":   "Nabíja sa, {time_to_full}",
  "battery.discharging": "{level}% – zostáva {time_to_empty}",

  "notifications.count": {
    "_plural": true,
    "zero":  "Žiadne upozornenia",
    "one":   "{count} upozornenie",
    "few":   "{count} upozornenia",
    "many":  "{count} upozornení",
    "other": "{count} upozornení"
  }
}
```

### Interpolation

`{name}` is replaced with `args.name`. The core does not evaluate
expressions in translation strings — values come from the arg map only.

### Plurals

A value that is an object with `_plural: true` is a plural form table. The
keys follow CLDR categories (`zero`, `one`, `two`, `few`, `many`, `other`);
`other` is required as fallback. `mesh.locale:pluralize(count, forms)`
selects the right form for the active locale.

## Lookup chain

For a given `(module_id, key)`:

1. **User-pinned override** (per-module settings, rare — mainly for
   corrections without repackaging)
2. **Highest-priority language pack** providing `(module_id, active_locale)`
3. **Next language pack** providing the same
4. **Module-bundled** `active_locale`
5. For each locale in the user's fallback chain, repeat 2–4
6. **Module-bundled** `default_locale`
7. Return the raw key prefixed with `!!` as a visible diagnostic
   (`!!results.files`) so missing translations surface in the UI

## User settings

```json
{
  "i18n": {
    "locale":           "sk-SK",
    "fallback_locale":  "en",
    "chain":            ["sk-SK", "sk", "cs", "en"]
  }
}
```

- `locale` — primary locale the user wants to see
- `fallback_locale` — terminal fallback for any missing key
- `chain` — explicit resolution order; when omitted, the core derives one
  from `locale` (drop region → drop script → fallback → `en`)

Changing any of these emits `LocaleChanged` and every subscriber refreshes.

## Locale codes

BCP 47 tags: `en`, `en-US`, `sk-SK`, `zh-Hant-TW`. Matching is
most-specific-first: `sk-SK` satisfies a `sk` request; `sk` does not
satisfy `sk-SK` unless the chain says so.

## Providing translations for third-party modules

A community translator can ship a pack translating modules they don't own:

```json
{
  "name": "@polyglot/slovak-extras",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "language-pack",
    "contributes": {
      "i18n": [
        {
          "id": "weather-sk",
          "locale": "sk",
          "path": "weather/sk.json"
        },
        {
          "id": "cpu-graph-sk",
          "locale": "sk",
          "path": "cpu-graph/sk.json"
        }
      ]
    }
  }
}
```

As long as the target module's keys are stable, the pack works. When a
module renames keys, packs targeting it need an update — the
`TranslationsReloaded` event fires on hot-reload during development.

## Number, date, and duration formatting

These go through `mesh.locale` and follow the active locale's CLDR rules.
Modules should never hand-format dates or numbers — the formatted output
differs across locales (decimal separators, date order, first day of
week, thousand grouping).

```luau
local loc = require("mesh.locale@>=1.0")
loc:format_number(1234567.89)           -- "1 234 567,89" in sk-SK
loc:format_date(os.time(), "short")     -- "20. 4. 2026"
loc:format_duration(3675)               -- "1 h 1 min"
```

Modules can pass locale-specific options (currency, unit systems) through
the `options` map.

## RTL, bidi, and font fallback

- Direction is derived from the active locale and exposed as `locale.dir`
  (`"ltr"` or `"rtl"`).
- Surfaces flip layout automatically when the current locale is RTL; module
  authors use logical properties (`margin-inline-start`) rather than
  physical (`margin-left`) to get correct behaviour.
- Font fallback for scripts the active theme's font lacks is handled by the
  renderer; themes may declare script-specific font stacks.

## Tooling

```
mesh locale list                       # locales available from bundled + packs
mesh locale active                     # current locale + chain
mesh locale set <code>                 # switch primary locale
mesh locale which <module> <key>       # which pack/layer supplied the string
mesh locale missing <module>           # keys the module declares but no locale satisfies
```

`mesh locale extract` is the translator's entry point — it produces a
template JSON with every key the module uses, ready to fill in.

## Summary

- Translations are scoped by module ID; no global namespace collisions.
- Modules bundle their own strings; language packs layer on top; multiple
  packs coexist as an ordered chain.
- Lookups go through `mesh.locale`; missing keys fall through a user-defined
  chain and surface visibly when still missing.
- Number, date, duration, and plural handling are delegated to the locale
  service — modules never format these themselves.
- RTL / bidi / font-fallback are the renderer's job; module CSS should use
  logical properties.
