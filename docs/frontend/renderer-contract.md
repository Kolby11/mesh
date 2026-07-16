# `.mesh` Renderer Contract

## Author-facing boundary

Authors use `.mesh` templates, Luau scripts, scoped CSS-like styles, semantic
theme/icon names, service interfaces, and explicit component imports. Renderer
crate types and backend-specific paint objects are not public module APIs.

## Stable responsibilities

| Area | Public contract |
| --- | --- |
| Layout | MESH elements and the bounded CSS profile define row, column, stack, fixed/content sizing, spacing, and positioning semantics |
| State | Components consume typed service state, methods, and events through interface proxies |
| Styling | Theme tokens, local custom properties, and supported CSS declarations remain backend-neutral |
| Text | Localization and text-selection behavior remain MESH contracts independent of the painter |
| Input | Documented pointer, keyboard, focus, touch, and gesture handlers route through MESH elements |
| Surfaces | MESH owns Wayland surface creation, placement, show/hide behavior, and presentation |
| Accessibility | Authors provide semantic roles, names, states, and focus metadata; core adapters build platform-facing data |
| Diagnostics | Unsupported style and painter behavior must remain observable rather than silently disappearing |

## Internal ownership

Taffy currently performs retained layout. MESH owns retained node identity,
style resolution, render-object synchronization, display-list ordering, damage,
and profiling. Skia currently executes low-level painter commands. Presentation
and Wayland surface commits remain outside the painter.

These are implementation choices, not module dependencies. A future backend
must implement the same backend-neutral painter contract and preserve visible
behavior before becoming authoritative.

## Not promised

- MESH `.mesh` files are not arbitrary HTML or browser DOM applications.
- Full browser CSS compatibility is not a goal.
- Skia, Taffy, Parley, AccessKit, Vello, and Blitz types are not author APIs.
- A renderer migration does not implicitly change module, service, settings,
  or profile contracts.

See [CSS coverage](../css-coverage.md) and
[renderer ownership](../renderer-ownership.md).
