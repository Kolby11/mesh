# `@mesh/panel`

Top panel shell surface. This is the default top-edge bar and typically the
first surface a user sees.

- **Type:** `surface`
- **Entrypoint:** `src/main.mesh`
- **Compositor requirement:** `wlr-layer-shell-v1`

## Capabilities

Required:

- `shell.surface` — anchor to the top screen edge
- `theme.read` — consume theme tokens
- `locale.read` — localized strings

## UI layout

The panel is a three-column row:

- **Left** — active workspace indicator
- **Center** — clock (`%H:%M`)
- **Right** — network icon, clickable volume icon, battery percentage

Clicking the volume icon emits the `shell.toggle-quick-settings` channel so
the quick-settings surface can react.

## Consumed interfaces

Looked up via `mesh.interfaces.get(name, range)`; all are optional and fall
back to `"N/A"` / `"0"` / `"disconnected"` when no implementation is
registered.

| Interface | Used for |
|-----------|----------|
| `mesh.audio` | Reading `default_output().volume` |
| `mesh.power` | Reading `battery().level` |
| `mesh.network` | Reading `active_connection()` |

## Settings (`<schema>`)

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `clock_format` | enum(`"12h"`, `"24h"`) | `"24h"` | Clock display format |
| `show_seconds` | boolean | `false` | Show seconds in clock |
| `show_battery_percent` | boolean | `true` | Show battery percentage |

## Theme tokens

Uses `color.surface`, `color.on-surface`, `spacing.sm`, `spacing.md`,
`typography.size.md`, `typography.size.sm`.

## Accessibility (`<meta>`)

- `role = "toolbar"`
- `label = "System panel"`
