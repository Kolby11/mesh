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

Shell visual damage comes from the retained display list's world bounds,
including ancestor transforms, scrolling and clips. Changed nodes contribute
both previous and current bounds so moved pixels are repainted.
Rounded clipping coverage and complete gradient/shadow rasters are prepared
independently of destination damage. The painter retains their bounded geometry caches;
scroll and partial-repaint paths use the same coverage as a full paint.
Painting runs through a paint session over CPU pixels or an immutable scene
recording. The UI prepares glyph/icon/image resources using the same font state
as layout. GPU execution and retained textures belong to the render thread;
Wayland attaches descriptor-only outputs. Partial damage and full replay share
the same recorded scene. Small scene images and icon bitmaps share immutable
atlas pages; the worker retains image identities within a bounded cache and
allows GPU cache headroom beside retained targets. CPU scroll shifts remain a raster fallback mechanism.

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
