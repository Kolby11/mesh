# Renderer Ownership

This document records current source ownership. Historical renderer adoption
research and migration gates are retained in
[`.planning/log/renderer-migration-2026-05-23.md`](../.planning/log/renderer-migration-2026-05-23.md).

## Authoritative boundaries

| Boundary | Source | Responsibility |
| --- | --- | --- |
| Component parser | `crates/core/ui/component/` | Parses `.mesh` template, script, and style blocks |
| Frontend compiler | `crates/core/frontend/compiler/` | Resolves imports and builds runtime widget trees |
| Element/style/layout model | `crates/core/ui/elements/` | Owns `WidgetNode`, computed style, retained Taffy layout, and semantic state |
| Runtime identity | `crates/core/shell/src/shell/component/runtime_tree.rs` | Preserves node identity and dirty categories across updates |
| Render objects and display list | `crates/core/frontend/render/src/render_object.rs` and `display_list.rs` | Synchronizes retained paint data, ordering, selection, and damage |
| Painter contract | `crates/core/frontend/render/src/surface/painter.rs` | Lowers backend-neutral commands and records diagnostics/profiling |
| Skia backend | `crates/core/frontend/render/src/surface/painter/backend.rs` | Executes low-level raster, path, image, gradient, layer, and effect operations |
| Presentation | `crates/core/presentation/` | Owns presented buffers and normalized surface input |
| Wayland | `crates/core/platform/wayland/` and presentation Wayland modules | Owns compositor protocol integration and surface commits |

## Scene preparation and handoff

The UI thread resolves an explicit per-frame `PaintContext` and prepares a
complete Skia recording through `PixelCanvasSession`. `PreparedScene` pins the
immutable picture and its raster images, icon variants, glyph pages, shaders,
backdrop and tooltip inputs. Text shaping uses the same UI font database as
layout; the worker receives prepared glyph images and never reopens font or
icon files or consults the live resource registry. This avoids copying the
entire installed font catalog into each scene. `FrozenFontDatabase` remains
available for consumers that need owned font sources rather than prepared glyphs.

`render::scene_thread` admits at most 64 surface incarnations in production,
with one pending scene and one completed output per surface and one active
render. Commits coalesce, and only current outputs may be attached. Missing
presentation acknowledgement, skipped commits, extent/scale/resource changes
and new targets force full replay. Removed canvas incarnations retire on the
worker. Queue waits use a condition variable.

## GPU and presentation ownership

`render::gpu::scene` creates, uses and destroys EGL, Ganesh, retained surface
pixels and exported dmabuf textures on the render thread, on the compositor's
DRM device. A shared 512 MiB admission budget bounds those targets; each surface
has at most three export slots. Partial frames replay the complete immutable
picture under disjoint device damage clips. Each export slot accumulates damage
while unavailable, then receives the corresponding copy from retained pixels.

Scene recordings also carry their deduplicated raster-image identities. The
worker retains up to 2,048 identities / 64 MiB (conservatively charged at four
bytes per pixel) and gives Ganesh that much cache headroom above admitted target
bytes. Ganesh owns the actual upload cache and evicts under pressure; textures
are reused by immutable image identity rather than recreated each frame.

On the scene path, icons, icon-font masks and images up to 128×128 pixels share
1024-wide RGBA or A8 atlas pages. The atlas uses the existing bounded glyph-page
allocator (two color pages and four mask pages) and a 16 MiB source-identity
cache. Pages refresh once at the next scene boundary; new cells use their own
image until that snapshot exists. Strict source rectangles prevent sampling
neighbors. Full atlases keep existing cells and draw overflow assets from their
own images. Growth and resource-revision resets leave submitted snapshots immutable. Larger images retain dedicated image identities. CPU paint keeps its
original raster images; text retains its existing glyph pages.

Only duplicated dmabuf descriptors, dimensions, damage and a release lease cross
to Wayland. Presentation checks the format/modifier before import, attaches the
buffer and acknowledges its scene only after a successful commit. A
`wl_buffer.release` returns its lease; dropping an output never calls GL. Leases
carry independent identities, so an old release cannot unlock a reused texture.
GPU errors switch subsequent canvases to SHM with a full repaint. The CLI enables
GPU presentation by default when compositor device feedback and export support
are available; `MESH_RENDER_GPU=0` selects SHM. Debug overlays and the dev-window
backend use CPU pixels. Hyprland presentation has been checked live with the AMD
780M as the main device; NVIDIA export and offscreen rendering are tested, but
NVIDIA as compositor main device and other compositor imports remain live checks.

CPU frames split the same way across surfaces. When two or more components want
to render, each parent `RasterCanvas` records its damage-clipped paint into a
Skia picture during the serial, VM-bound pass; the shell then replays every
surface's pictures into its own `PixelBuffer` on the rayon pool before any
present. Pictures pin their resources, so replay needs no painter thread-locals.
Any pixel read — scroll shifts, debug overlays, the SHM copy — replays pending
pictures first. Child popup surfaces still paint and present inside reconcile,
and the SHM copy stays on the shell thread.

The initial production integration waits for worker completion at the
presentation boundary. GPU execution has moved threads; scripts for frame N+1
are not yet overlapped with frame N raster. The worker sleeps when idle.
Scroll layer caching and blur caches remain measured follow-ups in the backlog.

## Adapter boundaries

`library_adapters.rs`, `parley_adapter.rs`, `accesskit_adapter.rs`,
`anyrender_adapter.rs`, and proof snapshots are internal integration seams.
They may change without modifying `.mesh` authoring contracts.

## Replacement rule

A replacement renderer or adapter must preserve or deliberately replace:

- stable retained identity;
- typed invalidation and damage;
- layout and text-selection behavior;
- backend-neutral display-list semantics;
- diagnostics and profiling;
- input and accessibility mappings;
- Wayland presentation ownership.

Backend-specific types must not leak into element style, display-list, module,
or public author APIs. The stable author contract is documented in
[the renderer contract](frontend/renderer-contract.md).
