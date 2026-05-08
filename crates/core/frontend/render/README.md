# MESH Render Engine

`mesh-core-render` is the software rendering crate. It owns pixel buffers,
surface painting, icon/glyph rasterization, text measurement, and debug overlay
painting.

This crate is deliberately outside `crates/core/runtime`. Runtime crates host
scripts and backend services; they should not depend on software painting or
glyph caches.

It lives under `crates/core/frontend/render` because it is part of the frontend
pipeline, but it remains a separate crate from `mesh-core-frontend` so compiler
code does not pull in text shaping, image decoding, or pixel-buffer painting.

The intended architecture mirrors a browser split:

- `mesh-core-component` parses author-facing `.mesh` source.
- `mesh-core-frontend` compiles and lowers frontend source into widget trees.
- `mesh-core-elements` exposes the retained widget/tree/style/layout API that
  runtime and shell code can inspect, similar to a small DOM-facing surface.
- `mesh-core-render` paints widget trees into `PixelBuffer`s.
- `mesh-core-presentation` presents `PixelBuffer`s through dev-window or
  layer-shell backends and normalizes input events.
- `mesh-core-shell` glues runtime events, service state, surface configuration,
  frontend output, rendered frames, and presentation events together.

Keep new render-specific code in this crate unless it is frontend compile/lower
logic (`mesh-core-frontend`), source parsing (`mesh-core-component`), a
runtime-inspectable element/style/layout contract (`mesh-core-elements`), or a
surface/window backend (`mesh-core-presentation`).
