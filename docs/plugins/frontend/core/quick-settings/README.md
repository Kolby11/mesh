# `@mesh/quick-settings`

Quick settings surface with the common toggles (network, bluetooth, audio,
power).

- **Type:** `surface`
- **Entrypoint:** `src/main.mesh`

## Capabilities

Required:

- `shell.surface`
- `service.network.read`
- `service.audio.read`
- `service.audio.control`
- `service.power.read`
- `theme.read`
- `locale.read`

Optional (the surface must degrade gracefully if these are denied):

- `service.bluetooth.read`
- `service.bluetooth.control`
- `service.network.control`
- `service.power.control`

## UI layout

A column containing:

- Title *Quick Settings*
- A row of toggle chips: **Wi-Fi**, **Bluetooth**, **Do Not Disturb**
- A *Volume* label with a slider (`min=0`, `max=100`)

The surface is intended to be opened by shared shell events from other
frontends. The core only routes those events into shell requests; the
frontend plugins decide when to publish them and how the surface should react.

## Theme tokens

`color.surface`, `color.on-surface`, `spacing.sm`, `spacing.md`, `spacing.lg`,
`typography.size.lg`.

## Accessibility (`<meta>`)

- `role = "dialog"`
- `label = "Quick settings"`
