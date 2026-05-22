# MESH Render Engine

`mesh-core-render` is the rendering crate. It owns retained render objects,
display-list construction, damage selection, pixel buffers, text measurement,
icon/glyph rasterization, and debug overlay painting.

The painter boundary is intentionally Skia-centric. MESH keeps the render-engine
responsibilities that depend on its widget tree and runtime model, while Skia
owns low-level paint/raster work such as antialiasing, paths, rounded rects,
strokes, shadows, blur/image filters, blend modes, clipping, layers/saveLayer,
gradients/images, and future text primitives where adopting them makes sense.
Skia is the paint backend, not the render engine.

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
- `mesh-core-render` resolves retained render objects and display-list commands
  into backend-neutral paint operations.
- the active paint backend, currently Skia, rasterizes those operations into
  `PixelBuffer`s.
- `mesh-core-presentation` presents `PixelBuffer`s through dev-window or
  layer-shell backends and normalizes input events.
- `mesh-core-shell` glues runtime events, service state, surface configuration,
  frontend output, rendered frames, and presentation events together.

The paint command shape is a small browser/Qt display list rather than ad hoc
helper calls:

- `PushClip`
- `PopClip`
- `PushLayer`
- `PopLayer`
- `DrawRect`
- `DrawRoundedRect`
- `DrawPath`
- `DrawText`
- `DrawImage`
- `DrawShadow`
- `ApplyFilter`

That keeps Skia authoritative for graphics behavior today and leaves room for a
future Vello backend to implement the same painter contract.

`PaintBackend` implementations must expose `PainterBackendCapabilities` and
must diagnose unsupported or deferred commands through `PainterDiagnostic`
rather than silently dropping visual behavior. Retained display-list data and
public render-object structures must stay backend-neutral: they may describe
MESH visual intent, but Skia-specific types such as `Canvas`, `Paint`, `Path`,
`RRect`, `ImageFilter`, and `SaveLayerRec` belong inside the Skia backend.

Keep new render-specific code in this crate unless it is frontend compile/lower
logic (`mesh-core-frontend`), source parsing (`mesh-core-component`), a
runtime-inspectable element/style/layout contract (`mesh-core-elements`), or a
surface/window backend (`mesh-core-presentation`).
