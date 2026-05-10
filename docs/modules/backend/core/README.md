# Backend Core Modules

Backend modules implement interface contracts such as `mesh.audio`,
`mesh.network`, `mesh.power`, and `mesh.media`. Frontend and shell-side Lua
code consume the active provider through the interface import, not by reaching
for a concrete backend package id.

See [`../../../extensibility.md`](../../../extensibility.md) for the broader
model. This page documents the bundled backend contracts and the current MVP
authoring path.

## Base interface module model

Each service area has a base interface module (`type = "interface"`) that
ships an `interface.toml`. That contract declares:

- `[[state_fields]]` — required public state fields
- `[[methods]]` — mutating command methods
- `[[events]]` — typed interface event channels
- `[types]` — shared types

Providers implement that contract through an `implements` entry in `package.json`.
Legacy `package.json` manifests may still use `provides` during migration:

```json
{
  "mesh": {
    "kind": "backend",
    "implements": [
      {
        "interface": "mesh.audio",
        "version": "1.0",
        "baseModule": "@mesh/audio-interface",
        "provider": "pipewire",
        "label": "PipeWire",
        "priority": 100
      }
    ]
  }
}
```

## State fields vs command methods

The MVP contract separates readable state from mutating commands:

```lua
local audio = require("mesh.audio")
local percent = audio.state.percent
local muted = audio.state.muted
```

```lua
local result = audio.set_volume("default", 0.75)
if not result.ok then
    mesh.log.warn(result.error)
end
```

For the migrated MVP interfaces (`mesh.audio`, `mesh.network`, `mesh.power`,
and `mesh.media`), read current service data from
`require("mesh.<interface>").state` and use interface methods only for
commands. Some older bundled interfaces still retain read helpers until they
move to the same state-first pattern.

## Runtime-defined extras

The base contract is the guaranteed portable minimum. Some providers publish
extra state fields beyond the contract, but provider identity is runtime
metadata and should not be authored into public state as `source_module`.

Frontend code should treat extras as provider-specific and keep its portable
path rooted in the base contract's `state` fields.

## Backend module layout

Two module kinds live side by side in the backend core tree:

- `type = "interface"` packages that ship `interface.toml`
- `type = "backend"` providers that implement one of those interfaces

## Backend script ergonomics

Backend Luau providers use an `init()` entrypoint, top-level exported `state`,
and the `mesh.*` host API:

- `init()` performs startup work such as logging, config reads, and poll interval setup
- `mesh.config()` returns the module settings table
- `mesh.exec(program, args)` is the public process API and returns `success`, `stdout`, `stderr`, and `code`
- `mesh.service.set_poll_interval(ms)` changes future poll cadence
- `mesh.service.payload()` returns the current command payload table
- backend commands are implemented as `on_command_<name>()` functions
- public provider state is exported through top-level `state = { ... }`

`mesh.exec` is generic host plumbing. A backend must declare either
`exec.command` or the specific binary capabilities it needs, such as
`exec.wpctl` or `exec.pactl`; service capabilities like `service.audio.control`
belong to consumers that call the interface.

For a concrete MVP example, start with
[`reference-media`](./reference-media/README.md). It is the bundled proof
module for `mesh.config()`, exported `state`, `init()`, polling, and
command-result handlers.

## Provider selection rules

Provider selection is explicit and visible:

1. The installed module graph records which provider is active for each interface.
2. The shell starts the configured active provider for that interface.
3. If that provider fails to load or initialize, the failure is surfaced through runtime status and diagnostics.

The shell does not silently try the next provider after an init failure. There
is no hidden fallback path in the normal MVP contract.

## Core interface packages

| Module                    | Manifest ID                     | Declares             | Base state fields                                          |
| ------------------------- | ------------------------------- | -------------------- | ---------------------------------------------------------- |
| `audio-interface`         | `@mesh/audio-interface`         | `mesh.audio`         | `available`, `percent`, `muted`                            |
| `network-interface`       | `@mesh/network-interface`       | `mesh.network`       | `available`, `wifi_enabled`, `connections`, `devices`      |
| `power-interface`         | `@mesh/power-interface`         | `mesh.power`         | `available`, `level`, `charging`, `time_remaining_minutes` |
| `media-interface`         | `@mesh/media-interface`         | `mesh.media`         | `available`, `title`, `artist`, `album`, `state`           |
| `notifications-interface` | `@mesh/notifications-interface` | `mesh.notifications` |                                                            |
| `brightness-interface`    | `@mesh/brightness-interface`    | `mesh.brightness`    |                                                            |

## Core backends

| Module                                                       | Manifest ID                | Implements           | Base module                     | Backend            | Priority | Notes                               |
| ------------------------------------------------------------ | -------------------------- | -------------------- | ------------------------------- | ------------------ | -------- | ----------------------------------- |
| [pipewire-audio](./pipewire-audio/README.md)                 | `@mesh/pipewire-audio`     | `mesh.audio`         | `@mesh/audio-interface`         | PipeWire           | 100      | Real integration                    |
| [pulseaudio-audio](./pulseaudio-audio/README.md)             | `@mesh/pulseaudio-audio`   | `mesh.audio`         | `@mesh/audio-interface`         | PulseAudio         | 50       | Real integration                    |
| [networkmanager-network](./networkmanager-network/README.md) | `@mesh/networkmanager`     | `mesh.network`       | `@mesh/network-interface`       | NetworkManager     | 100      | Real integration                    |
| [upower-power](./upower-power/README.md)                     | `@mesh/upower`             | `mesh.power`         | `@mesh/power-interface`         | UPower             | 100      | Real integration                    |
| [reference-media](./reference-media/README.md)               | `@mesh/reference-media`    | `mesh.media`         | `@mesh/media-interface`         | Reference          | 10       | Recommended MVP authoring reference |
| [mpris-media](./mpris-media/README.md)                       | `@mesh/mpris-media`        | `mesh.media`         | `@mesh/media-interface`         | MPRIS              | 100      | Future media integration            |
| `mock-notifications`                                         | `@mesh/mock-notifications` | `mesh.notifications` | `@mesh/notifications-interface` | Mock Notifications | 100      | Placeholder backend                 |
