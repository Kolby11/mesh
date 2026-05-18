# Frontend Example Modules

This directory documents a fuller composition example for frontend module
authors. The example modules live in `packages/modules/frontend/examples/`.

The shell discovers them the same way it discovers any other module under the
repo `modules/` tree. The `examples/` folder is a naming convention for human
organization, not a special discovery boundary.

## Example set

### Host surface

- `@mesh/workspace-hub`
  - Surface module
  - Declares required module dependencies on five widget modules
  - Imports dependency-backed component tags directly in template markup
  - Exposes three slot points for third-party extension

### Exported widget dependencies

- `@mesh/date-strip` exports `<DateStrip/>`
- `@mesh/calendar-card` exports `<CalendarCard/>`
- `@mesh/agenda-list` exports `<AgendaList/>`
- `@mesh/focus-timer` exports `<FocusTimer/>`
- `@mesh/status-rail` exports `<StatusRail/>`

These are imported by `@mesh/workspace-hub` through `mesh.dependencies.modules`.
The loader validates that each referenced tag resolves to exactly one required
widget dependency before runtime.

### Slot contributors

- `@mesh/weather-brief`
  - Contributes into `@mesh/workspace-hub:main-extra`
  - Demonstrates layout contribution metadata in `module.json`
- `@mesh/habit-streaks`
  - Contributes into `@mesh/workspace-hub:footer`
  - Demonstrates footer-style chip contributions

## Copyable pattern

1. Export a component tag from the child module:

```json
{
  "mesh": {
    "kind": "frontend",
    "contributes": {
      "layout": [
        {
          "id": "calendar-card",
          "entrypoint": "src/main.mesh",
          "label": "Calendar Card"
        }
      ]
    }
  }
}
```

2. Add the child module as a required module dependency in the host:

```json
{
  "mesh": {
    "dependencies": {
      "modules": {
        "@mesh/calendar-card": ">=0.1.0"
      }
    }
  }
}
```

3. Use the tag directly in the host template:

```xml
<CalendarCard heading="This week" accent="Launch prep"/>
```

4. Optionally expose slots so unrelated modules can extend the host without
editing it:

```json
{
  "mesh": {
    "contributes": {
      "layout": [
        {
          "id": "sidebar-extra",
          "entrypoint": "src/sidebar-extra.mesh",
          "label": "Sidebar Extra"
        }
      ]
    }
  }
}
```

## Why this example exists

The core modules stay fairly small. This example set is more explicit on
purpose: it shows dependency-backed imports, exported tags, host slots, slot
props, and a realistic multi-module layout all in one place.
