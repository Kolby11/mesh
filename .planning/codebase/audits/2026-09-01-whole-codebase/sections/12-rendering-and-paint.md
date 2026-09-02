# Section 12 — Rendering and paint

## Scope and coverage

Reviewed all 57 assigned files in frontend render/display-list/painter/buffer,
render contracts, benchmarks, performance fixtures/tools, and paint-related
documentation. Elements, animation, resources, shell damage, Wayland, and
presentation callers were searched. **57/57 assigned files inspected; no
follow-up remains.**

## Process tree

```text
WidgetNode + style/layout/state/resource revisions
  -> retained render objects/fingerprints/display list
  -> overflow, transform, clip, blur and logical damage
  -> device-scale paint commands and raster caches
  -> PixelBuffer/Skia/software painter
  -> SHM/presentation damage and frame callback
  -> generation/diagnostics/profiling and recovery
```

## Performance findings

### S12-PERF-001 — Raster and glyph work can block the frame path

- **Source:** `frontend/render/src/surface/glyph.rs:379-445,763+`, icon/image
  raster/decode queues, and painter call sites.
- **Current behavior:** cache misses can schedule or perform font/icon/image
  decode/raster work around frame rendering; queue draining and fallback work
  occur on the presentation-facing path.
- **Why it matters:** cold resources and large glyphs can create frame gaps and
  input latency.
- **Recommended improvement:** keep bounded worker queues, use placeholders and
  last-known-good resources, and commit completed resources by revision.
- **Measurement:** 100/1,000 glyphs, 1/10 images/icons, cold/warm caches and
  60/144 Hz frames; measure worker/frame CPU, p95/max frame gap, queue depth,
  allocations, and eventual visual correctness.
- **Confidence:** medium-high. **Status:** older audit/backlog overlap; no speedup
  claimed without measurement.

### S12-PERF-002 — Display-list and render-object signatures may rebuild broad subtrees

- **Source:** `render_object.rs:382-540` and display-list build modules.
- **Current behavior:** retained updates compare fingerprints and child ID slots,
  but topology, paint, resource, and generation changes can rebuild a whole
  subtree.
- **Why it matters:** small state/style changes in large surfaces increase CPU,
  allocations, and damage.
- **Recommended improvement:** maintain typed dirty classes and subtree spans;
  rebuild only affected command ranges while preserving order correctness.
- **Measurement:** 100/1,000/10,000-node trees with leaf style, topology,
  resource, text, and z-order changes; record commands rebuilt, allocations,
  paint CPU, damage pixels, and p95 frame time.
- **Confidence:** medium hypothesis. **Status:** new measurement candidate.

### S12-PERF-003 — Cache caps should be compared by bytes, not only entries

- **Source:** glyph/image/font and painter cache implementations, including
  `surface/profiling.rs:54-144`.
- **Current behavior:** caches expose entry/capacity metrics, while value sizes
  vary substantially with glyph/image dimensions.
- **Why it matters:** a nominally bounded cache can retain disproportionate
  memory or evict useful small entries.
- **Recommended improvement:** use byte budgets and explicit admission/eviction
  telemetry after measuring memory and hit-rate trade-offs.
- **Measurement:** 1/10/100k glyph/image values with varied sizes; record RSS,
  cache bytes, hit rate, eviction count, raster CPU and frame gap.
- **Confidence:** medium. **Status:** hypothesis; historical rejected cache
  experiments are not repeated.

## Dead code and redundancy

### S12-DEAD-001 — Focused proof/profiling and production render metrics overlap

- **Source:** `render/src/proof.rs:10-161`, `surface/profiling.rs`, and painter
  diagnostic snapshots.
- **Current behavior:** proof snapshots, raster metrics, paint metrics, and
  diagnostics retain related evidence with different collection/projection
  paths.
- **Why it matters:** observability code can add per-frame work and report
  inconsistent counters; unused fields are difficult to identify.
- **Recommended improvement:** define a single bounded frame evidence model and
  derive debug/proof/production projections, then remove confirmed unconsumed
  fields only after call-graph review.
- **Test:** feature-on/off parity, no-op frame overhead, and every metric caller.
- **Confidence:** medium redundancy, not confirmed dead. **Status:** new.

### S12-DEAD-002 — Multiple paint backends carry overlapping command lowering

- **Source:** `anyrender_adapter.rs:29-134`, painter/tree lowering, and software
  buffer paths.
- **Current behavior:** display paint commands are adapted into backend-specific
  scene/Skia/buffer operations with repeated color, border, transform, and
  clipping interpretation.
- **Why it matters:** one backend can render a different result or support a
  feature the damage/signature model does not represent.
- **Recommended improvement:** make display commands and effect scopes the single
  semantic owner; keep adapters mechanical and exhaustive.
- **Test:** command corpus rendered through each backend with pixel/geometry
  evidence and unsupported-feature diagnostics.
- **Confidence:** high parallel lowering; **Status:** older audit.

## Logic and core mechanics

### S12-LOGIC-001 — Pixel canvas session can outlive backing storage identity

- **Source:** `surface/buffer.rs:198-265,290+`.
- **Current behavior:** a canvas session retains a raw pointer/slice lifetime
  while a safe buffer callback exposes mutable buffer operations; resizing or
  replacing backing data during a live session can invalidate that pointer.
- **Why it matters:** safe API use can produce undefined behavior and corrupt
  paint memory.
- **Recommended improvement:** make storage private, prohibit resize while a
  session exists, or recreate the drawing surface whenever storage identity
  changes; document a non-resizing access boundary.
- **Test:** API-level interleaving, Miri/ASan, resize/reallocation failure, and
  concurrent session misuse tests.
- **Confidence:** confirmed safety hazard. **Status:** older audit/backlog overlap.

### S12-LOGIC-002 — Paint lowering does not preserve full border geometry

- **Source:** `display_list/paint_node.rs:22-48` and painter/tree border commands
  around `:741-774,1046-1064`.
- **Current behavior:** retained style/signatures carry four widths/corners,
  while paint lowering can use only one radius/edge and one rounded stroke.
- **Why it matters:** asymmetric borders are visibly wrong despite being dirtied
  and fingerprinted as distinct.
- **Recommended improvement:** carry four-edge/four-corner shape data to the
  backend and use the same shape for overflow/damage.
- **Test:** asymmetric widths/radii, zero edges, border-only changes, and pixel
  goldens across backends.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S12-LOGIC-003 — Transform semantics differ across paint, damage, blur, and input

- **Source:** `display_list/paint_node.rs:68-80`, build subtree/bounds code,
  `display_list/blur.rs:26-60`, painter clip rounding, and animation transform.
- **Current behavior:** scale/translation are composed in some paths while
  rotation/transform-origin/ancestor matrices are omitted or represented
  independently.
- **Why it matters:** painted placement, culling, damage, blur, and hit testing
  disagree for transformed content.
- **Recommended improvement:** introduce one affine transform stack with origin
  and derive all bounds/clips/input from it; reject/diagnose unsupported transforms.
- **Test:** nested rotate/scale/translate/origin, clipping, blur, partial damage,
  hit testing, and fractional scale.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S12-LOGIC-004 — Node opacity/blend must be a compositing scope

- **Source:** `paint_node.rs:22-48` and painter/tree background/shadow/image/
  gradient/text lowering around `:741-764,1015-1043`.
- **Current behavior:** opacity/blend is applied to selected primitives rather
  than the complete node contents as one group.
- **Why it matters:** gradients/images/shadows/text/borders do not composite as
  CSS-like node content.
- **Recommended improvement:** lower node opacity/blend/filter to an explicit
  offscreen/compositing scope or declare backend limitations with diagnostics.
- **Test:** translucent mixed-content node, blend with text/icon/image/shadow,
  nested scopes, and backend pixel goldens.
- **Confidence:** high. **Status:** older audit.

### S12-LOGIC-005 — Display-list generation must include paint-order/topology revisions

- **Source:** display-list build/signature modules and retained render object
  update at `render_object.rs:328-505`.
- **Current behavior:** some paint-affecting fields are fingerprinted, but
  topology/order/ancestor scope changes can preserve a command range identity.
- **Why it matters:** stale command order or stale damage can leave an old visual
  result while layout/state says the tree changed.
- **Recommended improvement:** commit command topology and paint signature under
  one generation and require presentation to acknowledge that generation.
- **Test:** reorder, insert/remove, z-order, blend/clip, resource change, partial
  damage, and failed present recovery.
- **Confidence:** medium-high. **Status:** older audit/backlog overlap.

### S12-LOGIC-006 — Presentation errors must retain unshown damage

- **Source:** painter/presentation adapter and shell frame commit consumers.
- **Current behavior:** failure/acknowledgement paths can clear or advance damage
  before a frame is actually shown.
- **Why it matters:** a failed buffer/commit can permanently lose visual updates.
- **Recommended improvement:** separate prepared, submitted, acknowledged, and
  shown generations; clear damage only after the corresponding commit is known
  accepted and retry failed regions.
- **Test:** SHM exhaustion, commit failure, frame callback timeout, output
  removal, and retry with retained damage.
- **Confidence:** high. **Status:** older audit/backlog overlap.

## Existing backlog or audit overlap

The August render audit covers canvas safety, borders, transforms, opacity/blend,
paint order, dirty signatures, text/font revisions, blur, scale rounding,
diagnostics, generation shortcuts, resource invalidation, presentation errors,
blocking raster work, and cache budgets. Current code adds bounded queues,
resource revisions, and retained signatures in places; fixed claims are not
repeated. New candidates are scoped invalidation and cache/frame measurements.

## Refuted suspicions

- Existing renderer work includes resource revisions and bounded raster queues;
  broad “all resource changes are untracked” is not promoted without a failing
  source path.
- Fractional-damage and traversal-fusion experiments are rejected historically
  and are not repeated here.

## Tests and benchmarks needed

- Canvas safety, border/transform/opacity/compositing, command topology, text
  resources, damage/presentation, blur/scale, and backend parity tests.
- Renderer benchmarks with node/command/resource sizes, cache distributions,
  frame rates, CPU, allocations, damage pixels, queue depth, p95/max frame gap,
  and commit/recovery outcomes.

## File coverage

**Assigned:** 57/57 frontend render/display-list/painter/buffer files, render
contracts, benchmarks/performance fixtures/tools, and paint documentation.
**Inspected:** 57/57. Elements/resources/shell/Wayland callers were searched but
belong to Sections 06, 08, 14, and 15. **Files still needing review:** none.
