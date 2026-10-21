# Change-proportional frame pipeline

**Status:** proposed 2026-10-21; the next implementation priority. Backlog lines
are under "Change-proportional frame pipeline" in
[`docs/BACKLOG.md`](../../../docs/BACKLOG.md). This note extends D1, D4, D6 and
D7 of the [runtime architecture direction](2026-10-10-runtime-architecture-direction.md):
those cover the *write* (tracked script state, typed values, typed styles, GPU
commit). This note covers what happens to a change *after* the write.

## Problem

A frame costs time proportional to tree size times the number of passes, not to
what changed. The pipeline produces the frame first and then works out what
changed:

```text
event → effects → build (full / narrow component rebuild / retained reuse)
      → annotate runtime state as `_mesh_*` string attributes
      → restyle → layout → a11y normalize → validate
      → style-animation sample → retained-tree fingerprint + diff
      → runtime style diagnostics → display-list diff → paint
      → child-surface request walk
```

Retained trees, scoped updates, memo entries and fingerprints exist to make
that reconstruction cheaper, and each performance item scopes one pass. With
about ten passes, scoping one buys 3–10% (2026-10-20/21 entries in the
[performance log](../log/performance-log.md)).

Evidence from 2026-10-21 (release, perf with DWARF call graphs):

- `glass_dashboard_spectrum_profile` (151 nodes, 32 visualizer bars per frame):
  3.2–3.5 ms mean. Inclusive shares: tree build 41% (the Player component is
  rebuilt every frame for its `{#for bar in bars}` list), finalize/restyle 17%,
  reactive effects 12%, display-list update 8%, a11y normalization 7%,
  `validate_widget_tree` 6%, annotation 4.5%, style-animation pass 4%.
- String attribute handling: `AttributeMap::get` 5.5–8.6%,
  `parse_attributes_runtime` 8.5%, plus memcmp and string hashing.
- `glass_dock_hover_animation_profile` (1,879 nodes, 1,680 of them a closed
  launcher): before dormant-popover pruning, `apply_style_animations_to_node`
  was 23% inclusive. Pruning one pass took hover p90 from ~2.5 to ~0.85 ms;
  every other pass still walks the tree.
- Live captures (2026-10-20): Settings 29.6 ms mean, dashboard 11.0 ms mean /
  28.7 ms p90, first glass open 100–160 ms. The D9 CPU gates pass easily, so
  they do not show the live gap.

## How Qt Quick and GTK4 avoid it

- **Qt Quick:** items are typed C++ objects. A binding re-runs only when a
  property it read emits NOTIFY, and writes one property. The setter sets a
  dirty bit (geometry, opacity, content, …) and queues the item. Polish
  re-lays out only queued items; the sync phase updates scene-graph nodes only
  for dirty items. A GPU render thread draws batched, retained geometry.
- **GTK4:** a state change (hover = PRELIGHT flag) invalidates its CSS node
  with change flags. The engine knows which selectors depend on that change and
  restyles only affected nodes, sharing computed styles. A changed property
  queues a resize or only a redraw. Frame-clock phases visit only widgets that
  asked: tick callbacks, allocation, snapshot. Unchanged widgets reuse cached
  render nodes; diffing the render-node tree gives damage. The GSK renderer
  draws on the GPU.

In both, a change is recorded *with its kind* at write time and pushed only
through the stages it affects. Diffing, where it exists, is the cheap last
step over already-known dirty widgets.

## Direction

### P0. Measure proportionality first

- A live-shell benchmark: drive the real shell under the glass profile
  (dashboard open, dock hover, Settings scroll, clock tick) and record
  per-surface `total_surface_render` and first-open time. Add it beside the D9
  gates so the live gap is visible.
- Per-pass "nodes visited" counters in profiling records. The target property
  of this work is *visited nodes per frame ∝ dirty nodes*; gate on it with a
  structural count (like `keyframe_compilations_per_instance`), not only on time.

### P1. Typed node properties and state flags

**Shipped 2026-10-21** ([log](../../log/2026-10.md)): `NodeState` flags, typed
runtime fields for every former `_mesh_*` attribute, and the closed `Prop`
vocabulary with per-prop `ChangeClass` returned by `AttributeMap::set`.
Properties stay in the sorted attribute vector rather than per-element structs;
the plan below is kept as the design record.

- Core elements are a closed set (`element/contracts.rs`). Give the
  layout/paint/state-relevant properties typed fields on the node, and keep a
  small map only for genuinely free-form attributes (`data-*`, pass-through
  ARIA).
- Replace the ~30 `_mesh_*` runtime-state string attributes (hover, focus,
  active, checked, scroll, entering/exiting, …) with a `NodeState` bitflag
  field. The annotation pass then mostly disappears: interaction code flips
  flags on the affected nodes.
- Selectors for state and known attributes compile to flag/field checks; this
  is D7's dependency-indexed matching applied to typed inputs.
- Each typed setter returns the change class it caused: style, layout, paint,
  a11y, hit-test, structure.
- Scope: about 180 non-test `attributes.get/insert/contains_key/remove` call
  sites across shell, elements, render, interaction, compiler and scripting,
  plus selector matching, the template compiler, the LSP and automation IPC
  (read-by-name adapter over typed fields). Per the no-compatibility rule, the
  string paths are deleted, not kept alongside.
- Direct payoff alone is about 1.2× (the string-handling shares above). The
  real value is P2: typed writes classify the change.

### P2. One frame dirty set drives every pass

- One per-frame `FrameDirty` holds per-node change classes, fed by typed
  setters, interaction flag flips, animation samples (with their bucket), and
  layout results (nodes whose geometry moved).
- Every pass consumes it instead of walking the tree: restyle (dirty nodes plus
  selector dependents), layout (Taffy dirty marking only), style-animation
  sampling (restyled + animating nodes), a11y normalization, validation
  (structural changes only), retained update, runtime style diagnostics,
  display-list update, hit-test data, and child-surface requests.
- Fingerprint-and-diff discovery is retired from the frame path. Keep it as a
  test-only verifier that the dirty set was complete (the pattern of
  `animated_ancestor_scoped_update_leaves_nothing_for_a_full_update`).
- Full passes remain only for structural rebuild, theme change and resize.
- This subsumes the canonical render-frame snapshot item from the section 12
  audit: one snapshot plus one dirty set is the shared invalidation model.

### P3. Bindings write typed properties (finish D1)

- The `.mesh` compiler resolves each binding's target to a typed property, so a
  binding update is a typed setter call: convert from Luau once, then raise
  the setter's change class.
- List and compound sources become keyed stores: changing `bars[i].height`
  patches item *i*'s bound properties instead of rebuilding the Player
  component (the dashboard's 1.1 ms per frame).
- Providers send typed field patches (D4), so no JSON is on the frame path.

### P4. GPU-first, pipelined rendering (D6)

- GPU raster as the default where the hardware gates pass. Pipeline frame N's
  GPU work against frame N+1's script and layout; presentation currently waits
  for the worker.
- Scene preparation narrowed by retained command/resource revision, so small
  damage does not record the whole scene.
- Re-check the scroll raster cache and in-surface blur plans against this.

## Order and exit criteria

1. **P0** first, so every later step has a before/after measurement.
2. **P1 + P2 prototype** on the glass dock and dashboard: typed state flags
   and the dirty set for interaction and animation frames. Exit: visited nodes
   per hover/animation frame track the dirty set, the verifier finds no
   missed nodes, and dock hover / dashboard frame time improve by a large
   factor, not a few percent. If they don't, record why in the performance
   log before expanding.
3. Expand P1/P2 to all elements and passes; delete the string state
   attributes and fingerprint discovery.
4. **P3** on top of typed setters; **P4** in parallel on the renderer lane.

## Backlog items folded in here

These lines were removed from the backlog on 2026-10-21 because P1/P2 replace
them:

- Retained layout snapshots treat every STYLE difference as a layout
  candidate; split layout and paint fingerprints (render sweep, 2026-10-19).
- The style-animation pass walks the whole tree on any restyle; visit only
  restyled and animating nodes.
- Unscoped restyle frames still fingerprint the whole tree
  (`retained_tree.update`); diffing only restyled nodes needs layout-changed
  nodes reported too.
- Binding-patch frames still pay whole-tree passes
  (`layout_dirty_node_snapshots`, style-animation pass, display-list diff,
  revalidation); patches inside `display: none` should skip layout.
- Layout-affecting animations still take the full retained update.
- Runtime style diagnostics re-resolve every style-dirty node after each
  retained update.
- Interned `Symbol`/`TagId` types and a typed `WidgetNode` (attribute keys,
  module ids and element tags are done; tags in the widget tree and attribute
  values remain).
- Establish one canonical render-frame snapshot and transform/clip model;
  unify invalidation, display-list reuse, damage, blur regions and hit testing
  ([section 12 audit](../log/sections/12-rendering-and-paint/improvements.md)).
- Pipeline GPU paint of frame N against script work of frame N+1, and narrow
  scene preparation by retained revision (moved to P4).

## Open questions

- Whether typed properties live on `WidgetNode` or on a separate per-element
  struct that `WidgetNode` points to (memory vs. cache locality).
- How CSS attribute selectors on free-form attributes keep working without a
  per-node map scan.
- Whether signals are Rust-owned (backlog item R) or Luau-side cells; P3
  depends on the answer.
- How accessibility normalization becomes incremental without missing
  inherited name/role changes.
