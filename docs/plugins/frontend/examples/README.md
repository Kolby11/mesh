# Frontend Example Plugins

This directory documents a fuller composition example for frontend plugin
authors. The example plugins live in `plugins/frontend/examples/`.

The shell discovers them the same way it discovers any other plugin under the
repo `plugins/` tree. The `examples/` folder is a naming convention for human
organization, not a special discovery boundary.

## Example set

### Host surface

- `@mesh/workspace-hub`
  - Surface plugin
  - Declares required plugin dependencies on five widget plugins
  - Uses exported component tags directly in template markup
  - Exposes three slot points for third-party extension

### Exported widget dependencies

- `@mesh/date-strip` exports `<DateStrip/>`
- `@mesh/calendar-card` exports `<CalendarCard/>`
- `@mesh/agenda-list` exports `<AgendaList/>`
- `@mesh/focus-timer` exports `<FocusTimer/>`
- `@mesh/status-rail` exports `<StatusRail/>`

These are imported by `@mesh/workspace-hub` through `dependencies.plugins`.
The loader validates that each referenced tag resolves to exactly one required
widget dependency before runtime.

### Slot contributors

- `@mesh/weather-brief`
  - Contributes into `@mesh/workspace-hub:main-extra`
  - Demonstrates slot props in `plugin.json`
- `@mesh/habit-streaks`
  - Contributes into `@mesh/workspace-hub:footer`
  - Demonstrates footer-style chip contributions

## Copyable pattern

1. Export a component tag from the child plugin:

```json
{
  "exports": {
    "component": { "tag": "CalendarCard" }
  }
}
```

2. Add the child plugin as a required plugin dependency in the host:

```json
{
  "dependencies": {
    "plugins": {
      "@mesh/calendar-card": ">=0.1.0"
    }
  }
}
```

3. Use the tag directly in the host template:

```xml
<CalendarCard heading="This week" accent="Launch prep"/>
```

4. Optionally expose slots so unrelated plugins can extend the host without
editing it:

```json
{
  "provides_slots": {
    "sidebar-extra": { "accepts": "widget", "layout": "column", "max": 3 }
  }
}
```

## Why this example exists

The core plugins stay fairly small. This example set is more explicit on
purpose: it shows dependency-backed imports, exported tags, host slots, slot
props, and a realistic multi-plugin layout all in one place.
