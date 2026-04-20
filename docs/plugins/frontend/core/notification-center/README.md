# `@mesh/notification-center`

Notification center and history surface.

- **Type:** `surface`
- **Entrypoint:** `src/main.mesh`

## Capabilities

Required:

- `shell.surface`
- `service.notifications.read` — read notification history
- `service.notifications.manage` — dismiss / modify notifications
- `theme.read`
- `locale.read`

## UI layout

A column with a *Notifications* title and a vertical list of notification
items. Each item is a box showing a summary line and body text. The initial
build ships a single placeholder "System ready — MESH notification center
initialized." item; full notification binding is delegated to the notification
service once wired up.

## Theme tokens

`color.surface`, `color.on-surface`, `color.surface-variant`, `spacing.sm`,
`spacing.md`, `spacing.lg`, `radius.md`, `typography.size.lg`.

## Accessibility (`<meta>`)

- `role = "region"`
- `label = "Notification center"`
