# `@mesh/notification-center`

Notification center host surface. It now acts as a container for embeddable
widget plugins rather than owning every piece of UI directly.

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

A two-column row:

- **Content slot** — main notification feed widgets
- **Sidebar slot** — smaller utility widgets like quick actions or filters

The default core build fills those slots via:

- direct dependency-backed component imports:
  `<NotificationFeed/>` from `@mesh/notification-feed`
  and `<NotificationSidebar/>` from `@mesh/notification-sidebar`

This makes the notification center a real composition host: third-party widget
plugins can still contribute to either slot without forking the surface.

## Theme tokens

`color.surface`, `color.on-surface`, `color.surface-variant`, `spacing.sm`,
`spacing.md`, `spacing.lg`, `radius.md`, `typography.size.lg`.

## Accessibility (`<meta>`)

- `role = "region"`
- `label = "Notification center"`
