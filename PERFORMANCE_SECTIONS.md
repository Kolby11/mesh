# MESH Performance Plan — by Codebase Section

Companion to `todo.md` (the canonical backlog). This file splits the codebase
into subsystems and, for each, lists what already shipped, what is still open
(with the `todo.md` section it lives in), and the recommended next step for
that subsystem. Work one section at a time, top to bottom inside each section.

Sections ordered by expected remaining win.

---

## 1. Component composition & template evaluation
`crates/core/shell/src/shell/component/composition.rs`, `crates/core/frontend/compiler`

Shipped: per-tree `StyleRuleIndex` on the build path (E), typed handler-call
args instead of JSON-in-attribute (G), `surface_css_props()` computed once per
paint (E), profiling-gated narrow diff (J).

Open — in priority order:
1. **Component-level render memoization** (I) — cache each embedded instance's
   built subtree keyed by (props fingerprint, `ScriptState::mutation_generation`,
   locale/theme generation) and reuse it wholesale. The single largest
   structural win; one reactive variable currently re-costs the whole surface.
2. **Affected-subtree template re-eval** (P1 → v1.27) — use
   `NodeServiceFieldDependencies` so only nodes whose tracked fields changed
   are re-evaluated instead of full template eval + diff.
3. **Typed expression values** (A) — `eval_expr` returns `String` for
   everything; numeric ops re-`parse::<f64>` both sides per evaluation
   (`compiler/src/expr.rs:208`) and float re-formatting causes false attribute
   dirtiness. Introduce a small bool/number/string value enum, stringify only
   at the attribute boundary.
4. **Handler write → binding dependency gating** (B) — record which public
   members templates actually bind and skip rebuilds for writes nothing binds
   to (extends v1.18 typed state dependencies).

### Section 1 deep dive — 2026-07-04 (new findings, not yet in todo.md)

Perf:
- **Full layout per embedded instance per rebuild.** `build_tree_with_state`
  always ends with `LayoutEngine::compute_with_measurer` (`compiler/src/lib.rs:203`),
  and `render_embedded_instance` calls it per embedded module instance
  mid-build; `finalize_tree` then re-lays-out the whole tree
  (`rendering.rs:460`). Embedded subtrees get ≥3 layout passes per rebuild
  (+1 per nesting level). Verify nothing reads `node.layout` between build and
  finalize, then skip layout in `FrontendRenderMode::Embedded` (and likely for
  the surface build too).
- **`{#for}` deep-clones the whole items array every rebuild.**
  `store.get(&for_node.iterable)` (`render.rs:429`) uses the owned `get` even
  though borrowed `get_ref` exists; switch to `get_ref`.
- **Three post-hoc full-subtree walks per embedded instance:**
  `namespace_event_handlers` re-`format!`s every handler string every rebuild
  (`interaction/src/hit_test.rs:359`) even though `build_widget_node` already
  receives `instance_key` — namespace at `parse_attributes` time instead;
  `apply_prop_handler_calls` clones each node's whole handler map and does an
  O(handlers × props) value-equality scan (`composition.rs:213-239`);
  plus the popover-root check.
- **Per-rebuild prop churn:** `ensure_runtime` re-`set`s every prop into script
  state per instance per frame (`runtime.rs:408-415`, 2–3 runtimes-mutex
  acquisitions each); `render_import` rebuilds `props_json` maps and
  `format!`s instance keys per frame; host+component rule slices are re-cloned
  into a merged Vec per instance (`render.rs:266-278`) — cacheable per
  (host, alias).
- **Per-node build allocations:** `_mesh_module_id` String inserted on every
  node (`attach_module_id`); `TrackingVariableStore` pushes two fresh Strings
  per dotted read; `resolve_event_handler_value` does an owned `store.get`
  per handler attribute. All fold into the v1.23 interning work but are worth
  keeping on this list since composition authors keep adding attributes.

Structure / correctness:
- **`and`/`or` diverge from Lua semantics.** `eval_compiled` returns literal
  `"true"`/`"false"` for `And`/`Or` (`expr.rs:193-204`) instead of the operand
  values — `{name or "Anonymous"}` renders `true`/`false`. Only the exact
  `cond and a or b` ternary shape works. Also `is_truthy` treats `"0"`/`""`
  as falsy (Lua doesn't), and `a or b and c` parses with wrong precedence
  (`and` is split before `or`). Fixing these naturally lands with the typed
  expression-value enum (§1.3).
- **Build is no longer a pure function.** `render_import` mutates shell state
  during build via RefCells (`pending_surface_states`,
  `portal_hidden_bindings`, `has_promoted_popover_wrappers`, live-binding
  installation). Component-level memoization (§1.1) will silently skip these
  side effects when serving a cached subtree — they must become explicit build
  outputs (a `BuildEffects` struct) before memoization can land.
- **Handler-call linkage matches by value equality.** `apply_prop_handler_calls`
  maps typed args onto child handlers by comparing resolved handler *values*
  to prop values; two props bound to the same handler mismatch args. Link by
  prop name through the child build instead.
- **Legacy JSON handler descriptors still parsed** in `unpack_handler_args`
  (`runtime.rs:644-664`) after typed `EventHandlerCall` landed — per the
  no-backward-compat rule, remove once nothing produces them.
- **`{#if}`/`{#for}` always wrap children in a synthetic `column` node**
  (`render.rs:394,423`) — an extra node per conditional/loop for layout,
  style, hash, and paint, and it forces column flow inside row parents.
  Needs a fragment/transparent-container concept.
- **No keyed list diffing.** `{#for}` identity is positional (`_mesh_key`
  paths), so any reorder/insert re-styles and re-hashes every following row.
  A `key=` attribute pairs naturally with component memoization.
- **Magic-string protocol at the composition boundary** (`__mesh_embed__::`,
  `__mesh_binding_*`, `__mesh_bind_this`, `_mesh_module_id`,
  promoted-popover marker) — the composition-boundary instance of the v1.23
  typed-fields work.
- Minor: `render_import`'s local branch does its catalog lookups twice
  (once for the `contains_key` gate, again inside `render_local_component`).

## 2. Retained tree, diff & display list
`shell/component/runtime_tree.rs`, `frontend/render/src/display_list.rs`

Shipped: shallow `Arc<Value>` state clones (A), snapshot inline `child_ids`,
primitive-aware hashing, `_mesh_key` hash removal (D), scratch-map reuse in
the display list (I), hash-chained runtime node IDs (J), metrics publication
gated on retained diff (A).

Open:
1. **Generation-aware diff / skip clean subtrees** (P1 → v1.27, D) — stop
   re-hashing every node's ~50 style fields + attributes per paint; dirty bits
   from the invalidation source should scope the fingerprint walk.
2. **String key-path allocation removal** (J) — IDs are chained now, but the
   slash-joined string path is still built per node per frame because
   interaction state and `refs` use it as the public key. Migrate those to
   `NodeId` and keep string paths for debug only.
3. **`WidgetNode` representation** (D, P2 → v1.23) — interned `Symbol`/`TagId`,
   small-vec attribute storage instead of two `BTreeMap<String,String>` per
   node, shell annotations (scroll offsets, focus flags, selection coords) as
   typed fields instead of formatted-float string attributes.
4. **Segment/rope display-list storage** (P1 → v1.21) — stop flattening
   retained subtrees into a new flat command buffer per update.
5. **`Arc<str>` display payload text** (I) — pointer-first comparison for
   `content`/`value`/`src`/`name` instead of per-entry string clones.

### Section 2 deep dive — 2026-07-04 (new findings; full list in todo.md §N)

The pipeline is annotate → `RetainedWidgetTree::update` → `RenderObjectTree`
→ `RetainedDisplayList` → damage. Key discoveries:

- **Three parallel diff systems fingerprint the same tree every dirty frame** —
  retained-tree snapshots, render-object paint-data slots, and display-list
  entry signatures (`collect_display_entries` re-hashes every node regardless
  of the dirty set). Unifying them around the retained tree's per-node dirty
  flags is the structural win for this section and folds into v1.27.
- **`ordered_entries` is release-build waste**: built per rebuild, consumed
  only by a `#[cfg(debug_assertions)]` metric (`display_list.rs:891-894`).
- **`render_object.rs` never received the D-item optimizations**: per-node
  String/Vec allocations, six string-parsed floats per node, fresh HashSets
  per update, byte-at-a-time hasher (the primitive-aware hasher was only added
  to `runtime_tree.rs`).
- **Scroll state round-trips float→string→float 3× per node per frame**
  (`"{:.2}"` annotation attrs re-parsed in three places) — the concrete typed-
  fields motivation.
- Reused paint subtrees clone twice per clean node; `build_command_spans` +
  effect-overflow count add two more full passes per rebuild.
- Structure: no NodeId collision detection (hash-derived ids, silent aliasing
  risk — add a debug assertion); `_mesh_key` identity-as-string-attribute
  forces special cases; three hand-rolled FNV hashers drifted apart.

## 3. Style system & theming
`ui/elements/src/style*`, `frontend/compiler/src/style.rs`, `foundation/theme`

Shipped: unified `css_property_table!`, cached class-token splits, indexed
build-path resolution (E), empty-diff no-op restyle, state-selector presence
gate for hover invalidation (K).

### Section 3 deep dive — 2026-07-04 (new findings; full list in todo.md §O)

- **Hidden second full restyle on every rebuild frame**: the runtime style
  diagnostics pass (always on in production) re-resolves every node through
  the diagnostics path, which still builds a `StyleRuleIndex` **per node** —
  the O(nodes × rules) pattern the build path already fixed. Likely the
  largest single win in this section.
- **Per-declaration static validation re-runs per node per pass**
  (profile status, supported-property, deprecated-token string scan) — all
  precomputable once per rule; the cheap first step toward typed declarations.
- Per-node allocation leaks in the resolver inner loop: module theme-token
  variable seeding (`format!` + `replace` per token per node), prop map
  cloning per node, `var()` token-name canonicalization per reference.
- **Correctness**: theme component defaults live in a `HashMap` and apply in
  nondeterministic order — overlapping shorthand+longhand defaults resolve
  randomly per run; theme-CSS source order is lost at parse.
- Structure: the diagnostics/no-diagnostics function-pair duplication is what
  let the per-node index rebuild survive on one path; fold into one path with
  a diagnostics sink parameter. No CSS specificity by design (source-order
  wins) — document in `docs/spec/04-styling.md`.

Second pass on §2 (same date, todo.md §N addendum): every rebuilt ancestor
copies its whole descendant command list into its own retained flat buffer
(O(n × depth) storage — the retained-memory face of v1.21); a dirty node
force-rebuilds its entire subtree's paint segments even for style-only
changes; `DisplayPaintCommand` embeds a cloned `DisplayPaintNode` per command
(share via `Arc`).

Open:
1. **Typed declarations end-to-end** (E, v1.23) — theme tokens resolve through
   `format!("{n}")` → re-`parse_px`/`Color::from_hex` string round-trips in the
   inner loop of build and restyle. Resolve tokens to typed values once per
   theme load; `apply_declaration` consumes typed values.
2. **Per-tag `ComputedStyle` prototypes** (E) — pre-bake theme component
   defaults once per theme change; start node resolution from a memcpy instead
   of re-applying string declaration maps per node.
3. **Selector-dependency analysis** (P0 → v1.18) — full-tree restyle avoidance
   for interaction changes beyond the shipped presence gate.

## 4. Rendering & paint
`crates/core/frontend/render` (painter, text, icons, debug overlay)

Shipped: multi-rect damage through the retained renderer (P1 → v1.20), shaped
single-pass ellipsis, opaque-region/blur wiring, checkbox/radio vector paint,
shared layout/paint text measurer.

Open:
1. **Fractional-scale partial damage** (D) — still the biggest visible lever on
   1.25×/1.5× outputs; the 2026-07-03 physical-damage experiment was
   byte-correct but showed no CPU win, so land compositor/SHM-upload damage
   instrumentation first (Tier 2/3 in L), then re-measure. Related memory:
   logical-vs-physical damage clip mismatch is the root cause.
2. **Text/glyph cache pressure visibility** (`TEXT_RENDERING_TODO.md`) — expose
   layout-cache entries/hits/misses/invalidations + shaping time in profiling
   output before further shaping changes; add locale/script/direction cases to
   canonical workloads. (Note: the ellipsis item in that file is stale — it
   shipped 2026-06-20 per todo.md.)
3. **Tile-parallel raster for large damage** (K) — band-split full-surface
   repaints; gate on a damage-area threshold, measure with v1.21 profiles.
4. **GPU backend** (P2 → v1.25) — wgpu/Skia-GPU with the retained display list
   as command source; only after damage/invalidation work stabilizes.

### Section 4 deep dive — 2026-07-04 (new findings; full list in todo.md §P)

- **Icon paints stat() files every frame** — `file_freshness` runs
  `fs::metadata` per file-backed icon per paint even on raster-cache hits
  (freshness is in the cache key), SVG twice, plus opaque-region derivation.
  Blocking syscalls in the paint hot path; TTL or inotify-driven invalidation.
- **Child popup surfaces bypass the retained pipeline** — full clear + 
  immediate-mode subtree repaint + two tree walks per child per present. An
  open popover repaints fully at frame rate. Route through the display-list +
  damage path.
- **Generation shortcuts only cover fully-clean frames**
  (`use_generation_shortcuts` needs `dirty_types.is_empty()`), so every
  interaction/animation frame runs the full render-object + entry-collection
  passes — the shell-side face of the §2 triple-fingerprint item.
- **Structure: every widget has two painter implementations** (immediate-mode
  + display-list twins for input/slider/icon/scrollbars/text) — silent drift
  hazard; converge on display-list and delete the twins once child surfaces
  migrate.
- Rotation transforms allocate a temp buffer + full subtree repaint per frame;
  gradient shader cache keys include absolute position (animated gradients
  thrash it). Text stack (layout LRU, glyph atlas, ellipsis cache) is healthy
  — no new text findings.

## 5. Interaction & input
`shell/component/input`, `ui/interaction`

Shipped: pointer/scroll coalescing, fused hit-test walk, take/restore instead
of tree deep-clone, single-walk hover dispatch, `NodeId` hover diff, O(1)
stale-target pruning, handler JSON-parse syntax gate, flag-gated neighbor
resync (B, J).

Open:
1. **Slider-drag reclassification** (J) — route drags through the
   STATE/interaction-restyle path (knob position is shell-owned
   `slider_values`) instead of SCRIPT invalidation → full rebuild per motion.
   Highest-leverage remaining input item.
2. **Per-paint hit-test/key index** (B follow-on) — a persistent flat index was
   rejected on rebuild cost; revisit only if the retained tree exposes a free
   key→node map (section 2 item 2 makes this nearly free).
3. **Instance-key interning in handler dispatch** (B) — remaining allocation
   in `call_namespaced_handler`.

### Section 5 deep dive — 2026-07-04 (new findings; full list in todo.md §Q)

- **Keyboard input reads settings files from disk per keystroke** —
  `current_keyboard_settings()` → `load_shell_settings()` does up to two
  file reads + JSON parses per KeyPressed/KeyReleased/Char event. Cache +
  invalidate via the existing settings watcher. The one serious find here.
- Click press/release still runs ~5–8 separate full-tree walks (the motion
  path was fused; clicks weren't) — extend `pointer_hit_test` to return
  focusable/kind/handler info. Scroll events similarly do two extra walks.
- Slider-drag `invalidate_script_state()` per motion confirmed at
  `input/mod.rs:193-200` (already tracked as §5.1/J).
- Interaction identity is string-keyed end to end (hover path, focus, scroll,
  input, slider maps) — the consumer side of the §2 NodeId migration.
- Otherwise this section is healthy: fused motion hit-test, single-walk hover
  dispatch, early-out scroll animations, probe-based pruning.

## 6. Script runtime & Rust↔Lua boundary
`crates/core/runtime/scripting`, `crates/core/runtime/backend`

Shipped: write-log `sync_state_from_lua`, pending-flag side channels, cached
lifecycle `self` table, proxy seen-field cache, shared-VM payload conversion
marker, borrowed `get_ref` template reads, host-value fingerprints (A, G, I).

Open:
1. **Per-thread VM with `_ENV` isolation** (P0 → v1.17) — one `mlua::Lua` per
   `ScriptContext` today; the shared-surface VM (Option B) landed for frontend
   surfaces, this tracks the remaining per-context VMs.
2. **Lazy `refs` field resolution** (A) — unchanged paints are now fully
   gated, but frames where metrics really changed still eagerly build JSON and
   convert to Lua; resolve fields on `__index` from a Rust-side store with a
   generation bump per paint.
3. **Storage read cost** (I) — the Lua-table cache prototype lost (0.8×);
   next attempt should share immutable JSON values or avoid the lock without
   adding a Lua lookup. Low priority until modules use storage per frame.

### Section 6 deep dive — 2026-07-04 (new findings; full list in todo.md §R)

- **`refresh_module_object` re-serializes full component state per handler**
  for every service-connected component (proxies defeat the generation skip
  AND the snapshot cache — deep JSON clone of every variable + proxy getters
  + full JSON→Lua conversion per handler/render). Feeds `module.state`, which
  docs mark as a legacy v1.12 lane no shipped module reads → verify + delete.
  Likely the largest remaining boundary cost.
- **The sync fast path still converts every known global Lua→JSON per
  handler** (write-log fixed discovery only). True fix needs a forwarding
  `_ENV` proxy (Luau `__newindex` doesn't fire on existing keys) or Rust-owned
  values — pairs with the v1.17 VM work.
- `snapshot()` with proxies bypasses caching entirely; exports sync runs even
  for components with no exports.
- Confirmed healthy: VM pool sandboxing, cached self table, flag-gated side
  channels, proxy seen-field cache, backend emit-only snapshots.

## 7. Events, services & backends
`foundation/events`, `extension/service`, `runtime/backend`, `shell/runtime/service_state.rs`

Shipped: `Arc<Event>` broadcast, wake-level coalescing with barrier semantics,
shell-boundary payload dedup, observation gating (C, K, P0).

Open:
1. **Shell-side subscription index** (C) — service → component-indices map
   invalidated on tracked-field changes, replacing the per-event
   O(components) mutex scan. The component-local summary experiment was
   rejected; the index design remains the viable one.
2. **Push-based backend host APIs** (C) — D-Bus signal subscribe, fd/socket
   watch, stream adoption (`pw-dump --monitor` for pipewire, P1) so exec
   polling becomes the fallback, not the mechanism.
3. **Channel-name interning** (C follow-on) — noted when `Arc<Event>` landed.

### Section 7 deep dive — 2026-07-04 (new findings; full list in todo.md §S)

- **`InterfaceRegistry::resolve` deep-clones the entire catalog per call** —
  every contract + provider map, cloned on every accepted state update, every
  named interface event, and every service command dispatch. Resolve under
  the read lock and return `Arc<InterfaceContract>`.
- **Contract validation re-derives typed info per event**: lowercased-String
  alloc per field per update; named-event schemas re-parsed from strings per
  event (also hand-rolled-parser debt). Precompile at contract registration.
- Interface-name canonicalization allocates 2–3 Strings per event.
- Structure: the hardcoded `mesh.audio` optimistic-mute branch located
  (`service_state.rs:66-75,137-165`) for the tracked service-specific-branch
  removal; replacement is contract-declared optimistic state.
- Confirmed healthy: boundary dedup, wake coalescing, backend-side dedup and
  stream batching, `Arc<Event>` bus.

## 8. Layout
`ui/elements/src/layout.rs`

Shipped: paint-only fast path skipping full style re-sync (F).

Open:
1. **Retain Taffy node state across passes** (P1 → v1.21) — `build_taffy_tree`
   still rebuilds a fresh TaffyTree on structural layout.
2. **Dirty-node-only style sync inside real layout passes** (F) — feed the
   retained-tree dirty set into `update_retained_node_styles` so one changed
   node doesn't re-convert every node's style. Depends on section 2 item 1
   exposing dirty IDs. (Scratch-map reuse prototype lost — don't retry that.)

### Section 8 deep dive — 2026-07-04 (new findings; full list in todo.md §T)

- **Unconditional `set_style` per node on layout-dirty frames defeats Taffy's
  internal caches** — one changed node makes Taffy recompute everything. The
  retained tree's per-node dirty flags are the ready-made fix (makes the F
  item pay off twice).
- **Text content String cloned twice per text node per pass** — into
  `TextMeasureData`, then again into the owned `TextMeasureKey` just to probe
  the cache (even on hits). `Arc<str>` + borrowed-key probe.
- **The LAYOUT-03 "key by String, not ephemeral NodeId" rationale is
  obsolete** — NodeIds are stable hash-chained now; re-keying `node_map` by
  NodeId removes all the structural-pass string clones and pairs with the §5
  interaction-map migration.
- Confirmed healthy: paint-only fast path, LRU-bounded intrinsic cache,
  measured-and-rejected scratch reuse.

## 9. Presentation & memory
`crates/core/presentation`

Shipped: surface-config change detection, region-cache by display-list
generation, popup reconcile early-outs and config equality (D, P1 → v1.20).

Open:
1. **Skia paints directly into mapped SHM** (H) — remove the extra full-buffer
   memcpy on full-present frames (`copy_bgra_to_canvas`), keeping `PixelBuffer`
   as the retained/compare copy only.
2. **SHM size-class pools** (H) — round buffer allocation up (next-64px) +
   viewport crop so animating content-measured surfaces stop reallocating all
   pool buffers per frame.

### Section 9 deep dive — 2026-07-04 (new findings; full list in todo.md §U)

- **Per-buffer pending damage is a single bounding rect, so the SHM copy is
  always a union** — two small disjoint changes on a bar (clock left, icon
  right) memcpy the whole span between them every frame, even though the
  `damage_buffer` calls are correctly per-rect. Make `pending_damage` a
  bounded rect list; pairs with the H direct-paint item.
- **kde_blur is re-created + re-committed per present while active** — the
  shell-side generation gate covers region *updates* only; the backend
  replays `set_region`+`commit` from stored state every frame. Needs the
  same last-committed/dirty pattern the input region already has.
- **Input normalization allocates a String per raw event via a linear
  surface scan** (`surface_id_for_wl_surface` find + clone per pointer/key
  event, re-allocated again in shell dispatch) — store surface ids as
  `Arc<str>`/numeric ids on `SurfaceEntry`.
- **Child popups force `force_full_present` every frame** — the
  presentation-side half of the §4 child-popup item; fixing the display-list
  side alone shows no SHM win.
- `wait_for_surface_configure` can block the whole frame loop for 10
  roundtrips on an unconfigured surface — bound by deadline or return
  not-ready.
- Confirmed healthy: pool reuse + busy-slot overflow, config fingerprint
  gating with the keyboard-only carve-out, popup-config equality gating,
  lazy input-region application, frame-callback 50 ms escape hatch,
  fd-based `wait_for_events`, boundary input coalescing.

## 10. Shell orchestrator, threading & startup
`crates/core/shell/src/shell/runtime`, `shell/discovery.rs`

Shipped: deadline-driven scheduler (no fixed 16 ms sleep), clean-surface render
bookkeeping gate, finalize-walk presence gates (P0, D, K).

Open:
1. **Parallel paint across surfaces** (K) — split `render_components` into a
   serial VM-bound phase (script/build/restyle/layout/display-list) and a
   parallel paint+SHM phase via rayon; painter caches are already
   `thread_local`. First threading step; do before pipelining.
2. **Paint/script pipelining** (K) — overlap frame N paint with frame N+1
   script work; classic guarded render loop. After item 1 proves the phase
   boundary.
3. **Parallel module discovery/compile at startup** (H) — `.mesh` parse +
   compile are pure per-module; rayon/spawn_blocking cuts perceived session
   start latency.
4. **Blocking IO off the shell thread** (K) — i18n catalog reads on mount,
   inline settings/theme reload reads, icon/SVG rasterize-on-miss stalling the
   frame; route through `spawn_blocking` + placeholder-then-fill.
5. **Fuse remaining unconditional finalize walks** (D) — note the 2026-07-04
   fusion prototype lost by scanning unrelated branches; any fusion must stay
   targeted.

### Section 10 deep dive — 2026-07-04 (new findings; full list in todo.md §V)

- **Every top-level surface holds a deep clone of the entire compiled
  frontend catalog.** `FrontendCatalog` (all `CompiledFrontendModule`s) is
  cloned per registered surface at startup, plus a per-entry clone in
  `top_level_surfaces()` — N surfaces means N+1 copies of every compiled
  module resident for the process lifetime. Wrap in `Arc`; same call site
  also deep-clones `interfaces.catalog()` per component (startup face of §7's
  resolve-clone item).
- **The §7 catalog deep-clone also fires per ServiceCommand dispatch**:
  `service_command_is_supported` + `service_command_is_coalescable` are two
  full-catalog clones per command (every throttled slider tick), plus one
  per throttled flush — retire these call sites with the §7 fix.
- Startup serial compile confirmed (H): manifest scan → per-module compile →
  backend spawn, all on the main thread; rayon over `from_modules` is the
  smallest first cut.
- Per-event allocation hygiene in `dispatch_wayland` (surface-id Strings ×2,
  per-request single-element VecDeques) and idle-loop clones in
  `render_components` — fold into v1.23 interning.
- Structure: `legacy_backend_candidates_from_discovery` is a duplicated
  compat lane behind graph-load failure — per the no-backward-compat rule,
  make a broken `config/module.json` a hard error and delete it, or document
  the degraded-boot requirement.
- Confirmed healthy: deadline-driven loop (reloads park 24 h on inotify,
  exact deadlines from throttles/ticks/hides), capped + barrier-correct
  message coalescing, lazy surface-index rebuild, TRACE-gated flush,
  eventfd-waking backend bridges.

## 11. Instrumentation & regression guard
`runtime/profiling.rs`, `render/src/surface/debug_overlay.rs`, tools

Shipped: Tier 0 Tracy live flamegraph (`perf-tracy`, `./tools/profile-shell live`).

Open (these de-risk everything above — run in parallel with sections 1–10):
1. **Tier 1 perf HUD in `DebugOverlay`** (L) — frame waterfall strip, live
   counters, damage paint-flashing. Paint flashing directly serves the
   fractional-damage decision (section 4 item 1).
2. **Tier 2 cause attribution** (L) — per-rule restyle time, per-instance build
   time (measures the memoization win before building it), per-command-kind
   paint time, wasted-work counters.
3. **Canonical workload profiles + CI baseline** (P1 → v1.21, L Tier 3) — the
   harness that keeps shipped wins from rotting.

---

## Cross-section attack order (updated 2026-07-04)

Of `todo.md`'s original 14-step order, steps 1, 3, 4, 5, 8, 9, 10 have shipped.
Remaining, re-ranked:

1. Slider-drag reclassification (§5.1) — small, bounded, kills the worst
   interaction case.
2. Perf HUD + damage instrumentation (§11.1–2) — cheap, unblocks the
   fractional-scale decision and measures §1.1 before building it.
3. Fractional-scale partial damage, re-measured (§4.1).
4. Shell-side event subscription index (§7.1).
5. Per-surface parallel paint (§10.1).
6. Component-level render memoization (§1.1) — plan with v1.18/v1.27
   dependency bookkeeping.
7. Typed declarations + typed expression values (§3.1, §1.3) — shared
   string-round-trip elimination.
8. `WidgetNode` interning/typed fields (§2.3) — feeds diff, input, and layout.
9. SHM direct paint + size classes (§9).
10. Pipelining, tile raster, GPU (§10.2, §4.3, §4.4) — after the phase split.
