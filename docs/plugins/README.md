# MESH Core Plugins

This directory contains the core plugins shipped with MESH under the `@mesh`
scope. They provide the default shell experience and reference implementations
for system service integrations.

Plugins are split into two kinds, enforced by the architecture described in
[`spec/pluggable-backend.md`](../../spec/pluggable-backend.md):

- **[Frontend plugins](./frontend/core/README.md)** — shell surfaces and widgets
  that render the UI. They consume services through named **interface
  contracts** and never reference a specific backend.
- **[Backend plugins](./backend/core/README.md)** — implementations of interface
  contracts (`mesh.audio`, `mesh.network`, `mesh.power`, `mesh.media`, …). They
  register with the interface registry and are looked up by interface name, not
  by plugin ID.

The interface registry is the only bridge between the two.

> **Full extensibility is a first-class goal.** The defaults below are
> ordinary plugins with no privileged status. Anyone can ship a backend, a
> frontend, or an entirely new service *category* by declaring a contract
> package. See [`docs/extensibility.md`](../extensibility.md) for the
> dynamic, D-Bus-style interface registry that powers this.

## Layout

```
plugins/
├── frontend/
│   └── core/
│       ├── panel/               — top panel surface
│       ├── launcher/            — application launcher
│       ├── notification-center/ — notification drawer
│       └── quick-settings/      — toggles + sliders surface
└── backend/
    └── core/
        ├── pipewire-audio/          — mesh.audio via PipeWire
        ├── pulseaudio-audio/        — mesh.audio via PulseAudio
        ├── networkmanager-network/  — mesh.network via NetworkManager
        ├── upower-power/            — mesh.power via UPower
        └── mpris-media/             — mesh.media via MPRIS D-Bus
```

## Plugin anatomy

Every plugin has a `plugin.json` manifest at its root declaring identity,
capabilities, dependencies, entrypoints, and its settings schema — see
[`../installation.md`](../installation.md) for the full format. Frontend surfaces have a
`src/main.mesh` single-file component (`<template>`, `<script lang="luau">`,
`<style>`, `<schema>`, `<meta>`). Backends have a `src/main.luau` entrypoint
that registers an interface implementation with the interface registry.

See [`spec/pluggable-backend.md`](../../spec/pluggable-backend.md) for the
authoritative plugin model, lifecycle, capabilities, and distribution rules.
