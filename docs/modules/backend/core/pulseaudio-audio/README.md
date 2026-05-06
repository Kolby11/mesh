# `@mesh/pulseaudio-audio`

Audio backend implemented against **PulseAudio**. Acts as the fallback
`mesh.audio` provider on systems without PipeWire.

- **Type:** `backend`
- **Implements:** interface `mesh.audio` (contract `@mesh/audio-contract`)
- **Backend name:** `PulseAudio`
- **Priority:** `50` (lower than PipeWire on purpose)
- **Entrypoint:** `src/main.luau`

## Capabilities

Required:

- `exec.pactl` — call `pactl` to read and mutate PulseAudio state
- `exec.aplay` — play shell sound effects through ALSA

No D-Bus capability is needed. The backend script talks to PulseAudio through
`pactl`; the shell host only routes `mesh.audio` calls to the selected
provider.

## Responsibilities

Same `mesh.audio` surface as the PipeWire backend: enumerate devices, read
and mutate volume / mute / default device, emit the contract's events.

## Selection

Used when either:

- PipeWire is not running on the system, or
- the user pins `audio = "@mesh/pulseaudio-audio"` in
  `~/.config/mesh/config.toml`.
