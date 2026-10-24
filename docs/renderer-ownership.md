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

Each scene canvas retains node pictures by immutable `DisplayPaintNode` identity,
scale, effective clip and fallback module. Unchanged nodes contribute picture
references to the complete scene without repeating drawing/resource preparation.
Recorded drawables freeze their records at scene finalization, retaining small
node drawings instead of allowing Skia to inline them into the parent recording.
The cache admits at most 4,096 nodes and 16 MiB of picture storage plus uniquely
counted raster images (charged at four bytes per pixel); insertion evicts oldest
entries. Scale, clip and owner changes miss the cache. Resource revisions,
completed async assets, newly published asset pages and font database/alias
changes invalidate its entries. Recordings made with pending resource jobs are
not admitted.

Promoted buffer bounds clip descendant visual extents at overflow viewports,
keeping scroll offsets from changing the popup reserve. Outer shadows remain
included, and fixed descendants keep their surface coordinate space.

Icon content applies its resolved node clip to file, glyph and fallback drawing,
including when hovered transforms move a scroll subtree onto the affine path.

Layer scopes, backdrop reads/refraction, scroll scopes and scroll-content nodes
are prepared live. Tooltip paint remains outside the node cache. Complete-scene
composition still visits command references; newly prepared node operations
scale with changed retained payloads within the cache budget, rather than the
whole scene. Submitted scenes own their pictures and images independently of
cache eviction, resource changes and canvas retirement. `preparation_metrics()`
separates recorded nodes/operations, reused nodes and live commands.

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
When all scene export slots are leased, the worker retains that commit's
immutable frame and raster target. Presentation waits for a native dmabuf
release and retries export/copy with the same damage, without another scene
recording or raster. The frame stays in flight; the shell receives no failed
completion for this transient condition. Parked frames and completed children
waiting for their parent do not consume the worker timeout. Newer submissions,
removal and shutdown discard stale retries; other surfaces remain runnable.
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

A ready child target can advance its component VM and paint even while its
parent waits for a compositor frame callback or buffer release. Each target
retains its own presentation backpressure and pending damage.

Presentation does not wait for the worker. `present_with_damage` submits the
immutable scene and returns `PresentStatus::Submitted`; the surface then reports
`surface_waiting_for_buffer_release`, so the shell skips it while its frame is
in flight but keeps running scripts, layout and other surfaces. The worker
signals an eventfd that `wait_for_events` polls beside the Wayland connection
and the shell wake. On that wake the backend takes the published output (whose
fence the worker already waited on), imports the descriptors, attaches and
commits, and only then acknowledges the scene. Frames commit in submission
order, and a popup never commits before a parent whose frame is still pending.
A frame that does not reach the compositor (superseded or removed canvas,
recreated compositor object, hidden surface, busy buffers, GPU error) is
reported through `take_scene_completions`; the shell repaints the whole surface.
A worker silent for two seconds disables GPU presentation. The worker sleeps
when idle. Scroll layer caching and blur caches remain follow-ups in the backlog.

## Render path selection

The first canvas request resolves the render path once and logs one `render path:`
line at info level naming the active path and why. GPU needs a compositor with
`zwp_linux_dmabuf_v1` v4 feedback naming a main device, an EGL/Ganesh context and
dmabuf export on that device, and a format/modifier the compositor imports; any
failed gate selects CPU/SHM with the reason (also `MESH_RENDER_GPU=0`). A GPU
error at runtime disables the path and logs the cause; surfaces then paint on the
CPU. `PresentationEngine::render_path` returns the same text.

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
