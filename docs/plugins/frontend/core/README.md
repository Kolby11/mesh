# Frontend Core Plugins

Frontend plugins render the shell UI. They are declared with
`"type": "surface"` (or `"widget"`) in `plugin.json` and provide a single-file
`.mesh` component as their entrypoint.

Frontends look up services **by interface name only** — never by backend plugin
ID. If no implementation is registered, the lookup returns `nil` and the
frontend is expected to degrade gracefully.

```luau
local audio = mesh.interfaces.get("mesh.audio", ">=1.0")  -- may be nil
if audio then
    local dev = audio:default_output()
    ...
end
```

The returned handle is a proxy over the `mesh.audio` contract — see
[`docs/extensibility.md`](../../../extensibility.md) for the full interface
model.

## The `.mesh` component format

Each surface's `src/main.mesh` is a Svelte-inspired single-file component with
these blocks:

| Block | Purpose |
|-------|---------|
| `<template>` | XHTML-like markup describing the UI tree. Attributes prefixed with `:` are bindings; `@event` attributes are handlers. |
| `<script lang="luau">` | Luau code implementing state, lifecycle hooks (`init`), and event handlers. |
| `<style>` | CSS-like styling. Token references use `token(group.name)` and inherit the active theme. |
| `<schema>` | TOML-style declaration of the plugin's public settings. The shell validates user input and can auto-generate a settings UI. |
| `<i18n>` | Translations scoped to the component (optional). |
| `<meta>` | Accessibility and display metadata: `name`, `description`, `role`, `label`. |

## Core surfaces

| Plugin | Manifest ID | Purpose |
|--------|-------------|---------|
| [panel](./panel/README.md) | `@mesh/panel` | Top panel with clock, status icons, system tray |
| [launcher](./launcher/README.md) | `@mesh/launcher` | Application launcher surface |
| [notification-center](./notification-center/README.md) | `@mesh/notification-center` | Notification list and history |
| [quick-settings](./quick-settings/README.md) | `@mesh/quick-settings` | Wi-Fi, Bluetooth, audio, power toggles |
