# `@mesh/base-surface`

Composable base surface used to test dependency-backed widget imports inside a
single top-level surface.

- **Type:** `surface`
- **Entrypoint:** `src/main.mesh`

## What it demonstrates

- Direct frontend composition through required plugin dependencies:
  `@mesh/base-launcher-widget` and `@mesh/base-sidebar-widget`
- Host markup that imports exported component tags directly:
  `<BaseLauncher/>` and `<BaseSidebar/>`
- Two extension slots: `main` and `sidebar`
- Surface placement and startup visibility coming from
  `config/settings.json`

## Settings

`config/settings.json` includes a `surface` section the shell now reads for
top-level placement:

```json
{
  "surface": {
    "anchor": "right",
    "layer": "overlay",
    "width": 1120,
    "height": 720,
    "exclusive_zone": 0,
    "keyboard_mode": "on_demand",
    "visible_on_start": true
  }
}
```

Changing those values moves/resizes the surface without editing Rust code.

The same settings JSON is also exposed to the frontend runtime as `settings`,
so the template can bind values like `settings.surface.anchor`.
