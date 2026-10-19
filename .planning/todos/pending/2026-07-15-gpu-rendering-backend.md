---
created: 2026-07-15T00:00:00.000Z
revised: 2026-10-19
title: GPU rendering backend (Skia-GL, dmabuf present) — remaining work
area: rendering
files:
  - crates/core/frontend/render/src/gpu.rs
  - crates/core/frontend/render/src/surface/canvas.rs
  - crates/core/presentation/src/wayland_surface/backend/gpu.rs
  - crates/core/presentation/src/wayland_surface/backend/present.rs
  - crates/core/shell/src/shell/runtime/render/mod.rs
---

## Goal

Paint retained display lists on the GPU per surface, with the SHM software
path as a runtime fallback. Blur, shadow and gradient effects stop costing CPU
per damaged pixel, and full-surface raster leaves animation-heavy frames.

## Shipped (2026-10-19)

Phases 1–4 and the rollout part of phase 6 have landed; see the 2026-10-19
entry in the [performance log](../../log/performance-log.md).

- **Skia Ganesh over GLES3.** The painter draws through a Skia `Canvas` on a
  `PixelCanvasSession`; the GPU target is the same session API.
- **The GPU paints on the UI thread, not the D6 scene thread.** Painter
  state (font database and aliases, glyph/icon/image caches, backdrop frame,
  tooltip hints, resource job queues) is thread-local on the UI thread. Moving
  paint to another thread needs SH-11's immutable `PaintContext` first. One
  EGL context stays current on the UI thread for the whole session.
- **Each surface keeps a retained GPU target** (`SurfaceCanvas::Gpu`), which
  plays the role `PixelBuffer` plays for SHM: partial repaints and scroll
  shifts keep working regardless of which ring buffer the compositor holds.
  Presentation copies the damage into one of a 2–3 deep ring of dmabuf
  buffers, each with its own pending damage, like the SHM slots.
- **No GBM.** Ring buffers are GL `RGBA8` textures exported with
  `EGL_MESA_image_dma_buf_export` (`DRM_FORMAT_ABGR8888`). Each export's
  format/modifier is checked against the compositor's dmabuf feedback table
  before `create_immed`, because importing an unadvertised pair is a protocol
  error.
- **Device selection.** The EGL display is opened with
  `EGL_EXT_platform_device` on the device whose DRM node matches the dmabuf
  feedback's main device. On this hybrid laptop Hyprland reports the AMD
  780M; the RTX 4060 is not used.
- **Synchronization without a CPU wait.** After the copy, an
  `EGL_ANDROID_native_fence_sync` sync file is imported into the dmabuf as an
  implicit write fence (`DMA_BUF_IOCTL_IMPORT_SYNC_FILE`, Linux 6.0+). The
  compositor waits for the GPU; the UI thread does not. Without fence support
  the frame falls back to a CPU wait (`glFinish`).
- **Rollout.** The CLI builds with `gpu` by default. GPU presentation turns on
  when the compositor offers `zwp_linux_dmabuf_v1` v4 feedback and the device
  exports importable buffers; otherwise one log line names the reason and
  surfaces paint on the CPU. `MESH_RENDER_GPU=0` forces the CPU. A dmabuf
  failure mid-session disables GPU presentation, and surfaces reallocate CPU
  canvases and repaint in full.
- **Debug overlays stay on the CPU.** The HUD, layout bounds and element
  highlight read and restore raw pixels, so while any of them (or the debug
  overlay) is on, surfaces reallocate CPU canvases. In-shell HUD timings
  therefore measure the CPU path.

## Remaining

### Resource residency (phase 5)

Glyphs share page images (performance log, "Shared glyph pages"). Icons,
images and icon-font glyphs still draw one texture each; Ganesh caches each
upload by image identity, so steady frames do not re-upload, but op combining
is quadratic in distinct textures. Move icons into pages if an icon-dense
surface shows `TextureOpImpl::onCombineIfPossible` in a profile.

### Gates (rest of phase 6)

D9 workload gates on both backends. The parity tests in `gpu::tests` cover
four small scenes; a canonical-workload pixel parity gate and a GPU frame-cost
gate (`gpu_frame_cost_workload` is the starting point) are still needed. Live
GPU presentation has been exercised only on Hyprland with an AMD main device.

### Scene thread

Moving GPU paint to the D6 render thread, after SH-11. The scene-thread
handoff and `SceneRenderer` trait exist; the per-surface retained target and
ring move with the paint.

## Measured

- GPU vs raster parity, production painter, 160×120 (RTX 4060): panel 2/19,200
  pixels off by more than 2 levels (worst 3); icon grid 0 (1); backdrop/filter
  blur and opacity layers 404 (4); scroll replay 0 (2).
- Frame cost (paint + copy to the presented buffer), 920×900 at scale 2:
  performance log, 2026-10-19 "GPU presentation".

## Risks / open questions

- Compositors whose main device cannot export or import `ABGR8888` with the
  driver's chosen modifier stay on SHM; this has not been seen yet.
- NVIDIA as the compositor's main device (proprietary driver, implicit sync
  through imported fences) is untested live.
- The dev-window (minifb) backend stays raster.
