# `@mesh/pipewire-audio`

Audio backend implemented against **PipeWire**.

- **Type:** `backend provider`
- **Manifest:** `module.json`
- **Implements:** interface `mesh.audio` from `@mesh/audio-interface` through
  `mesh.implements`
- **Backend name:** `PipeWire`
- **Priority:** `100` (default choice on modern Linux systems)
- **Entrypoint:** `src/main.luau`

## Capabilities

Required:

- `exec.wpctl` — call `wpctl` to read and mutate PipeWire state
- `exec.aplay` — play shell sound effects through ALSA

## Responsibilities

Implements the methods declared by `mesh.audio`:

- enumerate output and input devices
- report and update per-device volume and mute state
- set default output / input
- emit the contract's events (`DeviceChanged`, `VolumeChanged`, …) on the
  `mesh.audio/*` channels so subscribers (panel, quick-settings) can redraw

The shell host only routes `mesh.audio` calls to this provider and enforces
capabilities through generic interface/provider records. All PipeWire-specific
behavior stays in `src/main.luau` and is performed through `wpctl`.

## Selection

`config/module.json` selects `@mesh/pipewire-audio` as the active
`mesh.audio` provider in the shipped graph while keeping
[`@mesh/pulseaudio-audio`](../pulseaudio-audio/README.md) available as an
alternate provider record.
