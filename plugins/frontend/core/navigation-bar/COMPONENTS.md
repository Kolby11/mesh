This plugin has been split into smaller presentational components inside `src/components`:

`settings-button.mesh` — clickable wrapper for the settings glyph that toggles the quick settings surface.
`volume-button.mesh` — clickable wrapper for volume that toggles the volume surface and reuses reactive audio globals.
`meta-label.mesh` — presentational component rendering the "current" label.
`meta-pill.mesh` — presentational pill used for dashboard/section labeling.

The main `src/main.mesh` now uses `<battery-button />` in place of the inline battery markup.

Notes:
- Components rely on the parent's global reactive state. They don't declare or set those globals.
- If you prefer local encapsulation, move the state-accessing logic into the component scripts and expose events instead.
