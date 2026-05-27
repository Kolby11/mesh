Span takes defaultly full width of the parent component, the tags should initially take the space as possible. So defaultly the size of text inside

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
- [ ] Reconcile or archive `TEXT_RENDERING_TODO.md`. The current renderer already has shaped text rendering, layout measurement, glyph caching, and text paint paths; the remaining issues are cache sharing, eviction, and hot-path cost.

P0 - scheduling, invalidation, and full-tree work:

- [ ] Replace the fixed 16ms shell loop sleep with an event/deadline-driven scheduler. First pass replaces the unconditional sleep with a runtime deadline calculation in `crates/core/shell/src/shell/runtime/mod.rs`, using shell-message backlog, pending Wayland events, render needs, reload deadlines, throttled commands, and surface close transitions. Remaining work: block on real Wayland/frame-callback wakeups instead of bounded polling.
- [ ] Pace presents with Wayland frame callbacks and coalesce pending presents per surface. `crates/core/presentation/src/wayland_surface/backend.rs` can still commit while a frame callback is pending after a nonblocking dispatch. Use callbacks as the main render permit so the shell does not overproduce frames.
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
- [ ] Split visual dirtiness from metrics/accessibility dirtiness. `invalidation_requires_pixel_repaint` treats accessibility and element metrics as repaint reasons even when pixels do not change.
- [ ] Avoid full-tree restyle for safe interaction changes. `component/rendering.rs` still restyles the retained tree for hover/focus/active because relationship selectors may affect descendants. Use selector dependency analysis: direct/simple selectors restyle only the affected node/path, while descendant/sibling selectors expand to the minimum affected subtree.
- [ ] Cache `StyleRuleIndex` per module/theme/rule generation. `crates/core/ui/elements/src/style/resolve.rs` rebuilds rule indexes and allocates selector candidate sets during restyle.
- [ ] Replace per-node string/hash-heavy style matching with interned or typed node keys. `StyleNodeAttrs::from_node` splits classes and allocates strings/sets for every node during restyle.
- [x] Reduce per-node style attr allocation. `StyleNodeAttrs::from_node` now avoids the extra `StyleNodeAttrs::new` normalization pass and no longer builds a class `HashSet`; class checks use the existing class vector.
- [x] Remove style candidate hash-set allocation. `StyleRuleIndex::candidate_rules` now collects candidate rule ids into a vector, sorts, and deduplicates instead of hashing ids per node.
- [x] Remove child-vector churn during restyle recursion. `ui/elements/src/style/resolve.rs` no longer `mem::take`s every node's children while restyling; it recurses over `&mut node.children` directly.
- [ ] Make `IntrinsicLayoutCache` real and retain Taffy layout state. First pass implements a retained 512-entry LRU for intrinsic text measurements in `crates/core/ui/elements/src/layout.rs`, shared by text/style/width rather than node id. Remaining work: retain Taffy nodes/layout state instead of rebuilding a fresh tree and node maps every layout.
- [x] Avoid empty child-vector allocation for layout leaves. `ui/elements/src/layout.rs::build_taffy_tree` now only allocates a child vector for nodes that actually have children.
- [ ] Share text measurement/layout cache between layout and paint. Layout uses `SharedTextMeasurer` with a separate thread-local `TextRenderer`; paint uses the frontend renderer's `TextRenderer`. This can shape/measure the same text twice in one frame.
- [x] Replace full tree cloning in focus transfer. `FrontendSurfaceComponent::receive_focus_transfer` now collects focus traversal from a borrowed retained tree and applies focus from that traversal instead of cloning `last_tree`.

P1 - renderer hot paths:

- [ ] Avoid flattening reused retained display-list subtrees into a new flat command buffer on each update. `RetainedDisplayList::update` can reuse subtree metadata, but parent/root command vectors still copy child command slices. Move toward segment/rope-style command storage or immutable shared command spans.
- [x] Omit display-list scrollbar commands for nodes that cannot show scrollbars. `build_paint_subtree` now emits scrollbar commands only when overflow style and scroll extents indicate a scrollbar can be visible.
- [x] Skip sparse command filtering for full-surface damage. `display_list.rs` now promotes a minimal-damage rect covering the whole surface to the full-surface selection path instead of scanning command spans.
- [x] Cap sparse multi-rect display-list filtering cost. `display_list.rs` now unions damage lists larger than 8 rects before command selection, reducing span-vs-rect checks when many small damage sources accumulate.
- [x] Add empty display-list selection fast paths. `display_list.rs` now returns immediately when there are no paint commands instead of allocating/filtering selected spans.
- [x] Avoid display-list child-order clone. `build_paint_subtree` now computes child order once, uses it for traversal, then stores it without cloning.
- [ ] Reduce display-list hashing and allocation. Primitive signatures use `DefaultHasher`, per-node `Vec` slot construction, string hashing, and repeated attribute parsing. Use typed dirty fields, stable generation counters, small fixed arrays, and a faster hash only where hashing is still needed.
- [ ] Batch Skia work through one surface/canvas per paint pass or per command batch. `PixelBuffer::with_skia_canvas` wraps the raw buffer for many individual primitives; `SkiaPaintBackend::execute_commands` still calls into per-command helpers that recreate canvas wrappers.
- [ ] Reuse painter scratch buffers. Display-list painting allocates command vectors and clip/layer stacks per paint call. Store scratch vectors on the renderer/backend and clear them between paints.
- [x] Reuse painter display-list scratch buffers. `FrontendRenderEngine` now keeps reusable batched/node command vectors for display-list replay, and `surface/painter/backend.rs` pre-sizes replay stacks with small expected capacities.
- [x] Make painter replay clip lookup O(1). `surface/painter/backend.rs` now stores cumulative clip rects on the clip stack instead of folding the full clip stack for every command.
- [x] Make painter replay layer lookup O(1). `surface/painter/backend.rs` now stores cumulative layer opacity/filter state so each draw command reads only the active layer instead of walking the full layer stack.
- [ ] Improve text ellipsis and clipping measurement. First pass adds a bounded 512-entry LRU for ellipsis truncation results in `surface/painter/text.rs`, avoiding repeated binary-search substring measurements for stable labels. Remaining work: compute truncation from shaped glyph advances instead of measuring substrings on first miss.
- [x] Replace random `HashMap` text layout eviction with a real bounded LRU. `surface/text.rs` now tracks layout cache recency explicitly, evicts the least-recently-used entry instead of an arbitrary map key, and raises capacity from 128 to 512 entries for multi-module shells.
- [ ] Remove unnecessary text renderer locking from hot single-threaded paint paths or make the lock coarser. The thread-local renderer still uses a `Mutex<TextEngine>` around measure/render.
- [ ] Avoid cloned image/font data in icon and image paint. First pass returns cached image/font bytes through `Arc` in `surface/icon.rs::load_image_rgba` and `surface/glyph.rs::font_bytes`; remaining work is to avoid Skia `Data::new_copy` by caching decoded Skia images/faces.
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

- [ ] Audit SHM buffer release behavior under sustained frame backlog. The backend keeps a small pool and can allocate fallback buffers when no canvas is available. Ensure this cannot grow unbounded and consider explicit release tracking/triple buffering.
- [ ] Preserve and reuse surface configuration state more aggressively. The shell builds surface config on render; the presentation backend skips reconfigure when unchanged, but the shell can avoid config work unless size/title/options are dirty.
- [ ] Track damage as multiple rects deeper into the retained renderer. Presentation already accepts region damage, but retained display-list damage often collapses node changes to a union rect. Keep separate dirty node rects longer to reduce overdraw.
- [ ] Add performance profiles for canonical shell workloads: idle shell, pointer move over dense tree, text update from backend, scrolling, large icon grid, animation, theme reload, and resize. Use existing stage timings to pin budgets for build/restyle/layout/render-object/display-list/paint/present.

P2 - larger architecture options:

- [ ] Consider a typed runtime node representation for hot paths. `WidgetNode` stores tag, attributes, event handlers, and content as strings/maps; keep source compatibility but compile into compact typed node/style/layout/paint records before runtime.
- [ ] Consider GPU rendering only after CPU invalidation and batching issues are fixed. The current biggest wins are less work per frame, not a different raster backend.
- [ ] Consider parallel layout/paint preparation after retained immutable snapshots are stable. Parallelism will not help much until broad invalidation and shared mutable caches are reduced.
- [ ] Add allocation counters to profiling output. Many current costs are likely allocation/string/hash driven and will not show clearly in wall-time-only stage metrics.

See `docs/performance-roadmap.md` for the durable roadmap.

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
                                             
