# `@mesh/navigation-bar`

Top-edge navigation surface.

- **Type:** `surface`
- **Entrypoint:** `src/main.mesh`

## What it demonstrates

- A compact, theme-token-driven top navigation bar
- Displaying audio backend state through shell-injected frontend data (`audio.glyph`, `audio.label`, `audio.tooltip`)
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
<span class="volume-value">{audio.label}</span>
```

Dynamic values are embedded directly in element content using `{}`. The
runtime re-renders the text node when the variable changes.

### Dynamic attribute binding

```xml
<button title="{audio.tooltip}" aria-label="Open audio controls">
```

Any attribute can receive a dynamic value by wrapping the expression in `{}`.

### Event handlers

```xml
<button onclick={onVolumeClick}>
```

Standard HTML event attributes take a Luau function reference. No `@click=`
or `:on*=` syntax.

### Accessibility

```xml
<button
  title="{audio.tooltip}"
  aria-label="Open audio controls"
>
  <span aria-hidden="true">{audio.glyph}</span>
```

`title` provides a tooltip. `aria-label` provides the accessible name for
screen readers. Decorative glyphs use `aria-hidden="true"` so they are not
announced.

## See also

- [`docs/frontend/mesh-syntax.md`](../../../../frontend/mesh-syntax.md) — full `.mesh` syntax reference
