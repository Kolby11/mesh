---
created: 2026-07-15T00:00:00.000Z
revised: 2026-10-19
title: GPU rendering backend (Skia-GL on the D6 render thread, dmabuf present)
area: rendering
files:
  - crates/core/frontend/render/src/gpu.rs
  - crates/core/frontend/render/src/scene_thread.rs
  - crates/core/frontend/render/src/surface/buffer.rs
  - crates/core/frontend/render/src/surface/painter/backend.rs
  - crates/core/presentation/src/wayland_surface/backend/present.rs
---

## Goal

Paint retained display lists on the GPU per surface, with the existing SHM
software path as a runtime fallback. Unlocks cheap blur/shadow/gradient
effects (currently CPU-per-damaged-pixel) and removes the full-surface raster
cost from animation-heavy frames.

## Decisions (revised 2026-10-19)

- **Skia Ganesh over GLES3, not wgpu/Vello.** The painter already draws
  through a Skia `Canvas`; Ganesh uses the same API, so the port is target
  acquisition, not paint semantics. wgpu cannot express damage-aware partial
  present. The Vello lane stays open behind the same painter contract.
- **The renderer runs on the D6 scene thread.** `GpuContext` is not `Send`
  and is created, used and dropped by the `SceneRenderer` worker.
- **Present dmabuf buffers from the UI thread, not `eglSwapBuffers`.** The
  original plan presented through `wl_egl_window` + `eglSwapBuffersWithDamage`.
  That attaches and commits the `wl_surface` from the GL thread, while the UI
  thread commits input/opaque/blur regions and window geometry on the same
  surface — two committers, breaking presentation ownership. Instead the
  renderer draws into GBM-allocated buffers (imported as EGLImage-backed
  FBOs) and returns them as `SceneRenderer::Output`; presentation attaches
  them through `zwp_linux_dmabuf_v1` in the same commit as region state.
  Buffer age is our own per-buffer damage bookkeeping (as SHM already does),
  so no `EGL_EXT_buffer_age` dependency.
- **Display selection.** Tests use `EGL_MESA_platform_surfaceless`. Production
  must render on the compositor's device: open the EGL display on the
  render node reported by `zwp_linux_dmabuf_v1` feedback (main device), so
  hybrid laptops (this dev box: AMD iGPU + NVIDIA dGPU) do not render on one
  GPU and scan out on the other.

## Phases

### Phase 1 — headless context, offscreen raster, readback (done 2026-10-19)

`mesh-core-render::gpu` behind the `renderer-gpu` feature: dynamically loaded
libEGL (no link-time GL dependency), surfaceless display, GLES3 context,
Skia `DirectContext`, BGRA premultiplied offscreen targets, synchronous
readback into `PixelBuffer`. Parity test against Skia raster. The nix shell
now provides libglvnd plus `/run/opengl-driver/lib`.

### Phase 2 — painter over a canvas target (done 2026-10-19)

`PixelCanvasSession` is the paint target: a `PixelBuffer` (raster) or any
Skia surface; `GpuTarget::session()` makes one on a GPU target. Its raw-buffer
escape hatch (`with_buffer`) is gone. Damage clears and in-place scroll
shifts are session methods: byte fast paths on raster, Src draws and a
snapshot self-copy elsewhere. The scrollbar fallback and the tooltip draw on
the session's canvas. The paint-backend trait's required method takes a
session. The icon blits and glyph blends were already test-only; they now draw
through the canvas, and `blend_pixel` plus the dead
`composite_blurred_buffer` are deleted. Backdrop blur is a backdrop
`save_layer` and needs no buffer reads. The display-list and region paint
entry points have `_in_session` variants that a scene-thread renderer calls.

Still raster-only, with no GPU path needed yet: the debug overlay's
capture/restore copies bytes, and the shell's SHM frame path still calls
`PixelBuffer::clear_rect`/`shift_region_pixels` before painting. Phase 3
renders through the session methods instead.

### Phase 3 — `SceneRenderer` implementation

A GPU `SceneRenderer` on the scene thread: per-surface buffer rings, damage
replay per buffer, `requires_full_replay` honored, surface removal retires
GPU resources on the owner thread. Readback output first, so it can drive the
existing SHM present path end to end before dmabuf exists.

### Phase 4 — dmabuf presentation

GBM allocation on the compositor's main device, `zwp_linux_dmabuf_v1`
(v4 feedback for device and format/modifier selection), explicit or implicit
sync fence before attach, buffer release tracking feeding the ring. SHM stays
the fallback per surface; the backend id (`skia-raster` / `skia-gl`) joins
`paint_backend_snapshot()`.

### Phase 5 — resource residency

Upload glyph atlases, images and SVG rasterizations once as GPU images keyed
by the existing cache keys; measure icon-grid and text-update workloads.

### Phase 6 — parity, gates and rollout

Pixel-parity per canonical workload, D9 gates on both backends, then
GPU-when-available by default with `MESH_RENDER_GPU=0` as the escape hatch.
Never fail startup when GL is missing; fall back with one diagnostic.

## Measured so far

- Skia prebuilt binaries with `gl` download for skia-safe 0.97; no source build.
- GPU vs raster parity on a rect/rounded-rect/gradient scene (96×64,
  NVIDIA RTX 4060, GLES3): fills and gradients identical; 64 antialiased
  rounded-curve pixels differ, worst by 41 levels (analytic vs supersampled
  coverage). Parity checks must budget curve edges, not demand exactness.

- Production painter, GPU target vs `PixelBuffer`, 160×120 at scale 1
  (debug build, RTX 4060, `MESH_REQUIRE_GPU=1`), as pixels off by more than 2
  levels and the worst delta: panel chrome, shadow, gradient and text 2/19,200
  (worst 3); 20-icon grid 0 (1); backdrop blur, filter blur and opacity layers
  404 (4); retained scroll replay with shifts and strip clears 0 (2). GPU
  replay equals a fresh GPU paint byte for byte.

## Risks / open questions

- skia-safe `gl` increases build time; keep it feature gated.
- NVIDIA GBM/dmabuf export behavior on driver 580; Mesa on the iGPU.
- Dev-window (minifb) backend stays raster.
- Whether readback-to-SHM (Phase 3) is ever a net win — likely not; it is a
  correctness bridge, not a shipping mode.
