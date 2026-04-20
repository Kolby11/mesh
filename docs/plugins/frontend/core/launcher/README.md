# `@mesh/launcher`

Default application launcher surface.

- **Type:** `surface`
- **Entrypoint:** `src/main.mesh`
- **Compositor requirement:** `wlr-layer-shell-v1`

## Capabilities

Required:

- `shell.surface` — launcher window
- `shell.input.keyboard` — typed search input and keyboard navigation
- `exec.launch-app` — launch applications via desktop entries
- `theme.read`
- `locale.read`

## UI layout

A column containing:

- Title headline
- Search input (placeholder: *Search apps*)
- Status line
- Scrollable list of result buttons (Terminal, Files, Browser, Editor, System
  Monitor, Settings, Music, Messages)

Button labels are run through the i18n helper (`require("@mesh/i18n").t`) so
they localize to the active locale.

## Localization

`config/settings.json` declares the i18n setup:

```json
{
    "i18n": {
        "available_locales": ["en", "sk"],
        "default_locale": "en"
    }
}
```

Translation bundles live in `config/i18n/` (`en.json`, `sk.json`).

## Theme tokens

`color.surface`, `color.on-surface`, `color.on-surface-variant`, `radius.lg`,
`spacing.sm`, `spacing.md`, `spacing.lg`, `typography.size.lg`,
`typography.size.sm`.

## Accessibility (`<meta>`)

- `role = "dialog"`
- `label = "Application launcher"`
