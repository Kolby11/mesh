# `@mesh/pipewire-audio`

Audio backend implemented against **PipeWire**.

- **Type:** `backend`
- **Implements:** interface `mesh.audio` (contract `@mesh/audio-contract`)
- **Backend name:** `PipeWire`
- **Priority:** `100` (default choice on modern Linux systems)
- **Entrypoint:** `src/main.luau`

## Capabilities

Required:

- `service.audio.read` — enumerate devices, read volumes and mute state
- `service.audio.control` — change volume, mute, default output / input
- `dbus.session` — PipeWire is queried via the session bus

## Responsibilities

Implements the methods declared by `mesh.audio`:

- enumerate output and input devices
- report and update per-device volume and mute state
- set default output / input
- emit the contract's events (`DeviceChanged`, `VolumeChanged`, …) on the
  `mesh.audio/*` channels so subscribers (panel, quick-settings) can redraw

## Selection

Picked by auto-detection when PipeWire is running. If PipeWire is missing, the
core falls back to [`@mesh/pulseaudio-audio`](../pulseaudio-audio/README.md)
(priority 50).
