# `@mesh/navigation-bar`

Top-edge navigation surface.

- **Type:** `surface`
- **Entrypoint:** `src/main.mesh`

## What it demonstrates

- A compact, theme-token-driven top navigation bar
- Deriving audio display state inside the frontend from raw `audio.*` service
  bindings
- Displaying battery state from `mesh.power` through raw `power.*` service
  bindings and frontend-local formatting
- Text interpolation with `{variable}` syntax
- Event handling with `onclick={handler}`
- Dynamic attribute binding with `title="{expr}"`
- Surface placement through `config/settings.json`
- Container-query adaptation for narrower widths

## Default behavior

The shell discovers this as its own top-level frontend surface, and the
default settings pin it to the top edge with an exclusive zone so it behaves
like normal shell chrome.

## Syntax patterns used

### Text interpolation

```xml
<span class="meta-pill-text">{active}</span>
<span class="battery-value">{battery_label}</span>
```

Dynamic values are embedded directly in element content using `{}`. The
runtime re-renders the text node when the variable changes.

### Dynamic attribute binding

```xml
<div title="{battery_tooltip}" aria-label="{battery_aria_label}">
```

Any attribute can receive a dynamic value by wrapping the expression in `{}`.
The battery widget uses the same mechanism for its hover bubble.

### Event handlers

```xml
<button onclick={onVolumeClick}>
```

Standard HTML event attributes take a Luau function reference. No `@click=`
or `:on*=` syntax.

The callback receives an event object, so the navigation bar can position
quick-settings and volume surfaces explicitly from
`event.current_target.position`.

### Accessibility

```xml
<div
  title="{battery_tooltip}"
  aria-label="{battery_aria_label}"
>
  <span aria-hidden="true">{battery_icon_name}</span>
</div>
```

`title` provides a tooltip. `aria-label` provides the accessible name for
screen readers. Decorative glyphs use `aria-hidden="true"` so they are not
announced.

## See also

- [`docs/frontend/mesh-syntax.md`](../../../../frontend/mesh-syntax.md) — full `.mesh` syntax reference
