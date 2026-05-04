# Backend Core Plugins

Backend plugins implement interface contracts such as `mesh.audio`,
`mesh.network`, `mesh.power`, and `mesh.media`. Frontend and shell-side Lua
code consume the active provider through the interface import, not by reaching
for a concrete backend package id.

See [`../../../extensibility.md`](../../../extensibility.md) for the broader
model. This page documents the bundled backend contracts and the current MVP
authoring path.

## Base interface plugin model

Each service area has a base interface plugin (`type = "interface"`) that
ships an `interface.toml`. That contract declares:

- `[[state_fields]]` — required public state fields
- `[[methods]]` — mutating command methods
- `[[events]]` — typed interface event channels
- `[types]` — shared types

Providers implement that contract through a `provides` entry in `plugin.json`:

```json
{
  "type": "backend",
  "provides": [
    {
      "interface": "mesh.audio",
      "version": "1.0",
      "base_plugin": "@mesh/audio-interface",
      "backend_name": "PipeWire",
      "priority": 100
    }
  ]
}
```

## State fields vs command methods

The MVP contract separates readable state from mutating commands:

```lua
local audio = require("@mesh/audio")
local percent = audio.state.percent
local muted = audio.state.muted
```

```lua
local result = audio.set_volume("default", 0.75)
if not result.ok then
    mesh.log.warn(result.error)
end
```

There are no read-style helper methods in the MVP path. Read current service
data from `require("@mesh/<interface>").state`; use interface methods only for
commands.

## Runtime-defined extras

The base contract is the guaranteed portable minimum. Some providers publish
extra state fields beyond the contract, but provider identity is runtime
metadata and should not be authored into public state as `source_plugin`.

Frontend code should treat extras as provider-specific and keep its portable
path rooted in the base contract's `state` fields.

## Backend plugin layout

Two plugin kinds live side by side in the backend core tree:

- `type = "interface"` packages that ship `interface.toml`
- `type = "backend"` providers that implement one of those interfaces

## Backend script ergonomics

Backend Luau providers use an `init()` entrypoint, top-level exported `state`,
and the `mesh.*` host API:

- `init()` performs startup work such as logging, config reads, and poll interval setup
- `mesh.config()` returns the plugin settings table
- `mesh.exec(program, args)` is the public process API and returns `success`, `stdout`, `stderr`, and `code`
- `mesh.service.set_poll_interval(ms)` changes future poll cadence
- `mesh.service.payload()` returns the current command payload table
- backend commands are implemented as `on_command_<name>()` functions
- public provider state is exported through top-level `state = { ... }`

For a concrete MVP example, start with
[`reference-media`](./reference-media/README.md). It is the bundled proof
plugin for `mesh.config()`, exported `state`, `init()`, polling, and
command-result handlers.

## Provider selection rules

Provider selection is explicit and visible:

1. The installed module graph records which provider is active for each interface.
2. The shell starts the configured active provider for that interface.
3. If that provider fails to load or initialize, the failure is surfaced through runtime status and diagnostics.

The shell does not silently try the next provider after an init failure. There
is no hidden fallback path in the normal MVP contract.

## Core interface packages

| Plugin | Manifest ID | Declares | Base state fields |
|--------|-------------|----------|-------------------|
| `audio-interface` | `@mesh/audio-interface` | `mesh.audio` | `available`, `percent`, `muted` |
| `network-interface` | `@mesh/network-interface` | `mesh.network` | `available`, `wifi_enabled`, `connections`, `devices` |
| `power-interface` | `@mesh/power-interface` | `mesh.power` | `available`, `level`, `charging`, `time_remaining_minutes` |
| `media-interface` | `@mesh/media-interface` | `mesh.media` | `available`, `title`, `artist`, `album`, `state` |
| `notifications-interface` | `@mesh/notifications-interface` | `mesh.notifications` | |
| `brightness-interface` | `@mesh/brightness-interface` | `mesh.brightness` | |

## Core backends

| Plugin | Manifest ID | Implements | Base plugin | Backend | Priority | Notes |
|--------|-------------|------------|-------------|---------|----------|-------|
| [pipewire-audio](./pipewire-audio/README.md) | `@mesh/pipewire-audio` | `mesh.audio` | `@mesh/audio-interface` | PipeWire | 100 | Real integration |
| [pulseaudio-audio](./pulseaudio-audio/README.md) | `@mesh/pulseaudio-audio` | `mesh.audio` | `@mesh/audio-interface` | PulseAudio | 50 | Real integration |
| [networkmanager-network](./networkmanager-network/README.md) | `@mesh/networkmanager` | `mesh.network` | `@mesh/network-interface` | NetworkManager | 100 | Real integration |
| [upower-power](./upower-power/README.md) | `@mesh/upower` | `mesh.power` | `@mesh/power-interface` | UPower | 100 | Real integration |
| [reference-media](./reference-media/README.md) | `@mesh/reference-media` | `mesh.media` | `@mesh/media-interface` | Reference | 10 | Recommended MVP authoring reference |
| [mpris-media](./mpris-media/README.md) | `@mesh/mpris-media` | `mesh.media` | `@mesh/media-interface` | MPRIS | 100 | Future media integration |
| `mock-notifications` | `@mesh/mock-notifications` | `mesh.notifications` | `@mesh/notifications-interface` | Mock Notifications | 100 | Placeholder backend |
