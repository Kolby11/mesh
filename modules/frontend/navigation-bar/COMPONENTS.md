This module has been split into smaller presentational components inside `src/components`:

`battery-button.mesh` — passive battery-status helper mounted on the shipped bar without adding a popover or expanded power surface.
`settings-button.mesh` — settings trigger that owns the quick-settings surface mount and toggle behavior locally.
`theme-button.mesh` — compact theme trigger that owns the theme-selector popover toggle from inside the component.
`volume-button.mesh` — volume trigger that owns the audio-popover surface mount and toggle behavior while reusing reactive audio globals.
`language-button.mesh` — language trigger that owns the language-popover surface mount and toggle behavior locally.
`meta-label.mesh` — compact status label used inside the passive status cluster.
`meta-pill.mesh` — compact accent pill used for the bounded status-motion proof.

The main `src/main.mesh` mounts `<BatteryButton />`, `<VolumeButton />`, `<ThemeButton />`, `<LanguageButton />`, `<SettingsButton />`, `<MetaLabel />`, and `<MetaPill />` to build one passive status cluster plus one control cluster. Built-in template primitives stay lowercase, for example `<button>`, `<icon>`, and `<text>`.

Notes:
- Each imported component is standalone: it owns its own script state, handlers, and service bindings.
- Parent-to-child data flow must be explicit through component props; imported components do not read parent scope implicitly.
