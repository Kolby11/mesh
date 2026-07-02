# `@mesh/text-selection-proof`

Passive proof surface for selectable shell copy.

- **Type:** `surface`
- **Entrypoint:** `src/main.mesh`

## What it demonstrates

- Opt-in text selection with `selectable="true"` on a `text` node
- A deliberately bounded proof of passive shell copy, not a general rich-text
  editor or document surface
- Theme-token-driven shell styling on a minimal read-only surface
- Surface placement and keyboard policy through the standard `surface` schema

## Behavior

The shipped template keeps selection scoped to one wrapped `text` node and
leaves the surrounding frame non-selectable on purpose. That makes the current
contract explicit:

- authors opt text selection in node by node
- selection is local to the marked text node
- the surface demonstrates read-only copy behavior, not text editing

If you want live keyboard copy testing for this surface in a shell session, set
its `surface.keyboard_mode` to `on_demand` or `exclusive` in settings so the
surface can receive `Ctrl+C`.

## Settings

The module schema exposes the standard surface settings:

```json
{
  "surface": {
    "anchor": "right",
    "layer": "top",
    "width": 360,
    "height": 176,
    "exclusive_zone": 0,
    "keyboard_mode": "none",
    "visible_on_start": true
  }
}
```

`keyboard_mode: "none"` keeps the proof surface passive by default. Switch to
`on_demand` when you want pointer engagement to request keyboard focus for copy
testing.

## See also

- [`docs/frontend/mesh-syntax.md`](../../../frontend/mesh-syntax.md) — `.mesh`
  syntax, keyboard focus, and selectable text
- [`docs/spec/08-settings.md`](../../../spec/08-settings.md) — shell settings
  layers and keyboard overrides
