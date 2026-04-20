# Slots

Slots let third parties **extend an existing surface without forking it**. A
surface declares named slot points in its template; other plugins contribute
widgets into those slots via their manifest; the user reorders, disables, or
adds contributions through configuration.

This is the core reuse mechanism for the frontend. It's the difference
between "you can replace the whole panel with your own" and "you can drop a
weather widget into the default panel".

## Slot declaration

A surface marks insertion points in its template with `<slot>`:

```xml
<template>
  <row class="panel">
    <row class="left">
      <slot name="left"  accepts="widget" layout="row" max="4"/>
    </row>
    <row class="center">
      <slot name="center" accepts="widget" max="1"/>
    </row>
    <row class="right">
      <slot name="right" accepts="widget" layout="row" max="8"/>
    </row>
  </row>
</template>
```

Slot attributes:

| Attribute | Purpose |
|-----------|---------|
| `name` | Slot identifier, unique within the surface. Addressable as `<surface-id>:<name>`. |
| `accepts` | `"widget"` today; future values could allow nested surfaces or specific contract-typed content. |
| `layout` | Hint to the renderer: `"row"`, `"column"`, `"stack"`. Default `"row"`. |
| `max` | Maximum number of contributions. Extras are dropped with a diagnostic. |
| `min` | Minimum expected. Below this, a placeholder / empty-state hint renders. |
| `default` | ID of a widget to render when no contributions exist (e.g. a clock in `panel:center`). |

A surface **should document its slots** in its README and in `<meta>`. This
is the plugin's public contribution API.

## Contributing a widget

Plugins contribute via manifest. No code changes to the target surface.

```toml
# @community/weather-widget / mesh.toml
[package]
id   = "@community/weather-widget"
type = "widget"

[slot-contributions]
"@mesh/panel:right" = [
  { props = { units = "metric" }, order = 100 }
]
"@mesh/quick-settings:toggles" = [
  { order = 200 }
]
```

Each contribution entry carries:

| Field | Purpose |
|-------|---------|
| `widget` | Widget ID. Omitted when the contributing plugin *is* the widget (common case). |
| `props` | Props passed to the widget instance. Must validate against the widget's prop schema. |
| `order` | Sort key within the slot. Lower first. |
| `when` | Optional condition expression (capability presence, interface available, user setting). If false, the contribution is skipped. |
| `id` | Optional stable ID for the contribution. Needed if the same widget contributes twice into the same slot. |

The surface's `accepts` constraint is enforced: a contribution that doesn't
match is rejected at load with a diagnostic.

## User control

Users reshape any slot without editing plugin code. The system settings file
(see [`../settings/README.md`](../settings/README.md)) has a reserved
`slots` section:

```json
{
  "slots": {
    "@mesh/panel:right": [
      { "widget": "@mesh/battery-widget",  "order": 100 },
      { "widget": "@community/weather",    "order": 200, "props": { "units": "imperial" } },
      { "widget": "@mesh/volume-widget",   "order": 300 }
    ]
  }
}
```

When a user-defined entry exists for a slot, it **replaces** plugin
contributions for that slot entirely — predictable, no merge surprises. To
tweak one contribution while keeping the rest automatic, the user can edit
the generated settings UI, which rewrites the full list.

For incremental changes without replacing the whole slot, the same key
accepts a patch form:

```json
{
  "slots": {
    "@mesh/panel:right": {
      "disable": ["@community/weather"],
      "add":     [{ "widget": "@community/cpu-graph", "order": 150 }],
      "reorder": { "@mesh/battery-widget": 400 }
    }
  }
}
```

The array form and the patch form are mutually exclusive per slot.

## Resolution order

For a given slot, the core builds the contribution list in this order:

1. Plugin contributions declared via `[slot-contributions]`
2. Patch operations from the user's `slots.*.{disable,add,reorder}` (if patch form is used)
3. Full user replacement from `slots.*` as an array (if array form is used — skips step 1–2)
4. Sort by `order`, then by plugin ID (stable tiebreaker)
5. Enforce `max`; drop overflow with a diagnostic
6. If empty and `default` is set on the slot, render the default

Each step is visible in `mesh slots describe @mesh/panel:right`.

## Contribution visibility

Contributions are not silent. The core exposes them:

```
mesh slots list                      # every slot across every loaded surface
mesh slots describe @mesh/panel:right
mesh slots conflicts                 # overflow, rejected contributions, missing widgets
```

The diagnostics panel surfaces the same data so users can see *why* their
panel looks the way it does.

## Why this over "just ship your own panel"

Shipping a replacement panel means re-implementing the clock, the network
icon, the volume control, the battery indicator — and keeping up with their
updates. Slot contributions let a plugin author ship *only the new thing*
and leave the rest of the default surface alone. The user gets a coherent
panel from mixed sources, maintained by their respective authors.

Surface authors accept an API contract by declaring slots: the slot names,
`accepts`, and `max` are part of the surface's public interface. Changing
them is a breaking change and follows the same versioning rules as
interface contracts.
