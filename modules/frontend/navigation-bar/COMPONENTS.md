This module has been split into smaller presentational components inside `src/components`:

`settings-button.mesh` — clickable wrapper for the settings glyph that toggles the quick settings surface.
`volume-button.mesh` — clickable wrapper for volume that toggles the volume surface and reuses reactive audio globals.
`meta-label.mesh` — presentational component rendering the "current" label.
`meta-pill.mesh` — presentational pill used for dashboard/section labeling.

The main `src/main.mesh` now uses PascalCase custom component tags such as `<BatteryButton />` in place of inline button markup. Built-in template primitives stay lowercase, for example `<button>`, `<icon>`, and `<text>`.

Notes:
- Each imported component is standalone: it owns its own script state, handlers, and service bindings.
- Parent-to-child data flow must be explicit through component props; imported components do not read parent scope implicitly.
