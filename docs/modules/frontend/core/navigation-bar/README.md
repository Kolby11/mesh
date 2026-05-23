# `@mesh/navigation-bar`

Top-edge navigation frontend module.

- **Type:** `frontend module`
- **Manifest:** `module.json`
- **Entrypoint:** `src/main.mesh`

## What it demonstrates

- A compact, theme-token-driven top navigation bar
- Deriving audio display state inside the frontend from the `mesh.audio@>=1.0`
  interface contract
- Displaying battery state from `mesh.power` through raw `power.*` service
  bindings and frontend-local formatting
- Text interpolation with `{variable}` syntax
- Event handling with `onclick={handler}`
- Dynamic attribute binding with `title="{expr}"`
- Surface placement through `config/settings.json`
- Container-query adaptation for narrower widths
- Real keyboard traversal and default button activation on shipped shell chrome
- A focused-surface shortcut example: `mesh.keybinds.mute` defaults to `m` and
  toggles mute only while the navigation bar owns keyboard focus
- Author-facing `:focus-visible` styling on the shipped controls

## Default behavior

The shell discovers this from `module.json` as a frontend module with a layout
contribution. The default settings pin it to the top edge with an exclusive
zone so it behaves like normal shell chrome. The bundled settings also switch the surface to
`keyboard_mode: "on_demand"` so keyboard focus can move through the settings,
volume, and theme controls without turning the surface into an always-capturing
keyboard sink.

The volume control imports `mesh.audio@>=1.0` through `pcall(require, ...)`.
It never imports a backend provider module ID such as `@mesh/pipewire-audio`;
the root graph chooses the active provider for the interface contract.

## Syntax patterns used

### Text interpolation

```xml
<text class="meta-pill-text">{active}</text>
<text class="battery-value">{battery_label}</text>
```

Dynamic values are embedded directly in element content using `{}`. The
runtime re-renders the text node when the variable changes.

### Dynamic attribute binding

```xml
<box title="{battery_tooltip}" aria-label="{battery_aria_label}">
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

### Keyboard behavior

The shipped controls rely on shell-owned keyboard defaults:

- `Tab` / `Shift+Tab` traverse the settings, volume, and theme buttons in visual order
- `Enter` and `Space` activate the focused buttons
- The focused-surface shortcut `m` calls the navigation bar's mute handler,
  advertises the `mesh.keybinds.mute` action on the volume control metadata,
  and can be remapped through the shell-level
  `keyboard.surface_shortcuts["@mesh/navigation-bar"]` override
- The resolved binding is published to the control's accessibility
  `keyboard_shortcut` metadata and to `mesh.debug.keybinds`; invalid or
  conflicting keybind data appears as non-fatal component diagnostics.

Use `:focus-visible` when styling the strong keyboard ring or highlight. Use `:focus` for broader logical-focus styling.

### Accessibility

```xml
<box
  title="{battery_tooltip}"
  aria-label="{battery_aria_label}"
>
  <text aria-hidden="true">{battery_icon_name}</text>
</box>
```

`title` provides a tooltip. `aria-label` provides the accessible name for
screen readers. Decorative glyphs use `aria-hidden="true"` so they are not
announced.

## See also

- [`docs/frontend/mesh-syntax.md`](../../../frontend/mesh-syntax.md) — full `.mesh` syntax reference
