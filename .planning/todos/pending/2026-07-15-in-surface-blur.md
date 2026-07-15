---
created: 2026-07-15T00:00:00.000Z
title: Real in-surface blur — element filter + backdrop-filter
area: rendering
related_phases:
  - v1.25-gpu-rendering
files:
  - crates/core/frontend/render/src/surface/painter/backend.rs
  - crates/core/frontend/render/src/surface/painter/tree.rs
  - crates/core/frontend/render/src/display_list.rs
  - crates/core/presentation/src/lib.rs
  - crates/core/ui/elements/src/style.rs
---

## Status

Phase 1 shipped 2026-07-15 (same-day session): retained + immediate
backdrop-filter execution, effectiveness-gated backdrop read regions,
blur-aware damage expansion at the shell effective-damage choke point,
pixel-parity test sparse-vs-full, child popup compositor blur regions
(`child_surface_blur_region`), frosted bubble-options/audio-popover styling.
Remaining: element `filter: blur()` subtree layers (needs layer push/pop
command kinds in the retained display list), Phase 2 bounded CPU
resample/cache if profiling demands it, Phase 3 GPU execution.

## Current state (verified 2026-07-15, pre-implementation)

Style parsing and typed storage are done: `filter` / `backdrop-filter`
parse into `VisualFilter` on `ComputedStyle`, `box-shadow` blur works
(Skia mask filter, capped at `MAX_EFFECT_BLUR_RADIUS = 96`). What's
missing is execution:

1. **Element `filter: blur()`**: layer-scoped blur works
   (`begin_layer` → `image_filters::blur`), but standalone
   `PainterCommand::ApplyFilter { Blur }` commands are a no-op with the
   diagnostic "standalone blur filter commands are deferred to layer
   migration" (`backend.rs:618-626`). Nodes with `filter` don't reliably
   lower into a layer push/pop around their subtree.
2. **In-surface `backdrop-filter: blur()`**: the Skia backend has a real
   implementation (`apply_backdrop_filter_impl`, saveLayer with backdrop
   image filter, `backend.rs:949`), but the retained tree path
   deliberately no-ops it (`push_backdrop_filter_command`,
   `tree.rs:947-962`, "CPU software blur removed per BLUR-03") and
   offloads to the `org_kde_kwin_blur` protocol — which only blurs
   content *behind the whole surface*, only on KWin. Glassmorphism
   against the surface's own content (popover over panel content,
   frosted quick-settings card) renders flat.

So "implement blur" = re-enable both paths with acceptable cost. BLUR-03
removed CPU backdrop blur because it was unbounded per-pixel work on the
software painter; that constraint is why this item is coupled to the GPU
backend plan (`2026-07-15-gpu-rendering-backend.md`).

## Plan

### Phase 1 — correct lowering into the retained display list

Backend-agnostic, no perf risk, do first:

- Lower `filter` on a node into `PushLayer(Blur)` … subtree …
  `PopLayer` in the retained command stream (the layer path already
  rasterizes blur correctly). Delete the deferred-standalone-ApplyFilter
  lane.
- Lower `backdrop-filter` into a retained `ApplyFilter { Backdrop }`
  command carrying the node's rounded-rect bounds + radius, ordered
  before the node's own background. The command exists; the retained
  path just never emits it.
- Damage semantics: a node with backdrop-filter must treat damage in its
  backdrop region (bounds + 3·radius pad, matching
  `apply_backdrop_filter_impl`) as its own damage; a blurred subtree
  inflates its damage bounds by the blur pad. Without this, sparse
  damage will show stale blur rings.
- Keep KDE protocol blur for behind-the-surface transparency (separate,
  complementary; region plumbing already shipped and dirty-gated).

### Phase 2 — bounded CPU execution (ship before GPU lands)

Make the software painter execute both, but capped so idle/steady-state
stays cheap:

- Downsample-blur-upsample: render the backdrop/subtree region at 1/4
  scale, blur with sigma/4, upscale with bilinear filtering. Visually
  indistinguishable for frosted-glass radii (≥8px) and ~16x cheaper.
- Respect the existing `MAX_EFFECT_BLUR_RADIUS` diagnostic; add a
  capability entry in `paint_backend_snapshot()` (`backdrop-blur:
  approximate` on raster).
- Cache: for a static backdrop under an animating foreground (the common
  popover case), key the blurred backdrop tile by (region, radius,
  underlying display-list generation of the covered span) and reuse it
  until covered content changes. This is the same generation machinery
  the child-popup paint cache uses.
- Measure with the v1.21 animation + pointer-move workloads with a
  frosted popover open; set a budget (e.g. blur ≤ 1ms/frame at 1080p
  steady state via the cache; only cache misses pay the resample).

### Phase 3 — GPU execution (rides the GPU backend plan)

- On `skia-gl`, the exact same commands run through Skia's GPU image
  filters — no lowering changes, full-quality single-pass blur, cache
  optional. This is the end state; Phase 2's approximation stays as the
  SHM-fallback behavior, reported via backend capabilities.

### Phase 4 — authoring surface

- Document `filter` / `backdrop-filter` in `docs/frontend/mesh-syntax.md`
  and the style-profile status table; add theme tokens for standard
  glass materials so built-in modules (quick-settings, popovers,
  launcher) adopt one consistent recipe.
- Transition/animation: `VisualFilter.blur_radius` should be
  interpolable (`mesh-core-animation` already interpolates box-shadow
  blur — mirror that), so hover/open transitions can animate blur.

## Risks / notes

- Blur breaks the "damage rect = changed pixels" invariant; the damage
  inflation in Phase 1 must land *with* the execution phases or sparse
  repaints will artifact. Test at fractional scale (the 1.5x edge-mapping
  path).
- Nested backdrop filters (blur inside blurred popover) — cap at one
  level initially, diagnostic beyond that.
- Child popup surfaces paint from their own retained lists; their
  backdrop is the parent surface's content, which is a *different
  buffer*. In-surface backdrop-filter inside a popup can only see the
  popup's own content — glass popovers over the bar need KDE protocol
  blur or a sampled-parent-region hand-off. Start with: popup backdrop
  sampling limited to popup-local content + KDE region for the rest;
  document the limitation.
