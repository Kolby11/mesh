# `@mesh/pulseaudio-audio`

Audio backend implemented against **PulseAudio**. Acts as the fallback
`mesh.audio` provider on systems without PipeWire.

- **Type:** `backend provider`
- **Manifest:** `module.json`
- **Implements:** interface `mesh.audio` from `@mesh/audio-interface` through
  `mesh.implements`
- **Backend name:** `PulseAudio`
- **Priority:** `50` (lower than PipeWire on purpose)
- **Entrypoint:** `src/main.luau`

## Capabilities

Required:

- `exec.pactl` — call `pactl` to read and mutate PulseAudio state
- `exec.aplay` — play shell sound effects through ALSA

No D-Bus capability is needed. The backend script talks to PulseAudio through
`pactl`; the shell host only routes `mesh.audio` calls to the selected
provider through generic interface/provider records.

## Responsibilities

Same `mesh.audio` interface contract as the PipeWire backend: enumerate
devices, read and mutate volume / mute / default device, emit the contract's
events. All PulseAudio-specific behavior stays in `src/main.luau`.

## Selection

The shipped `config/module.json` keeps this provider enabled as the alternate
`mesh.audio` provider while selecting `@mesh/pipewire-audio` as active.
Users or distributions can pin `mesh.audio` to `@mesh/pulseaudio-audio`
without changing frontend modules.
