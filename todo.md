~~Span takes defaultly full width of the parent component, the tags should initially take the space as possible. So defaultly the size of text inside~~

Icon rendering using icon packs

Settings module to manager modules and core settings like theme and i18n

Popups, also with custom content rendering if users desire

Keybind management

Layer system, so that we can specify what to render on what layer

make sure positioning system works (relative, absolute, fixed)

Variable state management and and binding for components

Clean up the backend modules and interfaces, right now interfaces are separate from the backend, we should check our options and consider moving the interface into the module itself

Remove the icon assts from the core/ui. The icons should be installed into a folder outside the core

# Separate milestons

- GPU rendering
- i18n configurations
- package manager
- lsp / extension
- unify configurations to use .json configuration
- Improve Icon packs
- Keyboard control with custom keybinds
- 



# Major performance fixes

# Shell performance improvement audit - 2026-05-27

Baseline from this audit:

- [ ] Treat the older performance notes below and `docs/performance-roadmap.md` as partially stale before executing them. The codebase already has persistent paint buffers, retained widget/render-object/display-list state, partial damage presentation, SHM buffer reuse, text/glyph/icon caches, and per-stage profiling.
- [x] Reconcile or archive `TEXT_RENDERING_TODO.md`. Replaced the stale placeholder-integration plan with current text-rendering performance notes covering the completed text/input/tooltip/selection paths and remaining cache/profiling work.

P0 - scheduling, invalidation, and full-tree work:

- [ ] Replace the fixed 16ms shell loop sleep with an event/deadline-driven scheduler. First pass replaces the unconditional sleep with a runtime deadline calculation in `crates/core/shell/src/shell/runtime/mod.rs`, using shell-message backlog, pending Wayland events, render needs, reload deadlines, throttled commands, and surface close transitions. Remaining work: block on real Wayland/frame-callback wakeups instead of bounded polling.
- [x] Pace presents with Wayland frame callbacks and coalesce pending presents per surface. The shell now treats an outstanding Wayland frame callback as a render throttle for visible surfaces, keeps pending render intent coalesced on the component, and uses a bounded fallback when compositors leave callbacks pending.
- [x] Move hot-reload polling out of the frame loop or gate it behind a dev-mode interval/watch service. First pass gates frontend source checks to 250ms, module settings checks to 500ms in `crates/core/shell/src/shell/runtime/reload.rs`, and theme/global shell settings checks to 500ms in `runtime/theme.rs`; a filesystem watcher can replace this later.
- [x] Add a `surface_id -> component index` lookup in the shell runtime. Event routing and focus/request handling now use a cached surface index with lazy rebuilds in `runtime/mod.rs`, `runtime/wayland.rs`, and `runtime/request.rs`.
- [x] Avoid per-loop Wayland flush bookkeeping when trace logging is disabled. `runtime/wayland.rs::flush_wayland` now skips the component/surface walk unless TRACE logging is active.
- [x] Pre-size pointer coalescing scratch map. `presentation/src/lib.rs::coalesce_pointer_moves` now sizes the pending-move map from the event batch.
- [x] Avoid redundant surface-size notifications during render. `runtime/render.rs` now calls `surface_size_changed` only when the resolved size differs from the cached known size.
- [x] Skip redundant surface configure calls. `ComponentRuntime` now caches the last `LayerSurfaceConfig`, `runtime/render.rs` only calls `presentation_engine.configure` when the config changes, and hidden renders invalidate the cache so remapped surfaces still configure correctly.
- [x] Remove redundant render-loop profiling flag write. `runtime/render.rs` no longer calls `set_profiling_enabled` twice in the same render attempt.
- [x] Skip idle component ticks. `ShellComponent::wants_tick` lets frontend components avoid per-loop `tick()` calls unless tooltip or portal state work is pending.
- [x] Stop continuous tooltip-delay repainting. Frontend components now track `tooltip_visible` so the delay transition invalidates paint once per hover target instead of every idle tick after the delay elapses.
- [x] Cap retained render scratch buffers. Reused painter command buffers now shrink after unusually large frames so normal shell rendering does not retain pathological allocations.
- [x] Tighten AccessKit snapshot traversal. Accessibility tree updates now pre-size node storage, discover focus during collection, and skip leaf child-vector allocations.
- [x] Avoid input text clones while painting. Text input widgets now borrow existing values/placeholders during measurement and rendering, allocating only for password masks.
- [x] Trim render-object diff churn. Retained render-object updates now pre-size tracking sets, avoid leaf child-id allocations, and compute geometry once per node.
- [x] Remove render-object display formatting. Material diff hashing now uses a compact display slot instead of allocating a debug string per node.
- [x] Remove built-in accessibility role strings from render-object diffs. Built-in roles now use compact slots, with allocation only for custom role names.
- [ ] Stop broadcasting every backend service event to every component. First pass adds `ShellComponent::observes_service_event` and lets frontend components skip services none of their script runtimes can read. Remaining work: route by finer subscription/tracked fields/module dependencies, then invalidate only affected paths.
- [x] Skip no-op per-runtime service update work. `FrontendSurfaceComponent::handle_service_event` now continues immediately for script runtimes without read capability instead of preparing previous/tracked state and cloning payloads for a no-op update.
- [x] Avoid repeated service capability string construction. `FrontendSurfaceComponent` now builds service/theme/locale read capability objects once per service event and reuses them across runtimes.
- [x] Drop identical service state updates before fanout. `runtime/service_state.rs` now compares incoming backend state with the latest stored payload/provider and skips delivery when nothing changed.
- [x] Stop forcing full-surface presents for every service state update. `runtime/service_state.rs` now lets retained paint damage determine the presented region after a component handles service events.
- [ ] Narrow script/service invalidation below "tree rebuild plus pixel repaint". `FrontendSurfaceComponent::invalidate_script_state` marks `TREE_REBUILD` and `surface_pixels_invalid` for every script state change. Add typed state dependencies so simple text/value changes dirty only the dependent nodes, style slots, layout slots, and paint slots.
- [x] Split visual dirtiness from metrics/accessibility dirtiness. `invalidation_requires_pixel_repaint` treats accessibility and element metrics as repaint reasons even when pixels do not change. Fixed: removed `ACCESSIBILITY` and `METRICS` from the pixel-repaint gate in `component.rs`.
- [ ] Avoid full-tree restyle for safe interaction changes. `component/rendering.rs` still restyles the retained tree for hover/focus/active because relationship selectors may affect descendants. Use selector dependency analysis: direct/simple selectors restyle only the affected node/path, while descendant/sibling selectors expand to the minimum affected subtree.
- [x] Cache `StyleRuleIndex` per module/theme/rule generation. `FrontendSurfaceComponent` now keeps `cached_restyle_rules` plus `cached_style_rule_index`, reuses the index across restyle passes, and clears both caches on source/module reload.
- [x] Avoid per-node selector candidate allocation in indexed restyle. `StyleRuleIndex` now exposes `for_each_candidate_rule()` backed by a thread-local scratch `Vec<usize>`, so hot style resolution visits sorted/deduped candidates without allocating a new candidate vector for every node.
- [ ] Replace per-node string/hash-heavy style matching with interned or typed node keys. First pass changed `StyleNodeAttrs` to borrow tag/id/key/module/class values during style resolution instead of cloning strings per node; remaining work is interned/typed tags, classes, and attribute keys.
- [x] Reduce per-node style attr allocation. `StyleNodeAttrs::from_node` now avoids the extra `StyleNodeAttrs::new` normalization pass and no longer builds a class `HashSet`; class checks use the existing class vector.
- [x] Remove style candidate hash-set allocation. `StyleRuleIndex::candidate_rules` now collects candidate rule ids into a vector, sorts, and deduplicates instead of hashing ids per node.
- [x] Remove child-vector churn during restyle recursion. `ui/elements/src/style/resolve.rs` no longer `mem::take`s every node's children while restyling; it recurses over `&mut node.children` directly.
- [ ] Make `IntrinsicLayoutCache` real and retain Taffy layout state. First pass implements a retained 512-entry LRU for intrinsic text measurements in `crates/core/ui/elements/src/layout.rs`, shared by text/style/width rather than node id. Remaining work: retain Taffy nodes/layout state instead of rebuilding a fresh tree and node maps every layout.
- [x] Avoid empty child-vector allocation for layout leaves. `ui/elements/src/layout.rs::build_taffy_tree` now only allocates a child vector for nodes that actually have children.
- [x] Share text measurement/layout cache between layout and paint. `FrontendRenderEngine` now uses `SharedTextMeasurer` for paint too, so layout and paint route through the same thread-local `TextRenderer` and share shaped-layout cache entries within the render thread.
- [x] Replace full tree cloning in focus transfer. `FrontendSurfaceComponent::receive_focus_transfer` now collects focus traversal from a borrowed retained tree and applies focus from that traversal instead of cloning `last_tree`.

P1 - renderer hot paths:

- [ ] Avoid flattening reused retained display-list subtrees into a new flat command buffer on each update. `RetainedDisplayList::update` can reuse subtree metadata, but parent/root command vectors still copy child command slices. Move toward segment/rope-style command storage or immutable shared command spans.
- [x] Omit display-list scrollbar commands for nodes that cannot show scrollbars. `build_paint_subtree` now emits scrollbar commands only when overflow style and scroll extents indicate a scrollbar can be visible.
- [x] Skip sparse command filtering for full-surface damage. `display_list.rs` now promotes a minimal-damage rect covering the whole surface to the full-surface selection path instead of scanning command spans.
- [x] Cap sparse multi-rect display-list filtering cost. `display_list.rs` now unions damage lists larger than 8 rects before command selection, reducing span-vs-rect checks when many small damage sources accumulate.
- [x] Add empty display-list selection fast paths. `display_list.rs` now returns immediately when there are no paint commands instead of allocating/filtering selected spans.
- [x] Avoid display-list child-order clone. `build_paint_subtree` now computes child order once, uses it for traversal, then stores it without cloning.
- [ ] Reduce retained render hashing and allocation. First passes replaced retained display-list primitive/batch and render-object material/primitive `DefaultHasher` signatures with compact deterministic hashers, and removed per-node primitive-slot `Vec` construction during display-list collection. Remaining work is string hashing, repeated attribute parsing, typed dirty fields, stable generation counters, and broader small fixed-array cleanup.
- [x] Batch Skia work through one surface/canvas per paint pass or per command batch. `SkiaPaintBackend::execute_commands` now wraps the pixel buffer once per command batch and replays rectangles, rounded rectangles, paths, images, gradients, shadows, and filters against that canvas, avoiding repeated Skia surface wrapping inside display-list batches.
- [x] Reuse painter scratch buffers. `FrontendRenderEngine` now owns persistent `RenderScratch` vectors for batched display-list commands and per-node painter command lowering, clears them between paints, and caps retained capacity to avoid one large frame pinning excess memory.
- [x] Reuse painter display-list scratch buffers. `FrontendRenderEngine` now keeps reusable batched/node command vectors for display-list replay, and `surface/painter/backend.rs` pre-sizes replay stacks with small expected capacities.
- [x] Make painter replay clip lookup O(1). `surface/painter/backend.rs` now stores cumulative clip rects on the clip stack instead of folding the full clip stack for every command.
- [x] Make painter replay layer lookup O(1). `surface/painter/backend.rs` now stores cumulative layer opacity/filter state so each draw command reads only the active layer instead of walking the full layer stack.
- [ ] Improve text ellipsis and clipping measurement. First pass adds a bounded 512-entry LRU for ellipsis truncation results in `surface/painter/text.rs`, avoiding repeated binary-search substring measurements for stable labels. Remaining work: compute truncation from shaped glyph advances instead of measuring substrings on first miss.
- [x] Replace random `HashMap` text layout eviction with a real bounded LRU. `surface/text.rs` now tracks layout cache recency explicitly, evicts the least-recently-used entry instead of an arbitrary map key, and raises capacity from 128 to 512 entries for multi-module shells.
- [x] Remove unnecessary text renderer locking from hot single-threaded paint paths or make the lock coarser. `TextRenderer` now uses `RefCell<TextEngine>` for local interior mutability instead of `Mutex<TextEngine>` around measure/render/selection paths.
- [x] Avoid cloned image/font data in icon and image paint. First pass returns cached image/font bytes through `Arc` in `surface/icon.rs::load_image_rgba` and `surface/glyph.rs::font_bytes`; second pass adds a thread-local `SKIA_IMAGE_CACHE` in `surface/painter/backend.rs` keyed by `Arc` pointer so `Data::new_copy` and `raster_from_data` are skipped on cache hits.
- [x] Cache SVG cacheability, raster source identity, and font-axis detection by path/freshness. Current icon paths may canonicalize/stat/read/parse source files on hot cache misses. Implemented bounded LRU raster source identity and SVG cacheability caches in `surface/icon.rs`, bounded LRU supported font-axis detection cache in `ui/icon/src/xdg.rs`, and LRU recency updates for the capped raster variant cache.
- [x] Cache XDG icon lookup results. `ui/icon/src/xdg.rs` now keeps a bounded 2048-entry LRU of pack/theme/name/size lookup results, including misses, so repeated pack and global-theme icon resolution avoids rebuilding XDG searches.
- [x] Bound and LRU the glyph cache. `surface/glyph.rs` now caps glyph variants at 1024 entries, refreshes recency on hits/inserts, and stores cached masks behind `Arc<[u8]>` so cache hits do not clone pixel vectors.
- [x] Bound and LRU the font-byte cache. `surface/glyph.rs` now caps cached font files at 32 entries with recency updates, avoiding unbounded font-file retention in long sessions.
- [x] Bound and LRU the decoded image cache. `surface/icon.rs` now caps freshness-aware decoded image entries at 256 with recency updates.
- [x] Avoid duplicate raster icon metadata reads. `surface/icon.rs::raster_file_key` now passes already-read file freshness into source identity caching instead of statting the path again.
- [x] Decode image cache misses outside the cache lock. `surface/icon.rs::get_or_load` now releases the image-cache mutex before disk/image decode work.
- [x] Avoid cloning icon codepoint maps and string cache keys. `ui/icon/src/xdg.rs` now uses a bounded 128-entry LRU codepoint-map cache and looks up a single codepoint instead of cloning the whole map per glyph lookup, and `ui/icon/src/registry.rs` uses a typed cache key instead of formatting `module::semantic` strings for every lookup.
- [x] Bound the central icon registry cache. `ui/icon/src/registry.rs` now caps semantic icon resolutions at 2048 LRU entries and clears cache order with generation bumps.
- [x] Bound missing-icon warning memory. `ui/icon/src/registry.rs` now caps the missing-icon warning suppression set at 2048 entries.

P1 - presentation and memory:

- [x] Audit SHM buffer release behavior under sustained frame backlog. The backend keeps a two-buffer pool and now caps fallback growth at three SHM buffers per surface, returning a bounded allocation error instead of growing memory without limit when all buffers are busy.
- [ ] Preserve and reuse surface configuration state more aggressively. First pass caches each component runtime's fixed/flexible surface size policy and reuses it in render/request config construction instead of querying the component every frame; remaining work is dirty-bit tracking so unchanged size/title/options skip config construction entirely.
- [ ] Track damage as multiple rects deeper into the retained renderer. Presentation already accepts region damage, but retained display-list damage often collapses node changes to a union rect. Keep separate dirty node rects longer to reduce overdraw.
- [ ] Add performance profiles for canonical shell workloads: idle shell, pointer move over dense tree, text update from backend, scrolling, large icon grid, animation, theme reload, and resize. Use existing stage timings to pin budgets for build/restyle/layout/render-object/display-list/paint/present.

P2 - larger architecture options:

- [ ] Consider a typed runtime node representation for hot paths. `WidgetNode` stores tag, attributes, event handlers, and content as strings/maps; keep source compatibility but compile into compact typed node/style/layout/paint records before runtime.
- [ ] Consider GPU rendering only after CPU invalidation and batching issues are fixed. The current biggest wins are less work per frame, not a different raster backend.
- [ ] Consider parallel layout/paint preparation after retained immutable snapshots are stable. Parallelism will not help much until broad invalidation and shared mutable caches are reduced.
- [ ] Add allocation counters to profiling output. Many current costs are likely allocation/string/hash driven and will not show clearly in wall-time-only stage metrics.

See `docs/performance-roadmap.md` for the durable roadmap.

# Additional audit findings - 2026-05-27 (codebase scan)

The items below were discovered by a follow-up scan after the original audit above. They are deliberately separated so the original ordering stays intact. Each entry: `file:line` — issue — why it costs — fix direction.

P0 - scripting hot path (highest impact):

- [ ] One `mlua::Lua` VM per script context. `crates/core/runtime/scripting/src/context/runtime.rs:92` allocates `Lua::new()` per `ScriptContext`. Each component instance pays the full stdlib + metatable cost; scales linearly with component count. Move to a per-thread VM pool or a single VM with environment isolation (`_ENV` per script), and lazy-init for inactive components.
- [x] Full global table walk on every state sync. `runtime/scripting/src/context/runtime.rs:957-973` (`sync_state_from_lua`) iterates `lua.globals().pairs()` after every handler call, filtering and serializing each user global. First pass: after initial load, `user_global_keys` tracks the discovered user globals and subsequent syncs use targeted `globals().get(key)` lookups instead of walking all stdlib+user pairs. Remaining: dynamic globals created inside handlers still miss the fast path (needs `__newindex` proxy or periodic scan).
- [x] Full snapshot re-serialize per render. `runtime/scripting/src/context/runtime.rs:1036-1042` (`refresh_module_object`) serializes the whole `state.snapshot()` and `lua.to_value()`s it every render, even when nothing changed. Hash or version the snapshot and skip refresh when unchanged; on change push only diffed keys into the Lua module table. First pass adds snapshot_generation tracking to ScriptState and skips rebuild when generation unchanged and no proxies present.
- [x] `ScriptState::snapshot()` rebuilds the JSON map every call. `runtime/scripting/src/context/state.rs:94-100` iterates `.keys()` and `.get()` per key per render. Cache the last snapshot and invalidate on `set`/`remove` only.
- [ ] Tracked-fields and side-channel maps cloned per state sync. `runtime/scripting/src/context/runtime.rs:202-203, 1021` clones whole `HashMap<String, InterfaceResolution>` and `HashMap<String, HashSet<String>>`. First pass avoids cloning tracked service field sets during shell service fan-out and skips `interface_bindings` clones unless import bindings changed; remaining work: wrap remaining side-channel maps in `Arc` and use copy-on-write, or return borrowed references.
- [ ] Bound instance proxy clones the full snapshot Value into Lua tables. `runtime/scripting/src/context/runtime.rs:284` (`install_bound_instance_proxy`) deep-clones the JSON state per component mount. Use a Lua userdata view backed by `Arc<Value>` or install a metatable proxy that reads on demand.

P1 - shell runtime and service routing (medium-high):

- [x] Service payload cloned twice per event. `crates/core/shell/src/shell/component/shell_component.rs:138` clones into `cached_service_payloads`, then line 161 clones again into `apply_service_update`. Store an `Arc<Value>` in the cache and pass the same Arc into the apply path.
- [x] `service_name_from_interface` called per runtime per event. `component/shell_component.rs:132` (and again in `observes_service_event` at ~line 186) recomputes the canonical service name. Cache one mapping `interface -> service_name` on the shell and pass the precomputed name to each component.
- [x] `Capability::new(format!("service.{name}.read"))` rebuilt per event. `component/shell_component.rs:135-139` formats three capability strings per event. Intern them in a `OnceLock<HashMap<&str, Capability>>` keyed by service name.
- [x] Service-event runtimes mutex held across the entire fan-out. `component/shell_component.rs:141-172` holds `self.runtimes.lock()` while doing capability checks, state diffs, payload clones, and tracked-field comparison for every runtime. Split into (a) collect target runtimes under lock, (b) drop lock, (c) apply in parallel or sequentially without contention.
- [x] Service state equality uses full `serde_json::Value::eq`. `runtime/service_state.rs:128` deep-compares whole payloads to detect changes. Add a fast-path content hash on insert and short-circuit equality by comparing hashes before recursing into Value::eq.
- [x] `wants_render` scan over all components every frame. `runtime/mod.rs:53` walks `components.iter().any(|r| r.component.wants_render())` per loop iteration. Maintain a dirty-bit counter incremented on `mark_render_needed` and decremented on render so the scheduler can read one atomic. First pass caches the post-render pending state while rendering, so the scheduler no longer performs its own component scan.
- [x] Coalesced shell message map allocates tuple keys per event. `runtime/mod.rs:277` builds `(interface.clone(), provider_id.clone())` for each entry. Intern interface/provider IDs as `Arc<str>` or `Symbol`, store a `(Symbol, Symbol)` 16-byte key instead of two heap strings.
- [x] Service state lookups clone keys before `.get()`. `runtime/service_state.rs:51, 108` does `.get(&(interface.clone(), source_module.clone()))`. Use the borrow-aware `RawEntry` API or change the map to nest `HashMap<Arc<str>, HashMap<Arc<str>, _>>`.
- [x] Replay materializes events into a Vec before iteration. `runtime/service_state.rs:295-308` does `.collect::<Vec<_>>()` then loops. Iterate `.values()` directly.
- [x] Module dirs cloned per discovery iteration. `shell/discovery.rs:119` does `for dir in self.module_dirs.clone()`. Iterate over `&self.module_dirs`.
- [x] Hot-reload PathBuf clones per check. `runtime/reload.rs:31, 84` clones `PathBuf` on every mtime mismatch. Borrow the path, or canonicalize once at registration.

P1 - retained tree and runtime tree (medium):

- [x] `runtime_tree` next-nodes HashMap allocated every paint. `component/runtime_tree.rs:90` does `let mut next_nodes = HashMap::new()` per update even when the tree shape is unchanged. Reuse a scratch HashMap on the runtime struct; `clear()` between frames keeps capacity.
- [x] Removed-id collection allocates intermediate Vec. `component/runtime_tree.rs:125-130` collects keys-to-remove into a Vec, then loops to remove. Use `HashMap::retain(|k, _| ...)`.
- [x] Snapshot cloned even when dirty flags are empty. `component/runtime_tree.rs:106-107` writes `*slot = next.clone()` on every visit. Compare first, clone only if a diff exists, or store snapshots behind `Arc` so cloning is cheap.
- [x] `cached_service_payloads` not pre-sized. `component/shell_component.rs:233` (`last_surface_states.insert` in a loop) and the payload cache do per-insert resizing. Reserve capacity from the manifest service list at runtime construction.
- [x] Damage rect helpers re-allocate `Vec<DamageRect>` per call. `component/rendering.rs` (`damage_rects_from_options`, `push_damage_rect`). Thread-local scratch or per-component reusable buffer.

P1 - style and layout hot paths:

- [x] `StyleRuleIndex` rebuilt for every restyle entry point. `crates/core/ui/elements/src/style/resolve.rs:313, 384, 394, 429` each call `StyleRuleIndex::new(rules)` per restyle. Cache by `(theme_generation, module_id)` and reuse across hover/focus/layout restyles. Already noted at the index level in the original audit; this lists the four call sites that need to share one cache.
- [x] `IntrinsicLayoutCache` LRU uses O(n) `VecDeque::retain` per access. `crates/core/ui/elements/src/layout.rs:94, 101` does `text_order.retain(|e| e != key)` on every hit and insert. Replace with `LinkedHashMap` or `lru::LruCache` (or a hand-rolled indexed map) for O(1) recency updates. Same anti-pattern as the glyph cache flagged in the original audit, but in the layout cache.
- [x] `EllipsisCache` LRU has the same O(n) retain. `crates/core/frontend/render/src/surface/painter/text.rs:27-31, 37-38, 394` mirrors the issue. Apply the same fix.
- [x] `FontBytesCache` and `GlyphCache` retain pattern. `crates/core/frontend/render/src/surface/glyph.rs:78, 85, 107, 114` — even after the LRU caps were added, the recency update is O(n). Switch to an indexed LRU.
- [x] `resolve_value` allocates an empty HashMap per call. `style/resolve.rs:146` calls `resolve_value_with_variables(value, &HashMap::new())` every invocation. Use a `static EMPTY: OnceLock<HashMap<...>> = ...` or change the signature to accept `Option<&HashMap<...>>`.
- [x] Variable map rebuilt per node in `resolve_node_style_with_attrs_indexed`. `style/resolve.rs:325, 363` reseeds `HashMap::new()` for every node. Thread-local scratch HashMap cleared per call avoids repeated alloc/free cycles.
- [x] `selector_to_diagnostic_string` allocated in the hot rule-matching loop. `style/resolve.rs:345, 1172-1185` uses `format!()`/`join("")` per matched rule even though the string is only consumed by diagnostics. Gate behind `cfg(debug_assertions)` or skip unless `diagnostics_enabled`.
- [x] `decl.property.clone()` repeated per matched declaration. `style/resolve.rs:520, 531, 542, 555, 565, 578, 589` clones the property name into diagnostics seven separate places inside `apply_declaration`. Hoist diagnostics out of the apply path or borrow the property name.
- [x] `active_state_names` iterator rebuilt per node. `style/resolve.rs:112, 648` iterates state names for each candidate-rules call. Precompute a `u32` state mask once on `StyleNodeAttrs` and match rules against the mask.
- [ ] `StyleNodeAttrs::from_node` re-splits class strings per restyle. `style/resolve.rs` no longer clones class/tag/id/key/module strings into `StyleNodeAttrs`, but it still splits the class attribute and allocates a borrowed class vector per node. Cache the split classes on the retained `WidgetNode` once attribute mutation is funneled through an invalidating API.
- [x] `parse_edges_shorthand` allocates a Vec per call. `style/parse.rs:4, 691-696` does `split().map().collect()` for every margin/padding/border parse. Bound to 4 values with a fixed `[Option<f32>; 4]` and parse in place.
- [x] `parse_transform` splits the args string twice. `style/parse.rs:29-107` splits whitespace for both function arg detection and per-function parsing. Single split + slice.
- [x] Container query check inside per-rule loop. `frontend/compiler/src/style.rs` now caches a compact inherited-style rule index and partitions inherited-property candidates into non-container and container-query buckets, so most nodes skip container-query checks entirely.
- [x] `inherited_style_mask` rebuilt per child. The mask is still child-specific by CSS semantics, but it now reuses a thread-local rule index that stores only rules declaring inherited text properties and their precomputed mask bits, avoiding full declaration scans per element.
- [x] `selector_matches` for Compound selectors does not short-circuit. `frontend/compiler/src/style.rs:68-86` already uses `parts.iter().all(...)`, so compound matching short-circuits.
- [x] `merge_missing_defaults` does inline string `==` matching on tag. `frontend/compiler/src/style.rs:114-152` now groups tag checks through `match` arms, keeping the later `TagId` interning path straightforward.

P1 - renderer hot paths (additional):

- [x] Per-pixel software rounded-rect fallback. The Skia painter batch path no longer falls back to the nested `for py / for px` coverage loop; rounded fills/strokes are rendered through the already-wrapped Skia canvas for the active command batch.
- [x] Linear-gradient shader rebuilt per draw. `surface/painter/backend.rs:973-1026` constructs a new Skia shader on every `DrawLinearGradient`. Cache by `(stops, direction, bounds_hash)` for repeated panel/background gradients.
- [x] Layer paint alpha recomputed per command. `surface/painter/backend.rs` now stores compact active-layer replay entries with pre-resolved opacity alpha and inherited filter state, so draw commands avoid repeated opacity clamping and float alpha recomputation while replaying layered command batches.
- [x] Diagnostics Vec sized to `min(8)`. `surface/painter/backend.rs:409-410` pre-allocates replay scratch stacks with very small capacities; clip/layer stacks now start at `min(32)` / `min(8)` to avoid common regrowth.
- [x] Skia `Data::new_copy` per image draw. `surface/painter/backend.rs:1058` (`draw_image_command`) copies full RGBA bytes into a Skia `Data` every frame the icon appears. Fixed: thread-local `SKIA_IMAGE_CACHE` keyed on `Arc<RgbaImage>` pointer; cache hits reuse the `skia_safe::Image` handle without pixel copies.
- [x] Text renderer mutex around measure/render. Repeated in `surface/text.rs` and `surface/painter/text.rs`. The local renderer now uses `RefCell<TextEngine>` instead of `Mutex<TextEngine>` on the single-threaded paint path.
- [x] Cache keys built from `to_string()` + `.to_bits()` per call. `surface/painter/text.rs:335, 336, 340, 362` now hashes borrowed text/font inputs for ellipsis lookup and stores owned verifier fields only on cache insert.
- [x] Text layout cache key construction clones strings on lookup. `surface/text.rs:68-69` and `surface/painter/text.rs:335-336` now use borrowed hashed lookup keys with owned verifier fields stored only on cache insert.
- [x] Icon raster lookup calls `file_freshness` twice on miss. `surface/icon.rs:298, 303` now threads SVG cacheability freshness into raster key construction and avoids the extra `path.exists()` stat in the draw path.
- [x] `PathBuf` allocations in icon/font cache insertions. `surface/icon.rs` image/source/SVG cache keys and `surface/glyph.rs` font-byte cache keys now store `Arc<Path>` and borrow `&Path` on lookup, avoiding repeated owned path allocation on hot cache hits. `resvg` resource-dir setup still keeps its required one-off owned path during SVG rasterization.
- [x] SVG cacheability parser scans the file each cache miss. `surface/icon.rs:334-356, 358-373` stores the parsed cacheability flag with the file freshness key in the existing SVG cacheability LRU.
- [x] Display-list iteration indirect borrow. Retained display lists now keep a parallel compact `DisplayPaintCommandKind` stream, and sparse selected replay iterates `(command, kind)` through `SelectedDisplayListPaint::iter_with_kinds()` so the hot repaint loop no longer reloads the discriminant through each full command struct.

P1 - presentation, foundation, registry:

- [x] Lua exec / interface proxy dedup is O(N²). `crates/core/runtime/scripting/src/host_api.rs:91-105` (`InterfaceProxy::available_interfaces`) does `.contains()` on a `Vec<String>` per capability. Use a `HashSet<&str>` then collect.
- [x] `InterfaceProxy::can_read` formats capability strings per check. `runtime/scripting/src/host_api.rs:121-137` builds `format!("service.{name}.read")` on every read attempt. Intern per-service capability values once at script start.
- [x] `HostApiManifest::from_capabilities` double-iterates with extra format!(). `runtime/scripting/src/host_api.rs:52-67`. Collect once with `strip_prefix` + `split` in a single pass.
- [x] Backend `coalesce_command_batch` builds a fresh `HashMap` per batch. `crates/core/runtime/backend/src/lib.rs:28-33, 179-181`. Reuse a `HashMap` on the backend service with `clear()`.
- [x] Backend events clone `service_name` and `module_id` per emission. `runtime/backend/src/lib.rs:144-159`. Backend service events now carry `Arc<str>` service/module identifiers, and the shell bridge avoids cloning command-result/interface payload fields before forwarding them.
- [x] Static command results re-emit `serde_json::json!()`. `runtime/scripting/src/backend/command.rs:18, 30` rebuilds the ok/error JSON shells per call. Successful nil command results now clone a cached static JSON envelope and error results use a pre-sized map instead of the macro path.
- [x] `EventBus::subscribe` locks Mutex and allocates per subscriber. `crates/core/foundation/events/src/lib.rs:36-42` now uses an `RwLock` read fast path for existing channels and takes the write lock only to install a missing channel.
- [x] `EventBus::publish` always takes the Mutex even on miss. `foundation/events/src/lib.rs:46-51` now uses a read lock around the channel lookup and sender send path.
- [x] `Theme::token` walks nested HashMaps per access. `Theme` now precomputes a flat token lookup table at theme load, including explicit module-scoped token keys, so style token resolution uses one hash lookup on the hot path while preserving the public nested maps.
- [x] `LocaleEngine::translate_with` chains `.replace()` per placeholder. `crates/core/foundation/locale/src/lib.rs:83-90` now uses a single-pass formatter over the template and preserves unknown placeholders literally.
- [x] `ServiceRegistry::register` uses `Vec::retain` to dedup. `crates/core/extension/service/src/registry.rs:70-76` now stores entries in a `HashMap<String, ServiceEntry>` keyed by service type and replaces directly.
- [x] SHM buffer pool double-iterates per present. `SurfaceEntry` now caches the active SHM pool geometry and clears/rebuilds the pool only when the requested width/height/stride changes, avoiding the per-present validation scan.
- [x] `SurfaceEntry::needs_reconfigure` does field-by-field equality. Presentation now stores a compact surface configuration fingerprint and compares it on configure, with keyboard-mode reapply paths keeping the fingerprint in sync.
- [x] Pointer-move coalescing iterates events twice (insert + drain). `coalesce_pointer_moves` now keeps the common single-surface pending move in one slot and promotes to a `HashMap` only when multiple surfaces have concurrent pending moves.

P1 - interaction / hit-test:

- [x] `find_node_path_at` uses `Vec::insert(0, ...)` during recursion. `crates/core/ui/interaction/src/hit_test.rs:78, 80`. Current hit testing already builds the path bottom-up and reverses once at the root.
- [x] `node_tooltip_text` does up to 5 attribute lookups + clones. `ui/interaction/src/hit_test.rs:98-107`. Tooltip text selection now borrows through the fallback chain and clones only the selected non-empty string.
- [x] `find_tooltip_by_key_with_inherited` clones inherited tuple per child. `ui/interaction/src/hit_test.rs:138`. Inherited tooltip owner/text is now passed by reference and cloned only for the matching key.

P2 - architecture / measurement:

- [ ] Introduce interned string types (`Symbol`, `TagId`) before further string-key fixes. Almost every "clone string" finding above resolves cleanly once tags, classes, attribute names, interface names, service names, and module IDs share a single global interner (`lasso` or hand-rolled).
- [ ] Add an allocator-level profile mode that counts allocations per render pass, so the LRU/clone/format issues above can be ranked by actual frame cost rather than guessed impact.
- [ ] Consider replacing the per-component `Lua` VM with a single VM that uses environment isolation (`setfenv` / `_ENV`) — saves the largest per-component cost and unblocks shared compiled chunks.

Current retained-rendering status:

- Stable runtime node IDs are implemented from `_mesh_key`.
- Style-only renders now mutate the retained cached `WidgetNode` tree instead of
  cloning it.
- Retained widget-tree dirty summaries now track inserted, removed, layout,
  style, attribute, child-order, and state changes by stable node ID.
- Full dirty renders still rebuild the widget tree.
- There is not yet a retained render-object tree, retained display list,
  incremental layout consumer, or damage tracking.

Implementation order:

1. Retained widget tree with stable node identity and dirty summaries. Done for
   the widget layer.
2. Dirty-type invalidation for script/state, style, layout, paint, text,
   accessibility, metrics, and surface configuration.
3. Incremental style and layout propagation.
4. Retained display list plus damage tracking.
5. Text shaping and glyph cache.
6. Typed attribute/style slots and interned identifiers.
7. Selector indexing for restyle.
8. Display-list batching.
9. GPU backend through wgpu/Vulkan/OpenGL.
10. Parallel paint/layout after ownership bottlenecks are removed.

Key sequencing note: adding a GPU backend before retained rendering gives
diminishing returns, because rebuilding the tree and uploading brand-new paint
data every frame wastes much of the GPU win. The right sequence is retained
tree, dirty invalidation, damage tracking, retained display list, then GPU.

Two ways to fix the correctness bug                                                           
                                                                             
  A. Cheapest, restores correctness: drop partial damage and always go through the full-clear + 
  full-paint branch. Keeps the tree_has_icon + clear_rect wins from the last pass, surrenders
  the partial-paint speedup. ~5 lines deleted.                                                  
                                                                             
  B. Proper: move the PixelBuffer ownership into FrontendSurfaceComponent (or a per-surface slot
   in Shell) so it persists across paints. Reallocate only on size change. Then the
  partial-damage branch actually reuses old pixels, and damage tracking becomes a real          
  optimization. Also enables sending wl_surface.damage_buffer regions to the compositor instead
  of treating every commit as a full surface change.

  I'd recommend B — A is a regression band-aid.                                                 
   
  Why "overall rendering is still slow" — gap to Qt                                             
                                                                             
  Even after fixing damage retention, the pipeline is software-rasterizing everything on the    
  CPU, single-threaded, and reallocating per frame. The big items, ordered by impact:
                                                                                                
  1. Per-frame full surface allocation — PixelBuffer::new zeroes a fresh Vec<u8> of W*H*4 bytes 
  every render call. For a 1920×48 panel that's ~370 KB of Vec allocation + zeroing every frame
  just to throw it away. Same fix as B above.                                                   
  2. Per-frame wl_buffer allocation — wayland_surface/backend.rs:335 calls   
  pool.create_buffer(...) and copies pixels in every present. Wayland clients normally maintain 
  a 2–3 buffer SHM pool and recycle them on wl_buffer::release. Right now each frame is a fresh
  shm allocation + memcpy.                                                                      
  3. Tree-walking and command rebuild every paint — collect_paint_commands clones every
  WidgetNode (paint_node = node.clone()) into a Vec<DisplayPaintCommand> on every paint, even   
  when the tree hasn't changed. The retained-tree generation is already tracked; gate command
  list rebuild on it.                                                                           
  4. Software path for everything — borders, rounded corners, gradients, shadows, glyph blending
   all go through set_pixel/blend_pixel with bounds checks per pixel. Qt is GPU-accelerated via 
  QRhi/Vulkan/OpenGL. Two practical options:
    - Tiny-skia upgrade: keep the CPU pipeline but route geometry through tiny_skia::PixmapMut  
  (already a transitive dep through resvg). Tiny-skia is SIMD-aware and ~10–50× faster than     
  hand-rolled set_pixel loops for primitives.
    - GPU: replace PixelBuffer with a wgpu-backed renderer (use dmabuf linux-dmabuf-v1 instead  
  of shm). This is the actual Qt-parity path but it's a multi-week change.                      
  5. Glyph atlas — text is currently re-rasterized + cached per-string. Qt keeps a per-font GPU
  glyph atlas keyed on (font, glyph_id, subpixel-position, weight). On the CPU side, even a     
  HashMap<(FontId, char, subpx), Bitmap> glyph cache (vs. string-level cache) would help.
  6. Image cache hands out clones — crates/core/ui/icon's RgbaImage::clone() per lookup is a    
  full pixel copy. Should hand out Arc<RgbaImage>.                                              
  7. No frame-throttling to compositor frame callbacks — paints fire on Tokio ticks, not on
  wl_surface::frame callbacks. You can paint at 1000 Hz and it just gets dropped, while still   
  burning CPU.                                                               
                                                                                                
  My recommendation                                                                             
                                         
  Order I'd attack this in:                                                                     
                                                                             
  1. Fix the disappearing-surface bug now — Option B (persistent buffer in component or shell). 
  Without this, none of the rest matters.
  2. Reuse SHM buffers — at least 2-deep pool; biggest single win after #1, no API change.      
  3. Gate collect_paint_commands and RenderObjectTree work on retained-tree generation — skip   
  command/object rebuild when nothing changed.                                                  
  4. Route primitives through tiny_skia — keeps the architecture, enormous CPU paint speedup.   
  Already half-pulled-in via resvg.                                                             
  5. Frame-callback driven paint — paint when the compositor asks, not on a timer.
  6. (Long-horizon) wgpu backend — actual Qt parity.                                            
                                                                                                
  Want me to start with (1) — make the buffer persistent so the shell stops vanishing — and we  
  can stage the rest after? That's a contained change in render.rs + FrontendSurfaceComponent   
  and unblocks evaluating the partial-damage path on its own merits.  

    What that means for MESH               

  You don't need a GPU to be Qt-fast. You need:                                                                    
   
  1. A SIMD-aware software rasterizer (tiny-skia gets you there for free).                                         
  2. A glyph atlas / cache keyed on (font, glyph, subpixel) — Qt has one, you don't yet.
  3. Real damage-region tracking (the work you started, once the persistent-buffer bug is fixed).                  
  4. Fewer per-frame allocations (SHM pool, no tree clones).                                                       
                                                                                                                   
  GPU only becomes necessary when you want effects Qt's software path also drops: heavy blurs, large filter chains,
   very high-resolution surfaces (4K+ at 120 Hz). For a panel, launcher, notification center on a normal monitor, a
   tuned CPU pipeline is enough — that's exactly the regime Qt's software backend was built for, and it's smooth.  
                                                                             
  So: tiny-skia is the right next step after fixing the disappearing-surface bug. It buys you most of Qt's no-GPU  
  performance without committing to wgpu.

## Performance Improvements

- Severity: High
  Subsystem affected: Skia painter / retained display-list replay
  Likely impact: Lower CPU frame time when painting batches with many primitives, especially dense panels and popovers.
  Recommended optimization: Continue moving text and icon draw paths into the same batch-level canvas replay where possible, or split command batches by renderer boundary so Skia-backed primitives never re-wrap the pixel buffer per draw.
  Estimated complexity: Medium

- Severity: High
  Subsystem affected: Wayland presentation / frame pacing
  Likely impact: Lower CPU usage and smoother pacing under compositor back-pressure.
  Recommended optimization: Make `wl_surface::frame` callbacks the render permit per surface, and coalesce dirty surfaces while a frame callback is pending instead of rendering and dropping frames.
  Estimated complexity: High

- Severity: High
  Subsystem affected: Layout / retained tree
  Likely impact: Major reduction in CPU time for stable surfaces with text-heavy trees.
  Recommended optimization: Retain Taffy node state and node-id mappings across layout passes so layout updates mutate existing Taffy nodes instead of rebuilding the full tree.
  Estimated complexity: High

- Severity: Medium
  Subsystem affected: Text layout and paint
  Likely impact: Reduced shaping and measurement duplication for common labels and inputs.
  Recommended optimization: Share one text layout cache between intrinsic layout measurement and paint, keyed by text/font/width/style, with generation-aware invalidation.
  Estimated complexity: Medium

- Severity: Medium
  Subsystem affected: Display-list storage
  Likely impact: Lower allocation and memory bandwidth during dirty subtree updates.
  Recommended optimization: Replace flattened command-vector rebuilds with immutable command segments or a rope-like retained command store so clean child spans are referenced instead of copied into parent vectors.
  Estimated complexity: High

- Severity: Medium
  Subsystem affected: Style matching
  Likely impact: Lower restyle cost for hover/focus/active transitions.
  Recommended optimization: Add selector dependency metadata and restrict direct/simple interaction restyles to affected nodes and required descendants, expanding only for relationship selectors.
  Estimated complexity: Medium

- Severity: Medium
  Subsystem affected: Image/icon caches
  Likely impact: Less cache churn and fewer path allocations in icon-heavy surfaces.
  Recommended optimization: Store file cache keys as interned `Arc<Path>`/symbols and avoid repeated `PathBuf` allocation on hot cache lookups.
  Estimated complexity: Low

## Refactoring Opportunities

High Impact

- Rendering command replay is still split across batchable Skia primitives and helper paths for text/icons/widgets. Introduce explicit renderer boundaries in the display-list command model so each backend can replay contiguous spans with clear ownership and profiling attribution.
- `FrontendSurfaceComponent::paint` coordinates dirty flags, retained tree sync, render object sync, display-list update, damage selection, painting, profiling, metrics, tooltip damage, and measured-size updates. Split it into small pipeline stages with typed inputs/outputs so each stage can be tested and profiled independently.
- Shell scheduling, Wayland event dispatch, pending requests, throttled backend commands, render, present, and sleep calculation all live in the runtime loop. Extract a frame scheduler that owns deadlines, render permits, and pending-surface state.

Medium Impact

- `display_list.rs` remains a large module containing retained storage, damage math, primitive extraction, batching metrics, subtree pruning, and tests. Split retained storage, damage selection, primitive extraction, and batching into separate modules.
- Painter backend diagnostics and production draw paths share the same command loop. Separate validation/diagnostic passes from hot replay so normal rendering does not carry diagnostic-specific branching when diagnostics are disabled.
- Style resolution mixes hot matching, diagnostics, variable resolution, selector indexing, and cache ownership. Split diagnostic construction from match/apply and make cache ownership explicit at the component/style-generation boundary.

Low Impact

- Several hot-path APIs still pass many scalar arguments for text, icon, and widget drawing. Group stable paint parameters into compact structs to reduce call-site churn and make batching compatibility easier to reason about.
- `todo.md` contains older notes that now conflict with implemented retained rendering and SHM reuse. Archive stale historical discussion into a dated notes document so the active backlog stays executable.
                                             
