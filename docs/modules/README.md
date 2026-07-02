# MESH Core Modules

This directory contains the modules shipped with MESH under the `@mesh`
scope. They provide the default shell experience, reference implementations
for system service integrations, and example compositions for module authors.
The canonical vocabulary for these docs is
[`docs/spec/01-module-system.md`](../spec/01-module-system.md).

Modules are split into two kinds, enforced by the architecture described in
[`docs/spec/01-module-system.md`](../spec/01-module-system.md):

- **[Frontend modules](./frontend/core/README.md)** — shell surfaces and widgets
  that render the UI. They consume services through named **interface
  contracts** and never reference a specific backend.
- **[Backend modules](./backend/core/README.md)** — implementations of interface
  contracts (`mesh.audio`, `mesh.network`, `mesh.power`, `mesh.media`, …). They
  register with the interface registry and are looked up by interface name, not
  by module ID.

Core interface modules live alongside the default backends under the
backend tree as ordinary `kind = "interface"` modules. The shell core does
not define service behavior; it only discovers contracts, validates them, and
bridges providers to consumers.

The interface registry is the only bridge between the two.

> **Full extensibility is a first-class goal.** The defaults below are
> ordinary modules with no privileged status. Anyone can ship a backend, a
> frontend, or an entirely new interface domain by declaring an interface
> module. See [`docs/spec/01-module-system.md`](../spec/01-module-system.md) for the
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
│   ├── text-selection-proof/    — passive selectable-text proof surface
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

## Standalone frontend proof surfaces

Some shipped frontends live outside `core/` and `examples/` because they are
small proof modules rather than part of the default daily shell chrome.

- [`frontend/text-selection-proof`](./frontend/text-selection-proof/README.md)
  — selectable-text proof surface used to exercise passive text selection and
  clipboard-copy behavior without implying full document editing semantics.

## Module anatomy

New modules should use `module.json` with MESH-specific declarations under
the `mesh` key. Use `mesh.kind`, `mesh.dependencies`, `mesh.capabilities`,
`mesh.entrypoints`, and `mesh.contributes` for shell behavior. Old manifest
names are listed in the vocabulary inventory as replacement/internal migration
debt; new examples should prefer `module.json`.

Frontend surfaces have a
`src/main.mesh` single-file component (`<template>`, `<script lang="luau">`,
`<style>`). Backends have a `src/main.luau` entrypoint
that registers an interface implementation with the interface registry.
Interface modules ship an `interface.toml` declaration instead of an
executable entrypoint.

See [`docs/spec/01-module-system.md`](../spec/01-module-system.md) for the
authoritative module model, lifecycle, capabilities, and distribution rules.

The example frontend module set is documented in
[`frontend/examples/README.md`](./frontend/examples/README.md).
