# MESH Core Plugins

This directory contains the plugins shipped with MESH under the `@mesh`
scope. They provide the default shell experience, reference implementations
for system service integrations, and example compositions for plugin authors.

Plugins are split into two kinds, enforced by the architecture described in
[`spec/pluggable-backend.md`](../../spec/pluggable-backend.md):

- **[Frontend plugins](./frontend/core/README.md)** — shell surfaces and widgets
  that render the UI. They consume services through named **interface
  contracts** and never reference a specific backend.
- **[Backend plugins](./backend/core/README.md)** — implementations of interface
  contracts (`mesh.audio`, `mesh.network`, `mesh.power`, `mesh.media`, …). They
  register with the interface registry and are looked up by interface name, not
  by plugin ID.

Core contract packages now live alongside the default backends under the
backend tree as ordinary `type = "interface"` plugins. The shell core does
not define service behavior; it only discovers contracts, validates them, and
bridges providers to consumers.

The interface registry is the only bridge between the two.

> **Full extensibility is a first-class goal.** The defaults below are
> ordinary plugins with no privileged status. Anyone can ship a backend, a
> frontend, or an entirely new service *category* by declaring a contract
> package. See [`docs/extensibility.md`](../extensibility.md) for the
> dynamic, D-Bus-style interface registry that powers this.

## Layout

The shell discovers plugins by scanning the `plugins/` tree recursively.
Folders like `core/` and `examples/` are organizational only; they do not
change whether a plugin is discoverable.

```
plugins/
├── frontend/
│   ├── core/
│   │   ├── panel/               — top panel surface
│   │   ├── launcher/            — application launcher
│   │   ├── notification-center/ — notification drawer
│   │   └── quick-settings/      — toggles + sliders surface
│   └── examples/                — larger composition examples for plugin authors
└── backend/
    └── core/
        ├── audio-interface/         — contract for mesh.audio
        ├── network-interface/       — contract for mesh.network
        ├── power-interface/         — contract for mesh.power
        ├── media-interface/         — contract for mesh.media
        ├── notifications-interface/ — contract for mesh.notifications
        ├── brightness-interface/    — contract for mesh.brightness
        ├── pipewire-audio/          — mesh.audio via PipeWire
        ├── pulseaudio-audio/        — mesh.audio via PulseAudio
        ├── networkmanager-network/  — mesh.network via NetworkManager
        ├── upower-power/            — mesh.power via UPower
        ├── mpris-media/             — mesh.media via MPRIS D-Bus
        └── mock-notifications/      — mesh.notifications default provider
```

## Plugin anatomy

Every plugin has a `plugin.json` manifest at its root declaring identity,
capabilities, dependencies, entrypoints, and its settings schema — see
[`../installation.md`](../installation.md) for the full format. Frontend surfaces have a
`src/main.mesh` single-file component (`<template>`, `<script lang="luau">`,
`<style>`, `<schema>`, `<meta>`). Backends have a `src/main.luau` entrypoint
that registers an interface implementation with the interface registry.
Interface packages ship an `interface.toml` declaration instead of an
executable entrypoint.

See [`spec/pluggable-backend.md`](../../spec/pluggable-backend.md) for the
authoritative plugin model, lifecycle, capabilities, and distribution rules.

The example frontend plugin set is documented in
[`frontend/examples/README.md`](./frontend/examples/README.md).
