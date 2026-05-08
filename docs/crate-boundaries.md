# Core Crate Boundaries

This workspace keeps browser-like concerns split by crate:

- `mesh-core-frontend` compiles `.mesh` source and builds `WidgetNode` trees.
- `mesh-core-animation` owns easing, interpolation, transitions, and keyframes.
- `mesh-core-interaction` queries `WidgetNode` trees for hit testing, focus, tooltip, and scroll behavior.
- `mesh-core-render` paints `WidgetNode` trees into `PixelBuffer`s.
- `mesh-core-presentation` presents `PixelBuffer`s and normalizes surface input events.
- `mesh-core-surface-config` resolves manifest/settings surface layout policy.
- `mesh-core-frontend-host` owns frontend component host contract types.
- `mesh-core-shell` glues these crates to modules, services, theme, locale, diagnostics, and the event loop.

Normal dependency direction should remain:

```text
presentation -> render
render -> elements + icon
frontend -> component + elements + module + theme
frontend-host -> capability + elements + locale + render + theme + wayland
animation -> elements
interaction -> elements + module
surface-config -> module + wayland
shell -> all boundary crates as orchestration glue
```

Avoid adding dependencies from lower-level crates back into `mesh-core-shell`.
If a lower-level crate needs a shell concept, define a small contract type in
the appropriate boundary crate instead.
