# Frontend Core Plugins

Frontend plugins render the shell UI. They are declared with
`"type": "surface"` (or `"widget"`) in `plugin.json` and provide a single-file
`.mesh` component as their entrypoint.

Frontends look up services **by interface name only** — never by backend plugin
ID. If no implementation is registered, the lookup returns `nil` and the
frontend is expected to degrade gracefully.

Frontend composition now happens in two ways:

- Dependency-backed component imports: add a frontend plugin to
  `dependencies.plugins`, then use the tag exported by its
  `plugin.json.exports.component.tag` in `<template>` markup
- Slot hosting via `provides_slots` and `slot_contributions`

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

For a larger copyable composition example, see
[`docs/plugins/frontend/examples/README.md`](../examples/README.md).

## The `.mesh` component format

Each surface's `src/main.mesh` is a Svelte-inspired single-file component with
these blocks:

| Block | Purpose |
|-------|---------|
| `<template>` | XHTML-like markup describing the UI tree. Attributes prefixed with `:` are bindings; `@event` attributes are handlers. |
| `<script lang="luau">` | Luau code implementing state, lifecycle hooks (`init`), and event handlers. |
| `<style>` | CSS-like styling. Token references use `token(group.name)` and inherit the active theme. Supports `overflow`, `overflow-x`, `overflow-y`, and container breakpoints via `@container (min-width: 640px) { ... }`. |
| `<schema>` | TOML-style declaration of the plugin's public settings. The shell validates user input and can auto-generate a settings UI. |
| `<i18n>` | Translations scoped to the component (optional). |
| `<meta>` | Accessibility and display metadata: `name`, `description`, `role`, `label`. |

## Core surfaces

| Plugin | Manifest ID | Purpose |
|--------|-------------|---------|
| [base-surface](./base-surface/README.md) | `@mesh/base-surface` | Composition test surface with imported launcher/sidebar widgets and configurable placement |
| [panel](./panel/README.md) | `@mesh/panel` | Top panel with clock, status icons, system tray |
| [launcher](./launcher/README.md) | `@mesh/launcher` | Application launcher surface |
| [notification-center](./notification-center/README.md) | `@mesh/notification-center` | Notification host surface with content/sidebar slots |
| [quick-settings](./quick-settings/README.md) | `@mesh/quick-settings` | Wi-Fi, Bluetooth, audio, power toggles |
