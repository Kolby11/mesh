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

**Visit accounting implemented 2026-10-21.** Opt-in frame counters cover the
named traversal/synchronization functions across style, layout, animation,
semantics, validation, annotation, retained updates and display lists. Child
requests report separately after paint. The profiler snapshot, session logs,
report tool and glass fixtures expose those counts. The animation structural
gate consumes the exported counts. This is not an all-pass proportionality gate;
see [counter definitions](../../../docs/performance-profiling.md#node-visit-accounting).
The reproducible live workload replay and compositor timing gates remain open.

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

**Prototype measured 2026-10-21; exit not met.** `FrameDirty` coalesces typed
binding writes, interaction annotation changes and animation buckets. Only
geometry-preserving animation-only frames currently use it as an authoritative
retained-update scope; snapshot paths also limit sampling to registered nodes.
The sparse/full-diff verifier and visit-count regression pass. Dock steady p50
improves, but dock p90 and dashboard frame ranges overlap. The dashboard still
rebuilds Player on every spectrum update. Step 1 below then scoped
interaction frames. Layout propagation, display-list ancestors, validation
and child-surface requests still need explicit scopes. See the P2 prototype entry in the
[performance log](../../log/performance-log.md). The bullets below remain the
full direction, not a shipped claim.

#### Step 1: interaction-frame scope (design 2026-10-21)

**Implemented 2026-10-21** as designed below. Steady dock hover now annotates
8–14 nodes per paint and restyles 12, with no retained discovery. Hover-change
and animation-only frames are about 2.5× faster. It also fixed a latent
surface-root inheritance bug in the targeted restyle. Measurements and what
remains: the P2 step 1 entry in the
[performance log](../../log/performance-log.md).

Measured shape, glass dock steady hover, visits per paint. A hover-change
paint (one step in six) visits annotation 1,879, restyle scan 1,878 and
retained discovery 1,879. Animation-only paints visit annotation 1,879 and
otherwise only sampled nodes. The 376-per-paint means in the P0 profile are
1,879 / 5, not a 20% scan.

**No selector-dependents index is needed.** The selector subset
([04](../../../docs/spec/04-styling.md), "Component `<style>` blocks") rejects
combinators and `:has()`, so whether a rule matches depends only on the node's
own tag, classes, id and state. A state change reaches other nodes only
through inherited text fields and custom properties, which the targeted
restyle already detects by comparing `ParentInheritedStyle`. Ambient window
states change every node and keep the full path. The restyle targets are
exactly the state-changed nodes. Their dependents are the descendants whose
inherited values changed. If combinators are ever admitted, this is where a
dependents index goes.

1. **Scoped annotation.** Every retained or rebuild annotation whose overflow
   extents are current stores its `RuntimeAnnotationInputs`. The next retained
   frame diffs its inputs against them: hover tails, focus, focus-visible,
   active, active slider, checked, expanded, input value, preedit and slider
   diffs, plus patched binding nodes. It finds those nodes, and every scroll
   container, through the last frame snapshot's paths and projects only them.
   The full walk remains for rebuilds; window, promotion or shortcut changes;
   frames after layout ran (annotation measures overflow before layout); a
   missing snapshot; and a path that no longer matches. This subsumes the
   scroll-only annotation fast path.
2. **Path-directed restyle.** Targets are sorted by snapshot path and reached
   directly. A target's parent style comes from its parent's retained computed
   style. The target is re-resolved, and descent continues only while
   inherited values changed. Paths are validated before any mutation, so a
   fallback never restyles a node twice. Finalized-style validity is cleared
   on target ancestors only; clean siblings keep it, since rebuild frames
   revalidate bottom-up anyway. The pass reports the exact re-resolved set.
3. **Retained update from that set.** When layout was reused and `FrameDirty`
   holds no LAYOUT or STRUCTURE, the retained update consumes restyled,
   annotated, semantic and animation IDs through snapshot paths
   (`update_for_frame_dirty`), not the fingerprint walk.

Exit check: a structural test hovers one item beside many clean siblings.
Annotation, restyle-scan and retained-discovery visits must not grow with the
sibling count. A full retained diff afterwards must find nothing, and a twin
component forced onto the full paths must paint identical pixels and computed
styles. Dock p50/p90 before and after on the release glass fixture.

#### Full direction

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

## Execution plan (2026-10-21)

**Status:** wave 1 running. W1 landed 2026-10-21 (`06d5750f`): the patched-frame restyle was already path-directed, with 336 visits = about 24 targets × path depth. Paint-only patches now keep geometry. Dashboard frames are bound by layout-class patches, which makes W2 the dashboard lever. W6 parts (b) and (c) landed 2026-10-21: pipelined presentation and the render-path diagnostic; part (a) follows W3. W4 landed 2026-10-21: the live replay gate. Its structural assertion now passes: the idle dock repaint (a missing-icon resolution loop) and the live hover full rebuilds (handler writes during input dispatch) are fixed; see the performance log. Wave 1 is complete. W2 landed 2026-10-22: layout-class patch frames lay out the patched branch from the restyled set (`compute_incremental_for_stable_tree`, `layout/sparse.rs`); gate `patched_frame_layout_visits_scale_with_the_patch`. Its `changed_nodes` report is consumed downstream by W7. W5 implemented 2026-10-22: bindings retain typed `Prop`/free-form targets; rebuild-time indexes retain target paths and item snapshots; plain item-field key dependencies reuse validation. `binding_patch_visits_no_target_discovery` holds one setter, zero target discovery and zero runtime key validation at 16 and 1,024 siblings, with forced-rebuild pixel/style parity. Snapshot item comparisons remain a P3/D4 target. Wave 2 is complete. W3 implemented 2026-10-22: retained display ancestor paths, snapshot-backed transition capture, restyled/registered animation sampling, retained structure validation, reversible lifecycle classes, and indexed child-request dependencies. The 16/1,024-sibling gates pass with pixel/style parity. Closed popovers retain only ancestor/self dependencies. Concurrent W7 edits are included in the measured candidate, so timings are not a W3-only attribution. W6 scene preparation can proceed; W7 discovery retirement is also implemented. Measurements are in the performance log. Baseline commit `55800ed5`. Implementation
agents work in worktrees and do not edit this note, the backlog, STATUS or the
log. The orchestrator integrates one commit per workstream.

**W7 implemented 2026-10-22:** stable-frame retained synchronization uses explicit
node/subtree paths, including layout and scroll changes. Discovery helpers are
`#[cfg(test)]`; `MESH_VERIFY_RETAINED=1` runs a full completeness verifier after
every paint. Rebuilds and topology mismatches reconcile the full tree before
mutation. Dashboard discovery visits are 151 → 0 across nine samples per side;
latency ranges overlap and the candidate includes concurrent W3 changes. The
layout structural gate checks constant retained sync at 16 and 1,024 clean
siblings plus full retained verification and pixel parity. See the performance
log; shared path-prefix resolution remains open in the backlog.

### Workstreams

Shared hub: `ShellComponent::paint` (`shell_component/mod.rs`) and
`finalize_tree` (`rendering/mod.rs`). Every P2/P3 workstream edits them, so
those workstreams are serialized.

| ID | Scope | Main files | Structural gate (16 vs 1,024 clean siblings, plus forced-full twin parity) | Timing profile |
| --- | --- | --- | --- | --- |
| W1 | P2: keyed/binding-patch frames restyle only patched nodes plus inherited-value descendants (path-directed, as in step 1); dashboard `restyle_scan` 336 → about the patched count | `rendering/mod.rs` (`finalize_tree` restyle selection), `elements/style/resolve/subtree.rs`, `template_patch.rs` (patched-node set only) | `patched_frame_restyle_visits_scale_with_the_patch_not_the_tree` | dashboard spectrum |
| W2 | P2: layout from the dirty set. Taffy dirty-marking for LAYOUT-class nodes; delete `layout_dirty_node_snapshots` discovery on patch frames; `display: none` patches skip layout | `runtime_tree/tree.rs` (`layout_dirty_node_snapshots`), `elements/layout/retained.rs`, `lowering.rs`, layout part of `finalize_tree` | `patched_frame_layout_visits_scale_with_the_patch` (`layout_discovery` 0, `layout_sync` ∝ patched) | dashboard spectrum, dock hover |
| W3 | P2: remaining walks. Display ancestors on entrance/lifecycle frames; previous-style capture only for restyled transition nodes; `validate_widget_tree` only on STRUCTURE; child-surface requests from the dirty set | `animation.rs`, `render/display_list/build.rs`, `elements/tree.rs`, `shell_component/child_surface.rs` | one gate per pass; entrance-frame `display_ancestors` ∝ entering subtree | dock hover (entrance frames), navigation hover |
| W4 | P0: reproducible live glass replay (dashboard, dock hover, Settings scroll, clock tick) through the real shell and compositor, reporting `total_surface_render`, first-open time and per-pass visits; skips cleanly without a compositor | `presentation/tests/live_compositor_matrix.rs` or a new shell harness, `tools/`, `docs/performance-profiling.md` | replay emits `MESH_PERF` metrics; visit counts are checked against fixture counts | live replay |
| W5 | P3: the compiler resolves binding targets to `Prop`; a patch is a typed setter raising its `ChangeClass`; remove key-validation and binding-target-discovery scans; fix the sibling keyed-loop NodeId collision (loop discriminator) | `frontend/compiler/src/render/*`, `template_patch.rs`, `elements/prop.rs` | `keyed_list_*` gates plus `binding_patch_visits_no_target_discovery`; collision regression | dashboard spectrum |
| W6 | P4: GPU raster default where the hardware gates pass; overlap frame N GPU work with frame N+1 script and layout (no presentation wait); scene preparation narrowed by retained revision | `render/src/surface/painter/*`, scene thread / GPU worker, `presentation/src/wayland_surface/backend/*` | `gpu_frame_cost_workload`; scene-prep recorded commands ∝ damage; existing hardware gates | `gpu_frame_cost_workload`, `appearance_frame_cost_profile` |
| W7 | Retire fingerprint discovery (`retained_tree.update`, `RetainedDiscovery`) from the frame path; keep it as a `#[cfg(test)]` completeness verifier run after every gate frame | `runtime_tree/tree.rs`, `fingerprint.rs`, `paint` | every gate asserts `retained_discovery` 0 outside rebuild/theme/resize; verifier finds no misses across the integration suite | all of the above |

### Waves

1. **W1, W4, W6** in parallel. W1 is the shell restyle hub. W4 is test harness
   and tooling. W6 is the render crate and presentation (not `display_list/build.rs`).
2. **W2, W5** in parallel after W1. W2 owns layout and `layout_dirty_node_snapshots`.
   W5 owns the compiler and `template_patch.rs`. Both touch `paint` only at
   call sites; the orchestrator resolves that seam when merging.
3. **W3** after W2 (shared `paint`/`tree.rs`) and W6 (`display_list/build.rs`).
4. **W7** last, once every pass has a dirty-set consumer.

W6 part (a), scene preparation narrowed by retained revision, moved to after W3. The retained revision naturally comes from `display_list/build.rs`, which W3 owns. Its design inputs: scene prep repaints the full selection per frame (`complete_scene` forces `full_selection`). A per-node picture cache would key on `Arc<DisplayPaintNode>` identity, and it must leave out layer scopes, damage-clipped `paint_clip`, backdrop regions, scroll and tooltip state, and in-flight async resources.

### Measurement protocol (every agent)

- Build and test inside `nix develop`. Timing uses the release profile (thin
  LTO, one codegen unit) and CPU Skia raster unless the work is renderer work.
  Parallel agents pin to distinct cores (`taskset -c 2`, `-c 6`, `-c 10`) and
  build with `CARGO_BUILD_JOBS=4`. The orchestrator re-times every merge
  serially on `main` on core 2. Those are the numbers recorded in the log.
- Save the baseline test binary before editing. Alternate baseline and
  candidate at least three times. Report mean/p50/p90 as ranges.
- Each structural change gets a visit-count gate that is equal at two tree
  sizes, a full-diff completeness check and forced-full twin pixel/style
  parity. Add it to `tools/check-performance`.
- Compare failure-name sets against the baseline binary, never counts.
  Known: about 40–42 failures in `mesh-core-shell --lib`; frontend
  `phase87_layout_display_source_semantics_survive_lowering` and
  `shipped_settings_surface_compiles_with_local_pages`.
- If the exit criterion is not met, or the win is a few percent, stop and
  report rather than adding more scoping.

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
