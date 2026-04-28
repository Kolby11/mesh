# Frontend Core Plugins

Frontend plugins render the shell UI. They are declared with
`"type": "surface"` (or `"widget"`) in `plugin.json` and provide a single-file
`.mesh` component as their entrypoint.

Frontends look up services **by interface name only** — never by backend plugin
ID. If no implementation is registered, the lookup returns `nil` and the
frontend is expected to degrade gracefully.

The core's job here is generic: compile the `.mesh` file, host the Luau
runtime, forward raw service payloads into script state, and route emitted
events back into shell requests or backend commands. Frontend-specific display
logic still lives in the plugin.

Frontend composition now happens in two ways:

- Dependency-backed component imports: add a frontend plugin to
  `dependencies.plugins`, then use the tag exported by its
  `plugin.json.exports.component.tag` in `<template>` markup
- Slot hosting via `provides_slots` and `slot_contributions`

If you create a reusable frontend component, export its custom tag explicitly in
`plugin.json.exports.component.tag` so other plugins can consume it as a normal
template tag.

Common frontend pattern:

```luau
mesh.state.set("volume_icon_name", "audio-volume-muted")
mesh.service.bind("audio.muted", "audio_muted")
mesh.service.bind("audio.percent", "audio_percent")
mesh.service.on("audio", "sync_audio_state")

function sync_audio_state()
    if audio_muted or audio_percent == 0 then
        volume_icon_name = "audio-volume-muted"
    elseif audio_percent < 67 then
        volume_icon_name = "audio-volume-medium"
    else
        volume_icon_name = "audio-volume-high"
    end
end
```

`mesh.interfaces.get(...)` is still available for request/response style
lookups, but most reactive UI should treat service payloads as plugin-owned
data and derive its own labels, icons, and tooltips locally.

For interface-centric code, the proxy can now drive both sides of the flow:

```luau
function init()
    local audio = mesh.interfaces.get("mesh.audio", ">=1.0")
    audio:bind("muted", "audio_muted")
    audio:bind("percent", "audio_percent")
    audio:on_change("sync_audio_state")
end

function set_volume(percent)
    local audio = mesh.interfaces.get("mesh.audio", ">=1.0")
    audio:set_volume("default", percent / 100)
end
```

That keeps the call site pure Lua while preserving a clean ownership model:
reactive reads come from service state, while writes go through explicit
interface methods.

For a larger copyable composition example, see
[`docs/plugins/frontend/examples/README.md`](../examples/README.md).

## The `.mesh` component format

Each surface's `src/main.mesh` is a Svelte-inspired single-file component with
these blocks:

| Block | Purpose |
|-------|---------|
| `<template>` | XHTML-like markup describing the UI tree. Dynamic attributes use `{}` and event handlers use `onclick={handler}`-style attributes. |
| `<script lang="luau">` | Luau code implementing state, explicit `mesh.service.bind(...)` and `mesh.service.on(...)` subscriptions, and event handlers. |
| `<style>` | CSS-like styling. Token references use `token(group.name)` and inherit the active theme. Supports `overflow`, `overflow-x`, `overflow-y`, and container breakpoints via `@container (min-width: 640px) { ... }`. |
| `<schema>` | TOML-style declaration of the plugin's public settings. The shell validates user input and can auto-generate a settings UI. |
| `<i18n>` | Translations scoped to the component (optional). |
| `<meta>` | Accessibility and display metadata: `name`, `description`, `role`, `label`. |

## Core surfaces

| Plugin | Manifest ID | Purpose |
|--------|-------------|---------|
| [base-surface](./base-surface/README.md) | `@mesh/base-surface` | Composition test surface with imported launcher/sidebar widgets and configurable placement |
| [navigation-bar](./navigation-bar/README.md) | `@mesh/navigation-bar` | Top-edge navigation bar surface |
| [panel](./panel/README.md) | `@mesh/panel` | Top panel with clock, status icons, system tray |
| [launcher](./launcher/README.md) | `@mesh/launcher` | Application launcher surface |
| [notification-center](./notification-center/README.md) | `@mesh/notification-center` | Notification host surface with content/sidebar slots |
| [quick-settings](./quick-settings/README.md) | `@mesh/quick-settings` | Wi-Fi, Bluetooth, audio, power toggles |
