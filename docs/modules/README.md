# MESH Core Modules

This directory contains the modules shipped with MESH under the `@mesh`
scope. They provide the default shell experience, reference implementations
for system service integrations, and example compositions for module authors.

Modules are split into two kinds, enforced by the architecture described in
[`spec/pluggable-backend.md`](../../spec/pluggable-backend.md):

- **[Frontend modules](./frontend/core/README.md)** — shell surfaces and widgets
  that render the UI. They consume services through named **interface
  contracts** and never reference a specific backend.
- **[Backend modules](./backend/core/README.md)** — implementations of interface
  contracts (`mesh.audio`, `mesh.network`, `mesh.power`, `mesh.media`, …). They
  register with the interface registry and are looked up by interface name, not
  by module ID.

Core contract packages now live alongside the default backends under the
backend tree as ordinary `type = "interface"` modules. The shell core does
not define service behavior; it only discovers contracts, validates them, and
bridges providers to consumers.

The interface registry is the only bridge between the two.

> **Full extensibility is a first-class goal.** The defaults below are
> ordinary modules with no privileged status. Anyone can ship a backend, a
> frontend, or an entirely new service *category* by declaring a contract
> package. See [`docs/extensibility.md`](../extensibility.md) for the
> dynamic, D-Bus-style interface registry that powers this.

## Layout

The shell discovers modules by scanning the `modules/` tree recursively.
Folders like `core/` and `examples/` are organizational only; they do not
change whether a module is discoverable.

```
modules/
├── frontend/
│   ├── core/
│   │   ├── panel/               — top panel surface
│   │   ├── launcher/            — application launcher
│   │   ├── notification-center/ — notification drawer
│   │   └── quick-settings/      — toggles + sliders surface
│   └── examples/                — larger composition examples for module authors
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

## Module anatomy

New modules should use `package.json` with MESH-specific declarations under
the `mesh` key. The top level remains npm-compatible package metadata; use
`mesh.kind`, `mesh.dependencies`, `mesh.capabilities`, `mesh.entrypoints`, and
`mesh.contributes` for shell behavior. Legacy `package.json`, `package.json`,
and `mesh.toml` manifests are still loadable during migration, but new
examples should prefer `package.json`.

Frontend surfaces have a
`src/main.mesh` single-file component (`<template>`, `<script lang="luau">`,
`<style>`, `<i18n>`). Backends have a `src/main.luau` entrypoint
that registers an interface implementation with the interface registry.
Interface packages ship an `interface.toml` declaration instead of an
executable entrypoint.

See [`spec/pluggable-backend.md`](../../spec/pluggable-backend.md) for the
authoritative module model, lifecycle, capabilities, and distribution rules.

The example frontend module set is documented in
[`frontend/examples/README.md`](./frontend/examples/README.md).
