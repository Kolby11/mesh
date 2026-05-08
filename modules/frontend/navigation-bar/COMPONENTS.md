This module has been split into smaller presentational components inside `src/components`:

`battery-button.mesh` — passive battery-status helper mounted on the shipped bar without adding a popover or expanded power surface.
`settings-button.mesh` — clickable wrapper for the settings glyph that keeps the shipped control contract intact.
`theme-button.mesh` — compact theme toggle button that still publishes `shell.set-theme`.
`volume-button.mesh` — clickable wrapper for volume that toggles the audio surface and reuses reactive audio globals.
`meta-label.mesh` — compact status label used inside the passive status cluster.
`meta-pill.mesh` — compact accent pill used for the bounded status-motion proof.

The main `src/main.mesh` mounts `<BatteryButton />`, `<VolumeButton />`, `<ThemeButton />`, `<SettingsButton />`, `<MetaLabel />`, and `<MetaPill />` to build one passive status cluster plus one control cluster. Built-in template primitives stay lowercase, for example `<button>`, `<icon>`, and `<text>`.

Notes:
- Each imported component is standalone: it owns its own script state, handlers, and service bindings.
- Parent-to-child data flow must be explicit through component props; imported components do not read parent scope implicitly.
