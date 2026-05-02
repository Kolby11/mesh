# Backend Core Plugins

Backend plugins implement **interface contracts** — named, versioned
declarations of guaranteed state fields, command methods, event channels, and
types — and register their implementations with the core's **interface
registry**. Frontend plugins look services up by interface name (`mesh.audio`,
`mesh.network`, …), never by backend plugin ID.

Interface contracts are ordinary distributable packages, not a fixed list
baked into the core. The shell runtime starts with an empty registry and fills
it by discovering these packages from disk. See
[`../../../extensibility.md`](../../../extensibility.md) for the full model;
this page documents the core defaults.

## Base interface plugin model

Each service area has a **base interface plugin** (`type = "interface"`) that
ships an `interface.toml`. That TOML declares:

- `[[state_fields]]` — the guaranteed readable state that every provider of
  this interface must include in its emitted payload.
- `[[methods]]` — mutating command methods that frontend scripts can call.
  Read-style accessors are never methods; they are always state fields.
- `[[events]]` — typed event channels.
- `[types]` — shared type definitions.

Providers that implement the base interface declare their relationship
explicitly using `base_plugin` in their `provides` block:

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

This formal declaration means: the provider exports `mesh.audio` at version
1.0, its portable base contract is `@mesh/audio-interface`, and it must
include all `[[state_fields]]` documented in that base contract.

## State fields vs command methods

The service proxy model is strict about this distinction:

**State fields** (`[[state_fields]]` in the contract TOML) are readable through
the proxy as plain Lua field access. They are populated from the emitted
backend payload and tracked for field-level rerender:

```lua
local audio = require("@mesh/audio@>=1.0")
local p = audio.percent   -- reads from the last emitted payload
local m = audio.muted     -- reads from the last emitted payload
```

**Command methods** (`[[methods]]` in the contract TOML) are mutating
operations that the frontend dispatches to the backend:

```lua
audio.volume_up()
audio.set_volume("sink-1", 0.75)
network.set_wifi_enabled(true)
```

There are no read-style helper methods (no `default_output()`, no
`connections()`, no `active_player()`). State discovery always comes from
emitted fields, never from callable read helpers.

## Runtime-defined extras (dominant provider pattern)

The base contract documents the guaranteed portable minimum. Richer dominant
providers add **additive runtime extras** — additional state fields they emit
beyond the base contract — that bundled UIs can rely on when that provider is
active. These extras appear alongside the base fields in the emitted payload
and are read the same way (plain field access), but they are not guaranteed on
all providers.

Example: the NetworkManager provider is the dominant network provider. Beyond
the base contract fields (`available`, `wifi_enabled`, `connections`,
`devices`), it also emits:

- `networks` — the latest Wi-Fi scan results
- `source_plugin` — the plugin ID that emitted this payload

Frontend surfaces that target the full desktop experience may use these extras.
Their core read path (connectivity status, Wi-Fi toggle) still works on any
provider that satisfies the base contract.

## Backend plugin layout

Two plugin kinds live side by side in the backend core tree:

- `type = "interface"` packages that ship `interface.toml`
- `type = "backend"` providers that implement one of those interfaces

## Backend script ergonomics

Backend Luau scripts expose an `init()` entrypoint and use the `mesh.*` host
API for polling and command handling:

- `init()` is the required backend entrypoint and should contain startup setup
  like poll interval registration
- `mesh.exec(...)` and `mesh.exec_shell(...)` return a result table with
  `success`, `stdout`, `stderr`, and `code`
- `mesh.service.payload()` returns the full current command payload as a Lua
  table
- `mesh.service.has_capability("service.network.control")` checks a granted
  capability directly from the manifest-derived runtime grants
- `mesh.service.emit(data)` emits state to frontend plugins; `data` must
  include all base interface `[[state_fields]]` plus any runtime extras
- `mesh.service.emit_unavailable()` signals that the service is unreachable
- backend commands are handled by `on_command_<name>()` functions (e.g.
  `on_command_set_volume()`)

## Selection rules

1. If the user pins a backend in `~/.config/mesh/config.toml` under
   `[services]`, that backend is used.
2. Otherwise the core auto-detects compatible backends (e.g. is PipeWire
   running? is NetworkManager on D-Bus?) and picks the highest `priority`.
3. If the chosen backend fails to initialize, the next candidate is tried.

Backends can be hot-swapped at runtime. The registry replaces the active
implementation and emits a `BackendChanged` event on the interface's channel
so frontends can re-query.

## Core interface packages

| Plugin | Manifest ID | Declares | Base state fields |
|--------|-------------|----------|-------------------|
| `audio-interface` | `@mesh/audio-interface` | `mesh.audio` | `available`, `percent`, `muted`, `source_plugin` |
| `network-interface` | `@mesh/network-interface` | `mesh.network` | `available`, `wifi_enabled`, `connections`, `devices` |
| `power-interface` | `@mesh/power-interface` | `mesh.power` | `available`, `level`, `charging`, `time_remaining_minutes` |
| `media-interface` | `@mesh/media-interface` | `mesh.media` | `available`, `title`, `artist`, `state` |
| `notifications-interface` | `@mesh/notifications-interface` | `mesh.notifications` | |
| `brightness-interface` | `@mesh/brightness-interface` | `mesh.brightness` | |

## Core backends

| Plugin | Manifest ID | Implements | Base plugin | Backend | Priority |
|--------|-------------|------------|-------------|---------|----------|
| [pipewire-audio](./pipewire-audio/README.md) | `@mesh/pipewire-audio` | `mesh.audio` | `@mesh/audio-interface` | PipeWire | 100 |
| [pulseaudio-audio](./pulseaudio-audio/README.md) | `@mesh/pulseaudio-audio` | `mesh.audio` | `@mesh/audio-interface` | PulseAudio | 50 |
| [networkmanager-network](./networkmanager-network/README.md) | `@mesh/networkmanager` | `mesh.network` | `@mesh/network-interface` | NetworkManager | 100 |
| [upower-power](./upower-power/README.md) | `@mesh/upower` | `mesh.power` | `@mesh/power-interface` | UPower | 100 |
| [mpris-media](./mpris-media/README.md) | `@mesh/mpris-media` | `mesh.media` | `@mesh/media-interface` | MPRIS (D-Bus) | 100 |
| `mock-notifications` | `@mesh/mock-notifications` | `mesh.notifications` | `@mesh/notifications-interface` | Mock Notifications | 100 |

PipeWire and PulseAudio both implement `mesh.audio`; PipeWire wins by
priority on systems where it is available, PulseAudio is the fallback.

NetworkManager is the dominant network provider. It emits the base contract
fields plus the additive runtime extras `networks` and `source_plugin` on
every poll cycle.
