# Frontend Core Modules

Frontend modules render the shell UI. They are declared with
`"type": "surface"` (or `"widget"`) in `package.json` and provide a single-file
`.mesh` component as their entrypoint.

Frontends look up services **by interface name only** — never by backend module
ID. If no implementation is registered, `pcall(require, ...)` returns false and
the frontend is expected to degrade gracefully with visible explanatory copy.

The core's job here is generic: compile the `.mesh` file, host the Luau
runtime, forward raw service payloads into script state, and route emitted
events back into shell requests or backend commands. Frontend-specific display
logic still lives in the module.

## Reading service state

Service proxies are a **state view and command surface**. Read state fields
directly from the proxy on rerender; call named command methods for backend
mutations. Do not use callback-style subscriptions — there are none.

```luau
-- Acquire the proxy with a version constraint.
-- pcall catches failures so the surface degrades gracefully.
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

-- Reactive globals are the template's source of truth.
volumeIcon = "audio-volume-muted"
volumeLevel = "0%"

-- onRender() is called by the shell on each rerender.
-- Read proxy fields directly here — no callbacks needed.
function onRender()
    if not audio_ok or not audio then
        -- Explicit user-visible copy for the degraded path.
        volumeIcon = "audio-volume-muted"
        volumeLevel = "0%"
        return
    end
    local pct = audio.percent or 0
    local muted = audio.muted or false
    volumeLevel = string.format("%d%%", pct)
    if muted or pct == 0 then
        volumeIcon = "audio-volume-muted"
    elseif pct < 34 then
        volumeIcon = "audio-volume-low"
    elseif pct < 67 then
        volumeIcon = "audio-volume-medium"
    else
        volumeIcon = "audio-volume-high"
    end
end
```

The runtime tracks every top-level field read from the proxy (`audio.percent`,
`audio.muted`, etc.) and rerenders the component only when those specific field
values change in the next backend emission — not on every emission.

## Issuing backend commands

Write to a backend by calling a named command method on the proxy. The method
publishes a `ServiceCommand` event to the backend's command channel. The backend
handles it, emits updated state, and the frontend rerenders if any tracked field
changed.

```luau
function onVolumeUp()
    if audio_ok and audio then
        audio.volume_up()
    end
end

function onToggleWiFi()
    local network_ok, network = pcall(require, "@mesh/network@>=1.0")
    if network_ok and network then
        -- Read current state directly from the proxy, then send the command.
        local enabled = network.wifi_enabled or false
        network.set_wifi_enabled(not enabled)
    end
end
```

The command lifecycle is:
1. Frontend calls a proxy command method (`audio.volume_up()`).
2. Shell routes it as a `CoreRequest::ServiceCommand` to the backend.
3. Backend handles it, emits updated state.
4. Shell applies the updated payload; tracked fields are compared.
5. Component is rerendered if any tracked field value changed.

Do **not** mutate proxy fields directly — proxy reads are always sourced from
the backend-emitted payload and are immutable on the frontend side.

## Element events and reactive globals

Element handlers update top-level reactive globals, and changed globals drive
the next render. `onchange` handlers receive a typed value directly: sliders
receive a number, switches and checkboxes receive a boolean, and text inputs
receive a string.

```xml
<template>
  <slider min="0" max="1" value="{slider_value}" onchange={onVolumeChange} />
</template>

<script lang="luau">
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

slider_value = 0.0
audio_tooltip = "Audio unavailable"

function onVolumeChange(value)
    local normalized = math.max(0.0, math.min(1.0, value))
    slider_value = normalized
    audio_tooltip = string.format("Volume %d%%", math.floor(normalized * 100))

    if audio_ok and audio then
        audio.set_volume("default", normalized)
    else
        audio_tooltip = "Audio unavailable"
    end
end
</script>
```

Handler failures are recorded in diagnostics and logged by the shell. They do
not crash rendering or clear the last successfully rendered tree, so surfaces
remain visible while operators inspect the diagnostic entry.

## Degraded-state pattern

Always guard `require` calls with `pcall`. When a service is unavailable, show
explicit user-readable fallback copy — not blank UI. The diagnostic system
records the failed lookup even when `pcall` catches the Lua error, so operators
can see which interfaces are missing.

```luau
-- Guard every service require.
local network_ok, network = pcall(require, "@mesh/network@>=1.0")
if not network_ok then network = nil end

wifi_networks = {}

function onRender()
    if not network_ok or not network then
        -- Fallback copy that is user-visible in the Quick Settings drawer.
        wifi_networks = {}
        return
    end
    -- Read live state fields directly.
    local nets = network.networks
    wifi_networks = (type(nets) == "table") and nets or {}
end
```

For text-based indicators, produce a non-empty string:

```luau
batteryText = "N/A"  -- shown in the panel when power service is absent
```

## Shell surface events

Built-in shell surfaces route **surface toggle requests** through
`mesh.events.publish(...)`. This is distinct from backend commands — do not
confuse the two:

```luau
function onVolumeClick()
    -- Shell event: asks the shell to toggle a named surface.
    mesh.events.publish("shell.toggle-surface", { surface_id = "@mesh/quick-settings" })
end

function onClose()
    mesh.events.publish("shell.hide-surface", { surface_id = "@mesh/quick-settings" })
end
```

Service mutations always go through proxy command methods. Shell UI transitions
always go through `mesh.events.publish`.

## Component imports

Frontend composition happens in two ways:

- Dependency-backed component imports: add a frontend module to
  `dependencies.modules`, then import the module ID in the `<script>` block
  and use the imported PascalCase alias in `<template>` markup.
- Slot hosting via `provides_slots` and `slot_contributions`.

Built-in template primitives are lowercase (`<row>`, `<button>`, `<text>`).
Custom component tags are PascalCase (`<AudioSection />`) so component
boundaries are visually distinct from MESH primitives. They must be imported
explicitly in the `<script>` block:

```luau
import AudioSection from "./components/audio-section.mesh"
import WifiSection from "./components/wifi-section.mesh"
import CalendarCard from "@mesh/calendar-card"
```

If you create a reusable frontend component, export its custom tag explicitly in
`package.json.exports.component.tag` so other modules can consume it as a normal
template tag.

## The `.mesh` component format

Each surface's `src/main.mesh` is a Svelte-inspired single-file component with
these blocks:

| Block                  | Purpose                                                                                                                                                                                                          |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `<template>`           | XHTML-like markup describing the UI tree. Dynamic attributes use `{}` and event handlers use `onclick={handler}`-style attributes.                                                                               |
| `<script lang="luau">` | Luau code implementing state, service proxy reads via `require("@mesh/<service>")`, display-state derivation in `onRender()`, and element event handlers.                                                        |
| `<style>`              | CSS-like styling. Token references use `token(group.name)` and inherit the active theme. Supports `overflow`, `overflow-x`, `overflow-y`, and container breakpoints via `@container (min-width: 640px) { ... }`. |
| `<i18n>`               | Translations scoped to the component (optional).                                                                                                                                                                 |

## Core surfaces

| Module                                                 | Manifest ID                 | Purpose                                                                                    |
| ------------------------------------------------------ | --------------------------- | ------------------------------------------------------------------------------------------ |
| [base-surface](./base-surface/README.md)               | `@mesh/base-surface`        | Composition test surface with imported launcher/sidebar widgets and configurable placement |
| [navigation-bar](./navigation-bar/README.md)           | `@mesh/navigation-bar`      | Top-edge navigation bar surface                                                            |
| [panel](./panel/README.md)                             | `@mesh/panel`               | Top panel with clock, status icons, system tray                                            |
| [launcher](./launcher/README.md)                       | `@mesh/launcher`            | Application launcher surface                                                               |
| [notification-center](./notification-center/README.md) | `@mesh/notification-center` | Notification host surface with content/sidebar slots                                       |
| [quick-settings](./quick-settings/README.md)           | `@mesh/quick-settings`      | Wi-Fi, Bluetooth, audio, power toggles                                                     |
