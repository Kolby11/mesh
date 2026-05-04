# `@mesh/mpris-media`

Placeholder media backend target for a future real **MPRIS** D-Bus integration.
It is not the Phase 5 MVP authoring reference.

- **Type:** `backend`
- **Implements:** interface `mesh.media` (base plugin `@mesh/media-interface`)
- **Backend name:** `MPRIS`
- **Priority:** `100`
- **Entrypoint:** `src/main.luau`

## Capabilities

Required:

- `service.media.read` — discover players, read metadata and playback state
- `service.media.control` — play / pause / next / previous / seek
- `dbus.session` — MPRIS is exposed on the user's session bus

## Status

- This package remains the future integration target for real MPRIS-backed media behavior.
- Authors looking for the proven backend MVP pattern should start with [`@mesh/reference-media`](../reference-media/README.md) instead.
- `@mesh/reference-media` is the documented reference for top-level `state`, `init()`, polling, config, and `on_command_*` handlers.

## Intended responsibilities

When implemented as a real provider, this backend should cover:

- enumerate active MPRIS players on the session bus
- expose metadata (title, artist, album art URL) and playback state per player
- issue control commands (play/pause/next/previous/seek)
- emit the contract's events when players appear / disappear or state changes

## Notes

Because MPRIS is a protocol rather than a specific daemon, this remains a good
future real-provider target for Linux media players that implement
`org.mpris.MediaPlayer2`. For the current MVP author path, use
`reference-media`, not this placeholder.
