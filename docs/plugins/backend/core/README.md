# Backend Core Plugins

Backend plugins implement **interface contracts** — named, versioned
declarations of methods, event channels, and types — and register their
implementations with the core's **interface registry**. Frontend plugins then
look services up by interface name (`mesh.audio`, `mesh.network`, …), never by
backend plugin ID.

Interface contracts are ordinary distributable packages, not a fixed list
baked into the core. See [`../../../extensibility.md`](../../../extensibility.md)
for the full model; this page documents the core defaults.

Every backend manifest has `type = "backend"` and a `[service]` section:

```toml
[service]
provides = "mesh.audio"     # interface name this backend implements
backend_name = "PipeWire"   # human-readable name
priority = 100              # higher wins auto-selection

[dependencies]
"@mesh/audio-contract" = ">=1.0.0, <2.0.0"
```

## Selection rules

1. If the user pins a backend in `~/.config/mesh/config.toml` under
   `[services]`, that backend is used.
2. Otherwise the core auto-detects compatible backends (e.g. is PipeWire
   running? is NetworkManager on D-Bus?) and picks the highest `priority`.
3. If the chosen backend fails to initialize, the next candidate is tried.

Backends can be hot-swapped at runtime. The registry replaces the active
implementation and emits a `BackendChanged` event on the interface's channel
so frontends can re-query.

## Core backends

| Plugin | Manifest ID | Implements | Backend | Priority |
|--------|-------------|------------|---------|----------|
| [pipewire-audio](./pipewire-audio/README.md) | `@mesh/pipewire-audio` | `mesh.audio` | PipeWire | 100 |
| [pulseaudio-audio](./pulseaudio-audio/README.md) | `@mesh/pulseaudio-audio` | `mesh.audio` | PulseAudio | 50 |
| [networkmanager-network](./networkmanager-network/README.md) | `@mesh/networkmanager` | `mesh.network` | NetworkManager | 100 |
| [upower-power](./upower-power/README.md) | `@mesh/upower` | `mesh.power` | UPower | 100 |
| [mpris-media](./mpris-media/README.md) | `@mesh/mpris-media` | `mesh.media` | MPRIS (D-Bus) | 100 |

PipeWire and PulseAudio both implement `mesh.audio`; PipeWire wins by
priority on systems where it is available, PulseAudio is the fallback.
