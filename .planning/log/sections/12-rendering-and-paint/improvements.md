# Section 12 — Rendering and paint audit

**Audited:** 2026-08-20  
**Scope:** mesh-core-render, plus the shell and presentation seams that turn
retained paint into a Wayland commit. No production code was changed.

## Review execution

Four delegated Luna xhigh passes were used: a whole-process instruction-tree
pass, a logic/implementation-order pass, a direct code-error pass, and a
cache/resource/diagnostic render-boundary pass. Their reports were integrated
with the local source audit and focused test runs.

The inspected flow was:

  WidgetNode + computed style/layout/state
    -> retained snapshots and render fingerprints
    -> dirty summary + dirty NodeIds
    -> retained display entries/signatures and subtree spans
    -> logical damage, visual overflow, blur/layer expansion
    -> selected command spans / layer scopes
    -> device-scale clips and Skia/software paint commands
    -> cleared damage pixels + text/icon/effect rasterization
    -> SHM copy, Wayland damage, frame callback, presentation
    -> retained generations, opaque/blur regions, profiling and proof output

The intended invariant is that every pixel-affecting input has one
authoritative fingerprint, every retained command/topology change has a
generation visible to its consumers, and the same transform/clip/effect model
is used for culling, damage, paint, blur regions, hit testing, and presentation.
The current implementation does not yet maintain those invariants end to end.

## Severity-ranked findings

### P0 — `PixelCanvasSession` exposes a safe use-after-reallocation hazard

`PixelCanvasSession::with_canvas` releases Skia's slice lifetime and keeps a
raw pointer into `PixelBuffer::data`
(`crates/core/frontend/render/src/surface/buffer.rs:198-249`). Its safe
`with_buffer` method exposes `&mut PixelBuffer` (`buffer.rs:254-265`), while
the buffer's `data`, `width`, and `stride` fields are public. A caller can
resize or replace the `Vec`, then invoke another Skia draw through the same
session.

This violates the safety comment's allocation-stability invariant and can
turn ordinary safe Rust usage into undefined behavior. Make backing storage
private and expose only non-resizing byte access, or recreate the Skia surface
whenever storage identity changes. Add an API-level compile/use regression and
Miri or sanitizer coverage for interleaved raw writes and Skia draws.

### P1 — The paint lowering drops asymmetric borders and corners

DisplayPaintStyle stores only border_radius.top_left, even though the style and
display signatures retain all four radii
(crates/core/frontend/render/src/display_list/paint_node.rs:26-32 and
crates/core/frontend/render/src/display_list/signature.rs:189-208). The
retained painter then uses that one radius for background, shadow, and border
(crates/core/frontend/render/src/surface/painter/tree.rs:741-774). More
seriously, push_border_commands returns unless the top edge is nonzero and
draws one rounded stroke using border_widths.top, ignoring right, bottom, and
left widths (crates/core/frontend/render/src/surface/painter/tree.rs:1046-1064).

This is a visible correctness error, not merely an optimization: a style with
border-width: 1px 2px 3px 4px or four different radii is hashed and dirtied,
but paints the wrong geometry. The existing tests exercise equal-width or
single-radius borders, so they do not catch it.

Fix direction: carry a four-edge/four-corner paint shape into the backend
command and lower it to an RRect/path with per-edge stroke geometry. Keep
visual overflow and damage based on the same shape.

### P1 — Transforms are not one affine operation across paint, damage, blur, and descendants

transformed_layout_at applies only the node's scale and translation; it does
not apply rotation or transform-origin
(crates/core/frontend/render/src/display_list/paint_node.rs:68-80). The
subtree builder passes only accumulated translation to descendants
(crates/core/frontend/render/src/display_list/build.rs:377-400), and subtree
bounds do the same (crates/core/frontend/render/src/display_list/build.rs:595-621).
The animation module explicitly says rotation is still identity for the painter
but simultaneously returns true from is_paintable
(crates/core/ui/animation/src/transform.rs:8-27).

The invalidation contract is also incomplete: hash_transform omits
transform_origin (crates/core/shell/src/shell/component/runtime_tree/fingerprint.rs:168-172,422-428),
and TransformSlot has no origin
(crates/core/frontend/render/src/render_object.rs:361,548-555). A change to the
parsed origin therefore has no complete dirty-to-paint path.

The same omission reaches effects. Backdrop regions use the node's translated
layout and local scale but do not compose ancestor transforms or rotation
(crates/core/frontend/render/src/display_list/blur.rs:26-60), while raster
bounds and clips independently round axis-aligned rectangles
(crates/core/frontend/render/src/surface/painter/tree.rs:1067-1088). A rotated
or scaled ancestor can therefore have incorrect child placement, culling,
damage, compositor blur coverage, and partial repaint selection.

Fix direction: introduce one affine matrix/transform stack with an explicit
origin. Store transformed float bounds/AABBs alongside commands, use the matrix
for descendant paint and clip, and derive damage/blur/input geometry from that
same matrix. Until then, unsupported rotations should be rejected or diagnosed;
claiming they are paintable is unsafe.

### P1 — Node opacity and blend mode are lowered per primitive instead of as a node compositing group

The display-list builder premultiplies only background, border, and text colors
with node opacity; it leaves background_paint, box_shadow, filters, and the
rest of the paint payload separate
(crates/core/frontend/render/src/display_list/paint_node.rs:22-48). The painter
draws the shadow and background paint independently
(crates/core/frontend/render/src/surface/painter/tree.rs:741-764), and
mix_blend_mode is passed only to the solid background-color fill
(crates/core/frontend/render/src/surface/painter/tree.rs:748-756). Image and
gradient commands do not receive the node opacity or blend mode
(crates/core/frontend/render/src/surface/painter/tree.rs:1015-1043), and the
box shadow uses the unmodified style value.

Consequently, a translucent node containing a gradient/image/shadow, or a node
using blend mode with text, border, or icon content, does not composite as one
CSS-like element. The fingerprint/signature work correctly notices many of
these changes, which makes the wrong output more expensive rather than correct.

Fix direction: lower an effect-bearing node into an isolated node layer, paint
all of its primitives into that layer, then apply opacity and blend once.
Reuse the existing filter-layer machinery, but make isolation explicit in the
command topology and damage metadata.

### P1 — Paint-order changes are not represented by the display-list generation

Paint order is computed from z_index in build_paint_subtree
(crates/core/frontend/render/src/display_list/build.rs:377-400), but entry
collection walks authored order
(crates/core/frontend/render/src/display_list/build.rs:147-208). Entries are
stored in a HashMap, and primitive_signature does not include z_index
(crates/core/frontend/render/src/display_list/signature.rs:152-234). A z-order
change can therefore rebuild the command stream while leaving the entry map
unchanged. RetainedDisplayList::generation increments only for rebuilt or
removed entries or forced full damage
(crates/core/frontend/render/src/display_list/mod.rs:415-417).

The shell uses that generation to decide whether to recompute compositor opaque
and blur regions
(crates/core/shell/src/shell/runtime/render/mod.rs:795-826). After an
order-only change, those regions can remain from the previous command stream
even though the pixels and backdrop dependencies changed. Debug batch metrics
are also computed from authored traversal rather than actual paint order.
Finally, equal-z siblings are sorted with sort_unstable_by_key
(crates/core/frontend/render/src/display_list/build.rs:546-567), so stable
document order is not guaranteed when an inversion exists.

Fix direction: retain an ordered command/topology signature, not only a keyed
entry map, and increment a separate topology/effect generation whenever order,
layer scopes, blur dependencies, or command kinds change. Region caches must
key on that ordered generation. Use a stable sort with document-order tie
breaking.

### P2 — The render-object dirty contract is not the same contract as the paint signature

The shell's retained tree emits render dirty IDs after comparing
RenderObjectFingerprint
(crates/core/shell/src/shell/component/runtime_tree/tree.rs:631-653), while
sparse display-list patching requires those IDs and a compatible dirty summary
(crates/core/frontend/render/src/display_list/mod.rs:979-1000). The two
fingerprints are not closed over the same paint inputs:

- checkbox/radio checked is hashed by display-list content signatures but is
  absent from primitive_hash
  (crates/core/frontend/render/src/display_list/signature.rs:268-305 and
  crates/core/frontend/render/src/render_object.rs:694-724);
- text content can come from the text attribute, but text_slot retains only
  content
  (crates/core/frontend/render/src/display_list/paint_node.rs:101-113 and
  crates/core/frontend/render/src/render_object.rs:581-592);
- text font style, letter spacing, and direction, plus icon variable axes and
  blend mode, are represented in style/signature code but not completely in
  the render-object slots
  (crates/core/shell/src/shell/component/runtime_tree/fingerprint.rs:176-215,
  crates/core/frontend/render/src/render_object.rs:373-380,655-691).

The current full entry reconciliation often masks this by rescanning the tree
when the retained generation changes, but it defeats the promised sparse path
and makes correctness depend on a fallback. A future direct-reference or
subtree-patch caller can silently classify a changed node as clean.

Fix direction: derive the render dirty fingerprint from the same typed
DisplayPaintInput used to build signatures, including content attributes,
all style fields, topology inputs, and resource revisions. Add a debug proof
that a sparse patch's dirty IDs cover every changed signature.

### P1 — Text style changes can be dirtied but are not painted, and font caches lack resource revisions

ComputedStyle fingerprints and display signatures include font_style and
letter_spacing, but DisplayPaintStyle does not carry them
(crates/core/frontend/render/src/display_list/paint_node.rs:36-42), and the
text renderer's layout parameters contain neither field
(crates/core/frontend/render/src/surface/text.rs:100-143). The retained text
paint call therefore cannot implement those style changes
(crates/core/frontend/render/src/surface/painter/text.rs:546-560). Text
direction is reduced to an alignment adjustment rather than being part of the
shaping contract
(crates/core/frontend/render/src/surface/painter/text.rs:500-506).

The text/glyph caches have a second invalidation hole. The glyph key uses a
hashed font path, codepoint, quantized size, tint, and axes, but no file/catalog
revision (crates/core/frontend/render/src/surface/glyph.rs:46-58). The font
bytes cache is keyed only by Arc<Path> and returns the old bytes after a font
file is replaced in place
(crates/core/frontend/render/src/surface/glyph.rs:68-110). The text layout key
similarly includes text/family/numeric shaping inputs but no font-resource
generation (crates/core/frontend/render/src/surface/text.rs:159-181).
File-backed raster icons already use freshness in their key
(crates/core/frontend/render/src/surface/icon.rs:92-101,328-349), so icon-font
and text resources are the inconsistent side of this boundary.

Fix direction: pass a typed text style/shaping record through the display list,
including letter spacing, font style, language/direction and fallback policy.
Add an immutable font-catalog/resource revision to all font bytes, glyph,
shaping, and named-family caches; invalidate atomically on catalog replacement.

### P2 — Compositor-only backdrop blur is an implicit backend policy, not a complete renderer contract

The display-list path computes backdrop regions and expands damage, but the
retained painter intentionally leaves the SHM pixels flat; the existing test
asserts that behavior
(crates/core/frontend/render/src/surface/painter/tests/skia/filters.rs:7-44).
The backend nevertheless exposes and implements an ApplyFilter::Backdrop
command
(crates/core/frontend/render/src/surface/painter/backend.rs:225-242,802-833),
while the display-list tree emits no corresponding command in
render_display_node_self
(crates/core/frontend/render/src/surface/painter/tree.rs:718-798).

That is coherent for a Wayland compositor with a blur protocol, but not for a
software/Skia adapter or a presentation backend without that protocol. The
current contract has no explicit capability decision or diagnostic at the
lowering boundary; an adapter can show a flat translucent panel while the
metadata says blur was requested.

Fix direction: make backdrop policy explicit per presentation/backend:
compositor region, in-surface readback/filter, or rejected-with-diagnostic.
Include the selected policy in command topology and regression-test each
backend's observable result.

### P2 — Fractional scaling uses several independent rounding policies

Logical damage is floor/ceil scaled in the shell
(crates/core/shell/src/shell/component/shell_component/damage.rs:3-22) and
again at the presentation protocol boundary
(crates/core/presentation/src/wayland_surface/backend/damage.rs:151-181).
Paint bounds and command clips independently round x/y/width/height in the
painter (crates/core/frontend/render/src/surface/painter/tree.rs:1073-1088).
For fractional scale and adjacent fractional layout edges, independent width
rounding can create a one-pixel seam or cause a clip to cover a different set of
pixels than the damage that was cleared and reported.

Fix direction: normalize once into a device-space coverage representation:
transform logical edges, floor the left/top and ceil the right/bottom, intersect
with the physical buffer, and carry that exact device clip through clear, paint,
copy, and damage_buffer. Keep logical damage for policy/debug output only.

### P2 — Release diagnostics discard batch and barrier metrics

In debug builds the display list computes batch metrics from ordered entries, but
release builds replace them with DisplayListMetrics::default()
(crates/core/frontend/render/src/display_list/mod.rs:410-413). The public
metrics still expose these fields and the shell profiling/HUD consumes them.
Release diagnostics therefore report zero batches/barriers even when the
release renderer is doing the work.

Fix direction: retain a compact ordered material stream in release, or make
metrics collection an explicit runtime profiling mode rather than a build-mode
side effect. Add a release test that asserts nonzero metrics for a known
batchable scene.

### P2 — Generation shortcuts trust caller lineage without a validation hook

Both render-object and display-list updates return early when the supplied
retained generation is unchanged
(crates/core/frontend/render/src/render_object.rs:125-136 and
crates/core/frontend/render/src/display_list/mod.rs:203-222). Existing tests
intentionally demonstrate that changing the tree under the same generation is
skipped (crates/core/frontend/render/src/render_object.rs:1295-1319). This is
a valid optimization only if the generation is an unforgeable, authoritative
lineage token; the public APIs currently accept a plain u64 and do not validate
that promise.

Fix direction: pass an opaque retained snapshot/token or add debug-only
fingerprint validation and a diagnostic when a same-generation tree differs.
Keep the fast path in production once the authority is mechanically enforced.

### P1 — Resource revisions are missing from retained present decisions

The retained-list and shell present-cache keys do not carry a renderer
resource/catalog revision (`display_list/mod.rs:212-221`,
`display_list/build.rs:286-297`, and `shell/runtime/render/mod.rs:956-989`).
An icon pack, profile mapping, theme asset, or other resource can change while
the widget tree is unchanged; retained rendering can then report no damage and
leave old pixels visible. Add an immutable `RendererResourceRevision` to
retained-list, child-surface, and present-cache keys, and target affected
resources or conservatively damage the surface. Test unchanged-node icon-pack
switching with a non-empty present damage result.

### P1 — Presentation errors can lose damage and acknowledge an unshown frame

`wayland_surface/backend/entry.rs:357-365` drains pending damage before the
copy/attach operation, while `entry.rs:487-489` discards `buffer.attach_to`
errors. The present path can subsequently return `Presented`; shell restoration
focuses on `PresentStatus::NotReady` (`shell/runtime/render/mod.rs:830-839`).
Window geometry caching at `entry.rs:306-320` compares only width and height,
not the origin.

An attach or recoverable buffer error can therefore drop the only dirty-region
record and cause a retry to skip stale pixels. A same-sized window whose origin
changes can retain stale compositor geometry. Make presentation transactional:
retain damage until commit succeeds, restore it on every recoverable error,
propagate attach failures, and cache `(x, y, width, height)`. Add injected
attach-error and same-size-origin tests.

### P1 — Resource decode and rasterization can block the frame thread

Icon cache misses synchronously call `image::open`, read SVG text, and decode or
rasterize (`surface/icon.rs:129-151`, `252-265`, `520-567`, `652-713`); font
misses read files and rasterize glyphs (`surface/glyph.rs:91-110`). Large or
slow assets can exceed the frame budget, and external-resource SVGs can bypass
variant caching. Add a generation-aware asynchronous resource broker with
bounded immutable decoded assets, cancellation, and a deterministic placeholder.
Validate all `width * height * 4` arithmetic before allocation.

### P1 — Cache caps are entry-count based rather than byte-budgeted

Icon, glyph, text, painter, PixelBuffer, and SHM caches can hold large assets
while remaining within small LRU entry limits. A few large images or glyph
atlases can therefore cause unpredictable memory growth. Enforce process-global
and per-surface byte budgets and dimension caps, account for decoded images,
fonts, text, Skia, PixelBuffer, and SHM allocations, and evict or fail
deterministically. Add a large-asset stress test with a measurable memory
ceiling.

## Better architecture and concrete feature

The most valuable feature is a frame paint plan rather than separate dirty
hashes, display-entry maps, and ad-hoc damage expansions. A plan would contain,
per retained subtree:

- an immutable typed paint input and resource-revision fingerprint;
- ordered command topology, stacking/layer scopes, and a topology generation;
- a cumulative affine transform and clip stack;
- old/new device-space visual bounds and effect dependency regions;
- the selected replay spans and the exact logical/device damage used for clear,
  raster, SHM copy, and Wayland damage.

The plan would let the renderer answer one question once—“which old and new
pixels can differ, and which earlier commands/effect layers are dependencies?”—
and hand the same answer to painter, diagnostics, and presentation. It also
enables a concrete new capability: backend-selectable backdrop blur and
isolated opacity/blend groups without changing dirty-state semantics. This is a
better foundation for GPU or buffer-age presentation than extending the current
per-field hashes.

## Recommended implementation order

1. Lock the contracts with regression tests and proofs. Add pixel-parity tests
   for unequal borders/corners, opacity over gradient/image/shadow, text style
   changes, checked controls, z-order-only changes, rotated/scaled descendants,
   transformed blur regions, fractional scale, and release diagnostics. Add a
   sparse-vs-full proof that every changed paint signature is covered by dirty
   IDs.
2. Unify paint inputs and resource revisions. Define the typed paint-input
   record, include all content/style/topology fields and font/icon catalog
   revisions, and make render dirty, display signatures, and cache keys derive
   from it. This is a dependency for trustworthy sparse patching.
3. Implement the affine transform/clip model. Add transform-origin and matrix
   composition, then use it for descendant layout-to-paint bounds, culling,
   damage, blur regions, clips, and interaction coordinates. Do this before
   optimizing partial damage; otherwise the optimization preserves wrong pixels.
4. Correct primitive/effect lowering. Add four-edge/four-corner borders, text
   shaping fields, and isolated opacity/blend groups. Make backdrop blur an
   explicit backend capability with a diagnostic/fallback.
5. Make display-list topology retained and generational. Store ordered spans
   or rope segments with stable tie ordering. Increment topology/effect generation
   on order, layer, blur, and command-kind changes; key opaque/blur region caches
   and proofs to it.
6. Collapse scaling to one device-space coverage pass. Use the same physical
   rectangles for buffer clear, painter clips, SHM copy, and protocol damage;
   preserve logical rectangles only as metadata.
7. Finish cache and diagnostics lifecycle. Make font/icon catalog replacement
   revisioned and atomic, move blocking resource misses off the paint thread,
   and make release profiling truthful. Only then pursue direct SHM paint,
   tile-parallel raster, or a GPU adapter.

## Regression-test matrix

These are concrete tests to add; this review did not edit the test suite.

| Area | Suggested regression | Primary test location |
| --- | --- | --- |
| Dirty-to-display | checked_attribute_and_text_attribute_changes_cover_sparse_dirty_ids | crates/core/frontend/render/src/render_object.rs and display_list/tests/entries.rs |
| Borders | retained_painter_uses_all_border_edges_and_corner_radii | crates/core/frontend/render/src/surface/painter/tests/skia/shapes.rs |
| Compositing | opacity_and_blend_apply_to_shadow_gradient_image_text_and_icon_as_one_node | crates/core/frontend/render/src/surface/painter/tests/skia/shapes.rs |
| Transforms | rotation_origin_and_nested_scale_match_matrix_reference | crates/core/frontend/render/src/display_list/tests/entries.rs plus Skia pixel tests |
| Blur/damage | transformed_backdrop_region_and_sparse_replay_match_full_repaint | crates/core/frontend/render/src/display_list/tests/damage.rs and surface/painter/tests/skia/filters.rs |
| Order/generation | z_order_only_change_updates_topology_generation_and_regions | crates/core/frontend/render/src/display_list/tests/entries.rs and shell render tests |
| Text/resources | font_revision_invalidates_bytes_glyph_and_layout_caches | crates/core/frontend/render/src/surface/text.rs and surface/glyph.rs |
| Scaling/present | fractional_scale_uses_identical_clear_paint_copy_protocol_damage | shell damage and crates/core/presentation/src/wayland_surface/backend/damage.rs tests |
| Diagnostics | release_display_metrics_report_batches_and_barriers | crates/core/frontend/render/src/display_list/tests/damage.rs |

## Verification performed

- nix develop -c cargo test -p mesh-core-render --lib: 201 passed, 0 failed,
  37 ignored.
- nix develop -c cargo test -p mesh-core-render --test paint_perf_scenarios:
  6 passed, 0 failed.
- nix develop -c cargo check -p mesh-core-render: passed.

The report identifies uncovered correctness and ordering risks despite that
suite being green; the missing tests above are specifically designed to expose
the gaps.
