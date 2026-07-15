---
created: 2026-07-15T00:00:00.000Z
title: GPU rendering backend (Skia-GL first, wgpu/Vello later) — v1.25
area: rendering
related_phases:
  - v1.25-gpu-rendering
files:
  - crates/core/frontend/render/src/surface/painter/backend.rs
  - crates/core/frontend/render/src/surface/painter.rs
  - crates/core/frontend/render/src/display_list.rs
  - crates/core/presentation/src/lib.rs
  - crates/core/presentation/src/wayland_surface/backend.rs
  - crates/core/frontend/render/Cargo.toml
---

## Goal

Paint retained display lists on the GPU per surface, with the existing SHM
software path as a runtime fallback. Unlocks cheap blur/shadow/gradient
effects (currently CPU-per-damaged-pixel) and removes the full-surface raster
cost from animation-heavy frames.

## Why Skia-GL first, not wgpu/Vello first

- The authoritative paint backend is already `SkiaPaintBackend`
  (`painter/backend.rs`) drawing through a Skia `Canvas`. Skia's GPU backend
  (Ganesh, `skia-safe` `gl` feature) uses the **same Canvas API**, so every
  painter command ports with pixel parity — the diff is context/surface
  creation, not paint semantics.
- EGL on Wayland gives partial present via `EGL_EXT_buffer_age` +
  `EGL_KHR_swap_buffers_with_damage`, so the shipped damage pipeline
  (per-buffer damage, fractional-scale edge mapping) carries over. wgpu today
  cannot express buffer-age partial present — going wgpu-first would regress
  the just-completed damage work to full-frame presents.
- The painter command API is already backend-neutral by contract
  (`docs/renderer-ownership.md` forbids skia_safe types in retained data),
  and the `renderer-anyrender` / `renderer-vello-encoding` feature scaffolds
  stay as the long-term replacement-candidate lane. Nothing in this plan
  closes the Vello door; it defers it until wgpu can do damage-aware present.

## Prerequisite check (all shipped as of 2026-07-15)

- Retained display list + damage rects: shipped (display_list.rs).
- Retained Taffy layout: shipped 2026-07-15.
- Fractional-scale partial damage with physical edge mapping: shipped
  2026-07-15.
- Child popups replay retained command streams: shipped 2026-07-15.

## Phases

### Phase 1 — presentation: GPU surface plumbing behind a backend enum

- Add a `PresentationBufferKind { Shm, Egl }` decision per surface in
  `wayland_surface/backend.rs`. EGL path: `wayland-egl` (`wl_egl_window`)
  + `khronos-egl` context bound to the compositor's `wl_display`.
- One shared EGL context/display; per-surface `EGLSurface` sized in
  physical pixels (existing fractional-scale viewport logic reused —
  `wp_viewport` still maps physical buffer → logical size).
- Selection: config/env gate (`MESH_RENDER_GPU=1` initially), automatic
  fallback to SHM on EGL init failure with a one-shot diagnostic. Never
  fail startup because GL is missing (nix env, llvmpipe boxes).
- No painting change yet: prove context creation, resize, scale change,
  and teardown against both dev-window and layer-shell backends.

### Phase 2 — render: SkiaPaintBackend over a GPU surface

- Enable `skia-safe` `gl` feature (behind a cargo feature `renderer-gpu`
  so CI without GL headers still builds).
- Generalize the backend's surface acquisition: today it wraps the
  `PixelBuffer` as a raster surface; add a `gpu::DirectContext` +
  `backend_render_targets::make_gl` path targeting the EGL FBO.
- Paint commands are untouched — same Canvas calls. Flush + submit at end
  of paint instead of memcpy into SHM.
- Keep `PixelBuffer` as the retained/compare copy only where consumed
  (tests, debug snapshots, damage diffing that reads pixels). Add an
  explicit GPU→CPU readback used only by tests/debug, never per frame.
- Extend `paint_backend_snapshot()` with backend id (`skia-raster` /
  `skia-gl`), and route GL errors into the existing unsupported-feature
  diagnostics + rollback authority.

### Phase 3 — damage-aware present

- Query `EGL_EXT_buffer_age`; keep a ring of recent frame-damage sets per
  surface (the SHM per-buffer damage bookkeeping already models this —
  reuse its accumulation logic).
- Repaint the union of damage for the returned age, present with
  `eglSwapBuffersWithDamage`. Age 0 / unsupported ⇒ full repaint (still
  correct, just slower).
- Scissor/clip the GPU paint to the damage union exactly like the raster
  painter clips today (physical-edge mapping already shipped).

### Phase 4 — resource residency

- Glyph atlas / image / SVG rasterization currently produce CPU pixmaps
  per paint; upload once and cache as `skia_safe::Image` textures keyed
  by the existing icon/image cache keys. Measure with the v1.21 icon-grid
  and text-update workloads before/after.

### Phase 5 — parity + rollout

- Pixel-parity harness: render each canonical v1.21 workload on raster
  and GL, compare with small tolerance; run in CI where GL (llvmpipe) is
  available in the nix shell.
- Flip default to GPU-when-available once the eight canonical workloads
  show no regressions and popups/fractional scale pass; keep
  `MESH_RENDER_GPU=0` as the escape hatch.

## Risks / open questions

- nix dev shell needs EGL/GL headers + llvmpipe for tests (same class of
  problem as the solved xkbcommon one).
- skia-safe with `gl` significantly increases build time; keep it feature
  gated.
- Shared context across surfaces vs per-surface contexts: start shared
  (one GL thread — the shell already paints serially); revisit when the
  K-phase parallel paint work lands (GPU submission likely stays on one
  thread anyway).
- Dev-window (winit) backend: either keep it raster or add a GL swapchain
  there too; raster-only dev-window is acceptable initially.
- wgpu/Vello migration remains a replacement candidate behind the same
  painter contract; revisit when wgpu exposes damage-aware present or
  when we accept full-frame presents on GPU.
