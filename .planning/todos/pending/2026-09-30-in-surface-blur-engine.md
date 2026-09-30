# In-surface blur engine: cached backdrop and filter layers

**Status:** plan, 2026-09-30. Companion to the compositor backdrop blur work
(landed 2026-09-30, see the performance log), which moved blur of the
*desktop* to the compositor through `ext-background-effect-v1`. This plan covers blur of MESH's *own*
content inside a surface — a frosted panel over other widgets, a blurred card,
a modal scrim — so components can use it freely instead of avoiding it.

## Goal

Component authors write ordinary CSS — `backdrop-filter: blur()` and
`filter: blur()` — anywhere in a component, and get:

- correct results over in-surface content, over the desktop, and over both;
- a cost paid when the blurred *input* changes, not on every repaint near it;
- the same code path on today's software painter and on the planned GPU painter.

No new syntax. The engine decides where each blur runs and caches what it can.

## How toolkits and browsers do it

- **Qt Quick** renders the content under a frosted item into an offscreen
  texture (`ShaderEffectSource`), blurs that texture in a shader
  (`MultiEffect`, downsampled multi-level blur), and draws it under the item.
  `live: false` or an unchanged source reuses the blurred texture.
- **GTK 4** turns CSS `filter: blur()` into a `GskBlurNode` drawn by the GL/Vulkan
  renderer. It has no `backdrop-filter`.
- **Chromium** implements `backdrop-filter` with a *backdrop root*: the backdrop
  is everything painted before the element up to the nearest ancestor that
  isolates (filter, opacity < 1, mask, …). It is read into a GPU texture,
  downsampled and blurred, edges mirrored, then composited under the element.
- **Behind-the-window blur** is never done by the toolkit; it is always a
  compositor region (companion work).

The common shape is: **snapshot the input once → blur it into an offscreen →
cache the result keyed by what went into it → composite the cached image on
every repaint.** Downsampling is how they make the blur itself cheap, and that
only pays on the GPU: on MESH's software painter a downsample–blur–upsample
chain was measured slower at every radius (rejected experiment 2026-07-30), and
the same log entry showed the offscreen layer, not the kernel, dominating cost.
So on the CPU the win has to come from **not recomputing**, and on the GPU from
the blur being cheap. One design serves both.

## Where MESH is today

- `filter: blur()` (04 §9, shipped): the subtree paints into an offscreen layer,
  is blurred, and composited. Nesting is capped at four and radius at
  `shell.render.blur.max_radius`. A change inside the layer re-rasterizes and
  re-blurs the whole layer, on every such frame.
- `backdrop-filter`: executed in-surface with a Skia `save_layer` backdrop
  (`apply_backdrop_filter_impl`) whenever no compositor blur protocol is bound.
  It re-blurs on every repaint that touches the element, including repaints
  caused only by the element's own children (hover inside a frosted panel).
- The spec only covers the fallback in outline: 04 §10 (2026-09-30) says
  in-surface blur happens with no protocol and skips empty backdrops; the
  backdrop root, the MESH/compositor split, and edge clamping are unspecified.
- Damage already knows the backdrop: `compute_backdrop_regions`
  (`display_list/blur.rs`) finds whether anything is painted beneath a backdrop
  node, and `expand_damage_for_blur_regions` grows damage by the kernel reach.
  The painter does not use the "nothing beneath" answer.
- **Suspected defect, verify first:** on a partial repaint, the backdrop layer
  reads only inside the damage clip with `TileMode::Decal`, so pixels near the
  clip edge blur against transparency instead of the real backdrop. The
  navigation bar hides this because its backdrop is empty. A pixel test that
  compares a partial repaint with a full repaint over a non-empty backdrop
  settles it.

## Design

### 1. One contract in the spec (04 §9–§10)

- **Backdrop root.** A `backdrop-filter` reads everything painted before the
  element within its backdrop root: the nearest ancestor that paints through an
  offscreen (`filter`, `opacity < 1`, blend isolation, a compositing layer), or
  the surface root. Content outside the root is not visible to it.
- **Split backdrop.** At the surface root, what lies beneath is in-surface
  content over the desktop. MESH blurs the in-surface part itself; the desktop
  part is the compositor's, through the region the compositor backend sends. Over a
  fully transparent backdrop only the compositor region is used. (Blurring the
  two separately is an approximation — blur does not commute with alpha
  compositing — and the best a client can do without reading the screen.)
- **Edges** sample by clamping at the backdrop root's bounds, as browsers do,
  instead of `Decal`, which fades toward transparent at the edges.
- `filter: blur()` keeps its current layer semantics and caps.

### 2. Route per element, not per shell

The display-list frame plan classifies each backdrop node from data it already
computes:

| backdrop beneath the element | action |
| --- | --- |
| nothing in-surface | compositor region only; no pixel work |
| in-surface content | in-surface blurred backdrop (cached, below) + compositor region where the content is translucent |
| no compositor protocol | in-surface only; transparent parts stay transparent |

This replaces the global `BackdropBlurPolicy` switch, which today is either
right for the navigation bar or right for a frosted panel, never both.

### 3. Blurred-backdrop cache

For each in-surface backdrop node, keep the blurred backdrop of its whole read
region (element box inflated by the kernel reach, clipped to the backdrop root):

- **Key:** the retained render signatures of the commands beneath that intersect
  the read region (the display list already keeps per-command signatures), the
  region in device pixels, sigma, passes, scale, and resource revision.
- **Hit:** composite the cached image under the element, clipped to damage. A
  hover transition inside a frosted panel — the common case — becomes a blit.
- **Miss:** render just the commands beneath into an offscreen of the read
  region, blur it once, store it. This snapshot is complete even on a partial
  repaint, which also fixes the suspected edge defect.
- **Damage:** the existing kernel-reach expansion stays for backdrop *changes*;
  a change that is only *above* the backdrop (the element or its children) no
  longer needs the expansion, since the cached backdrop is unaffected.

### 4. Filter-layer output cache

Same idea for `filter: blur()`: cache the blurred layer keyed by the subtree's
retained signatures, sigma, passes, and scale. A static blurred card over
changing content is composited from cache; the card changing re-blurs as today.
A dismissing card animating its blur radius misses every frame and costs what
it does now, which is the transient use the spec already recommends.

### 5. Memory and lifetime

- Byte-bounded LRU per surface, with a global cap, evicting oldest first;
  entries drop with their node or when the surface scale changes.
- Generation-aware, following the backlog item on generation-aware bounded
  resources: a resource-revision or theme change invalidates by generation, not
  by walking entries.
- The render diagnostics report cache bytes, hits, misses, and evictions per
  surface, alongside the existing paint metrics, so the profiler shows whether
  a component's blur is being reused.

### 6. GPU path

The painter's command stream does not change. On the Skia-GL backend
(GPU backend plan, phases 2 and 4) the offscreens and cached blurs become GPU
textures, Skia's GPU blur downsamples internally, and the caches become texture
residency. Nothing here assumes the software painter, and nothing needs undoing
when GPU paint lands.

## Phases

0. **Evidence.** Pixel test for the partial-repaint edge defect. Two workloads
   under `nix develop`, release, three runs each: (a) hover inside a frosted
   panel over a static in-surface backdrop; (b) a scrolling list beneath a
   frosted header. Record the baselines in the performance log.
1. **Spec** 04 §9–§10 rewritten to the contract above; status shipped/target per
   part.
2. **Per-element routing and empty-backdrop skip.** Removes the navigation bar's
   CPU blur on every compositor; uses the compositor backend for the region
   side on Hyprland.
3. **Blurred-backdrop cache** with clamp edges and complete snapshots. Gate:
   workload (a) cached beats uncached by a checked ratio, and cached pixels equal
   uncached pixels exactly.
4. **Filter-layer cache.** Gate: a static blurred card over animated content;
   pixel parity with the uncached path.
5. **Budget diagnostics.** Warn when a component's in-surface blur misses the
   cache on most frames over a large area — the author-facing signal that an
   effect is expensive, instead of a silent CPU cost.
6. **GPU.** Folded into the GPU backend plan's resource-residency phase.

## Risks

- Signature coverage: the cache is only correct if every input to the commands
  beneath is in their signatures (images, glyph atlases, resource revision).
  The parity tests must use real text and icons, not rectangles.
- Memory: a surface-wide backdrop on a 4K output at scale 2 is tens of MB per
  entry. The cap has to prefer dropping the cache over growing without bound,
  and falling back to uncached painting must stay correct.
- Scroll beneath a frosted header misses every frame by nature (workload b).
  That case needs cheaper blur (GPU), not caching; do not tune the cache for it.
