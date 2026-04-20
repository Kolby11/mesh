# `@mesh/mpris-media`

Media playback backend using the **MPRIS** D-Bus protocol. Any MPRIS-compliant
media player (Spotify, Firefox, mpv with a plugin, etc.) is visible through
this backend.

- **Type:** `backend`
- **Implements:** interface `mesh.media` (contract `@mesh/media-contract`)
- **Backend name:** `MPRIS`
- **Priority:** `100`
- **Entrypoint:** `src/main.luau`

## Capabilities

Required:

- `service.media.read` — discover players, read metadata and playback state
- `service.media.control` — play / pause / next / previous / seek
- `dbus.session` — MPRIS is exposed on the user's session bus

## Responsibilities

Implements the methods declared by `mesh.media`:

- enumerate active MPRIS players on the session bus
- expose metadata (title, artist, album art URL) and playback state per player
- issue control commands (play/pause/next/previous/seek)
- emit the contract's events when players appear / disappear or state changes

## Notes

Because MPRIS is a protocol rather than a specific daemon, this backend works
out of the box with essentially any Linux audio/video player that implements
`org.mpris.MediaPlayer2`.
