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

- `service.audio.read`
- `service.audio.control`

Unlike the PipeWire backend, no D-Bus capability is needed — PulseAudio is
reached through its native socket protocol.

## Responsibilities

Same `mesh.audio` surface as the PipeWire backend: enumerate devices, read
and mutate volume / mute / default device, emit the contract's events.

## Selection

Used when either:

- PipeWire is not running on the system, or
- the user pins `audio = "@mesh/pulseaudio-audio"` in
  `~/.config/mesh/config.toml`.
