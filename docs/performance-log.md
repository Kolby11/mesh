# MESH Performance Log

Archived record of the performance work tracked in `todo.md`, moved here on
2026-07-13 to keep the backlog readable. This file is **history, not backlog**:
the canonical open-item list lives in `todo.md`. Checkbox state, progress
narratives, measurements, and section letters (A–V) below are preserved
verbatim as of the archive date; open items listed here also have a current,
slimmed entry in `todo.md`.

Every entry keeps its benchmark numbers so future work can compare against the
recorded baselines. Sections M–V were focused per-subsystem deep dives; see
`PERFORMANCE_SECTIONS.md` for the subsystem map.

## Rejected experiments — do not retry without new evidence

Each of these was prototyped, measured, and reverted. Full context lives in the
section noted.

| Date | Experiment | Section | Result |
| --- | --- | --- | --- |
| 2026-07-03 | Component-local copied tracked-field summary for the service event gate | C | Only 1.1x (2.544ms → 2.221ms/100k) and refresh had to clone tracked maps; shell-side subscription index is the viable design |
| 2026-07-03 | Fractional-scale physical-damage rect fix | D | Byte-for-byte correct but no CPU-side win (28.066ms vs 28.367ms one-box); revisit with compositor/upload damage instrumentation |
| 2026-07-04 | Fuse surface + child enter/exit class annotation into one full-tree traversal | D | 0.6x — 87.249ms fused vs 52.903ms targeted walks; fusion must not scan unrelated branches |
| 2026-07-04 | Retain/clear scratch `node_map`/`text_nodes` allocations in layout | F/T | 0.99x (77.502ms vs 77.033ms); full style sync dominates |
| 2026-07-04 | Render-object child IDs `Vec` → `SmallVec` | D | Slower (11.285ms Vec vs 21.992ms SmallVec) |
| 2026-07-04 | Private Lua-table cache for scalar `self.storage` reads | I | 0.8x (50.238ms vs 39.202ms); extra Lua lookup outweighed saved Rust work |
| 2026-07-05 | `Arc<str>` for display payload strings | I | 6.6% regression on two-string payload clone (67.591ms vs 63.400ms/5M); atomics on every command clone |
| 2026-07-05 | One full-tree inherited-state walk replacing per-key closing-popover searches | J | Slower for the realistic small-key case (1.035s vs 855.098ms/2k) |
| 2026-07-05 | `SmallVec` for `RenderObjectTree` child ids | N | Regressed the six-child case 28.448ms → 37.879ms; retained-`Vec` clear/refill won instead (4.2x) |
| 2026-07-10 | `Vec::drain` instead of the `VecDeque::from` adapter for emitted requests | V | 0.9x (82.505ms vs 74.102ms/1M four-request batches) |
| 2026-07-12 | Scratch `PixelBuffer` reuse for rotation transforms | P | 341.011ms scratch vs 2.586ms fresh alloc+clear over 2M 96x64 buffers; fresh zeroed allocation is much cheaper |
| 2026-07-12 | Hand-rolled inline clip/layer stacks in the Skia backend | P | 350.183ms inline vs 31.812ms `Vec` over 8M batches; later solved with `SmallVec` (2026-07-13) |
| 2026-07-12 | `tree.style(id) == new_style` equality guard before Taffy `set_style` | T | Slower than unconditional `set_style` (51.642ms vs 46.115ms); needed the retained dirty-bit feed, which landed 2026-07-12 |
| 2026-07-15 | Rust recursive detached-Lua-table cache for nested storage reads | I | Regressed 1.221s current to 1.815s cached over 100k reads (0.67x); deep-copy construction outweighed saved conversion/locking |
| 2026-07-15 | Luau `table.clone` plus recursive array replacement for nested storage reads | I | Regressed 1.237s current to 1.611s cached over 100k reads (0.77x); exact detached-value semantics still require too much table reconstruction |

## 2026-07-14 follow-up — five measured hot-path changes

- Retained narrow/layout analysis caps its initial result-set capacity at 256;
  a 4,096-node release microbenchmark measured 2.327s growing versus 2.238s
  reserved (1.04x), avoiding unbounded sparse-frame over-allocation.
- Surface shortcut resolution retains the preformatted accessibility index;
  1,000 release probes measured 3.297ms rebuilding versus 2.4µs borrowing
  the cached map.
- Element-metrics publication moves the refs JSON value into script state after
  borrowing it for the live proxy, eliminating a full snapshot clone; 20,000
  256-entry release snapshots measured 1.601s clone versus 996.7ms move (1.6x).
- Scroll overflow annotation reserves the reusable root key-path buffer;
  20,000 release passes measured 796.1ms unreserved versus 769.5ms reserved
  (1.03x).
- Service-field reverse dependencies use a nested borrowed lookup instead of
  allocating a `(String, String)` key per query; 1M release lookups measured
  33.7ms tuple allocation versus 27.7ms nested lookup (1.2x).

## 2026-07-14 follow-up — retained/render scratch reuse

- Clean `RenderObjectTree` updates now skip the full stale-entry scan when the
  visited count and retained map length prove that no structure changed; a
  4,096-entry release benchmark measured 65.3µs retain scanning versus 10.7µs
  conditional skip (6.1x).
- Retained display-list dirty-ancestor collection reuses its path vector and
  ancestor set; 50,000 sparse release walks measured 6.39ms fresh versus
  4.38ms reused (1.46x).
- Animation passes reuse the live-key sets and previous-style snapshot map;
  release benchmarks measured 2.35x and 1.68x versus fresh allocations.
- Element-metrics publication reuses the ref-name → node-key map backing
  storage between paints; 20,000 512-entry release maps measured 1.368s fresh
  versus 719ms reused (1.90x).
- The shell service-delivery index now starts dirty so its first event lazily
  builds from the registered component set instead of being dropped, then the
  built index is marked clean. A fresh release validation measured 204.4ms
  full scan versus 15.3ms indexed across the existing 20,000-event/256-component
  workload (~13.4x) while the accepted-delivery regression test passes.
- Unchanged embedded-component prop publication now borrows host-owned JSON
  values, cloning only when a prop changes. A 100,000-write release benchmark
  measured 15.5ms for the owned unchanged gate versus 4.3ms for the borrowed
  gate (~3.6x); eager Lua writes measured 130.3ms.
- Local component style cascades now cache the merged host/component rule
  vector and its selector index per host-module/alias pair. Over 20,000
  iterations of a 64-rule cascade, rebuilding took 706.0ms while borrowing the
  prepared entry took 10.7µs (~66,069x for the eliminated preparation work).
- Runtime validation and CSS projection convert JSON props by reference rather
  than deep-cloning values that the caller retains. A 20,000-iteration nested
  value benchmark measured 418.9ms owned versus 247.1ms borrowed (~1.7x); CSS
  projection maps also reserve their declared prop count up front.

## 2026-07-15 — canonical profiling workloads

- The typed debug benchmark contract now exposes idle, pointer move, text
  update, scroll, icon grid, animation, theme reload, and resize profiles with
  stable shipped targets and stage priorities. Inspector fallback rows and IPC
  scenario parsing use the same IDs; existing interaction scenarios remain for
  compatibility. Profile guidance requires a fresh profiling session per
  workload so accumulated summaries remain comparable.

## 2026-07-15 — cached runtime diagnostic class tokens

- Runtime style diagnostics now resolve live `WidgetNode`s directly through
  their restyle-populated class-token cache instead of splitting the `class`
  attribute into a fresh `Vec<String>` for every rebuilt node. The indexed
  allocating and cached paths have parity coverage. A release benchmark over
  200,000 diagnostic resolutions measured 79.0ms for per-node splitting versus
  55.0ms for cached node tokens (1.44x faster for the diagnostic-resolution
  subpath).

## 2026-07-15 — parallel diagnostic, handler, and popup invalidation wave

- Unchanged rebuilds now fingerprint runtime style-diagnostic inputs (rule
  generation, selector-facing tree, surface props, and container dimensions)
  and skip the second full style-resolution traversal when they match. A
  156-node release benchmark over 2,000 unchanged rebuilds measured 172.7ms
  full re-resolution versus 19.8ms for the fingerprint gate (8.7x; repeat runs
  ranged from 8.7x to 9.2x).
- Ordinary handler graph namespacing now constructs the embedded-instance
  prefix once per traversal and appends raw handler names with exact capacity.
  A 1,000-node release benchmark over 200 traversals measured 79.6ms formatting
  each complete handler versus 31.8ms with the shared prefix (2.50x; repeat
  2.53x).
- Retained paint subtrees now expose independent generations, allowing promoted
  child surfaces to remain cached across unrelated parent/sibling updates. A
  160x90 release lower-bound benchmark across 10,000 parent frames measured
  4.290ms and 10,000 clears using the broad generation versus 3.562us and one
  clear using the popup subtree generation (~1,204x for the gate plus avoided
  clear). Popup-descendant changes still advance the child generation.

## 2026-07-15 — retained popup replay and root dispatch borrowing

- Promoted child targets now own bounded, origin-aware retained display lists
  and replay their command stream instead of using the immediate tree painter.
  Pixel parity is covered. A 61-node popup across 400 root-opacity transition
  frames measured 38.0ms immediate versus 30.9ms retained (1.23x); a sparse
  one-descendant material workload measured 17.5ms versus 4.83ms (3.61x).
- Plain root handler dispatch now borrows the surface instance ID for runtime
  lookup and allocates it only when published events or live-binding neighbors
  need post-dispatch processing. Two 2M-lookup release runs measured
  41.4ms/41.0ms owned versus 30.9ms/31.4ms borrowed (1.34x/1.30x).

---

## Non-performance completed items

### Codebase cleanup — markup `{...}` as full Luau (completed 2026-07-05)

- [x] **Evaluate markup `{...}` as full Luau in the component instance
      environment.** The authoring contract is that braces contain a
      value-producing Luau expression in the same lexical/runtime scope as the
      component `<script lang="luau">`, including private locals, functions,
      standard operators, and `{#for}` locals. Today
      `frontend/compiler/src/expr.rs` is a hand-written subset evaluator and the
      compiler receives only a `VariableStore` JSON snapshot, so valid Luau is
      rejected or evaluated with divergent truthiness/`and`/`or` semantics.
      Compile/register expression closures inside the component script chunk
      itself (a second chunk sharing `_ENV` cannot see lexical locals), thread
      closure invocation through the tree-build/composition boundary by
      component instance key, pass `{#for}` locals as invocation bindings
      without mutating persistent globals, and isolate the legacy evaluator to
      VM-less preview/tooling paths.
      Completed 2026-07-05: runtime creation collects each component's template
      expressions and compiles them into closures appended to that component's
      Luau chunk, preserving lexical access to private locals/functions. Tree
      rendering invokes closures through the owning instance runtime; loop
      locals are supplied through a temporary locals-first function
      environment. Text, attributes, conditionals, loop iterables, component
      props, and handler-call arguments use typed VM results. The legacy
      evaluator remains only as a VM-less preview/test fallback. The markup
      preprocessor is now quote-aware inside brace expressions, including
      nested double-quoted Luau strings.

---

## Performance sections — archived detail (as of 2026-07-13)

## Performance — remaining open items

Items owned by a milestone are listed with their milestone reference.

### P0 — scheduling and invalidation (→ v1.18 / v1.19)

- [ ] Narrow script/service invalidation below tree-rebuild + pixel repaint; add typed state dependencies → v1.18
- [ ] Avoid full-tree restyle for safe interaction changes; use selector-dependency analysis → v1.18

### P0 — scripting (→ v1.17)

- [ ] One `mlua::Lua` VM per ScriptContext (`runtime.rs:92`); move to per-thread VM with `_ENV` isolation → v1.17
### P1 — renderer hot paths

- [ ] Interaction frames still re-apply string style declarations per node (`apply_declaration_no_diagnostics` + theme defaults maps dominate the post-2026-06-10 toggle profile); folds into the typed/compiled declaration work → v1.23 and narrower invalidation → v1.18. Progress 2026-07-05: `StyleRuleIndex` now precomputes no-diagnostics declaration metadata for cached restyle paths, so interaction/restyle frames reuse support/profile/deprecated-token classification instead of repeating it per matched declaration. Added parity coverage for cached vs uncached declaration application and a release-only microbenchmark showing indexed declaration application at ~1.4x faster (156.3ms → 109.1ms for 200k iterations on the local profile run).
- [ ] Avoid flattening retained display-list subtrees into a new flat command buffer on each update; move toward segment/rope-style command storage → v1.21. Progress 2026-07-05: `RetainedDisplayList::update_inner` now detects unchanged display entries for the same root/surface before rebuilding paint subtrees, preserving the existing flat command arrays on no-op updates. Added regression coverage for zero subtree/command rebuilds on unchanged trees and a release-only benchmark showing unchanged update at ~2.6x faster than fresh flat rebuild (349.0ms → 922.3ms for 1k iterations on the local profile run).
- [ ] Replace per-node string/hash-heavy style matching with interned/typed node keys; remaining after first pass: interned tags, classes, attribute keys → v1.23
- [ ] Retain Taffy node state across layout passes; `build_taffy_tree` rebuilds a fresh TaffyTree every layout → v1.21
- [ ] Affected-subtree template re-evaluation: `narrow_script_update` rebuilds the full tree (full template eval) then diffs; use `NodeServiceFieldDependencies` to re-evaluate only nodes whose tracked fields changed → v1.27. Progress 2026-07-14: narrow and layout analysis now walk the retained slotmap directly instead of building a temporary fresh snapshot map; a same-run release benchmark measured 396.1ms map-based versus 317.1ms direct over 2,000 passes (1.25x). The larger affected-subtree re-evaluation remains open.
- [ ] Generation-aware retained-tree diff: `RetainedWidgetTree::update` FNV-hashes every node's style + attribute strings per paint; skip clean subtrees using dirty bits → v1.27
- [ ] Fuse the five per-frame `finalize_tree` annotation walks into one traversal; move hot annotations from string attributes to typed `WidgetNode` fields → v1.27

### P1 — backend modules

- [ ] Investigate `pw-dump --monitor` as a real volume event source for the pipewire-audio backend — `pw-mon` emits no `changed:` block for volume changes (verified with and without `--hide-params`), so the stream currently only signals client/object lifecycle, and volume detection leans on the safety poll
### P1 — presentation and memory (→ v1.20)

- [ ] Add performance profiles for canonical shell workloads (idle, pointer move, text update, scroll, icon grid, animation, theme reload, resize) → v1.21
### P2 — architecture

- [ ] Introduce interned `Symbol` / `TagId` types before further string-key cleanups → v1.23
- [ ] Add allocator-level profile mode (allocation counts per render pass) → v1.23
- [ ] Consider typed runtime node representation for hot paths (`WidgetNode` tag/attrs/content as strings today) → v1.23
- [ ] GPU rendering — after retained layout, smart invalidation, and damage tracking ship → v1.25

---

## Performance improvements — 2026-07-02 deep scan

Findings from a full-codebase performance scan (data handling, component
communication, events, rendering) motivated by the gap to QtQuick/webview-class
shells. Each item cites `file:line` as of this scan; reverify before editing.
Items that overlap an existing milestone entry above say so instead of
duplicating it.

### A. Data handling — Rust ↔ Lua boundary is JSON-shaped and clone-heavy

- [ ] **Per-paint element metrics: build → deep-compare → JSON→Lua convert,
      every frame.** `publish_element_metrics`
      (`shell/component/interaction_state.rs:41-65`) serializes _every keyed
      node_ to a `serde_json::Map` per paint, `set_host_value` deep-compares
      it, then `apply_element_metrics`
      (`scripting/context/runtime.rs:414-428`) converts the whole object to a
      Lua table **and** reinstalls bound element proxies — per frame, even
      when nothing scripted reads geometry that frame. Make `refs.<name>`
      reads lazy: keep metrics in a Rust-side store and resolve fields on
      `__index` (the proxy machinery already exists in `element_ref.rs`),
      publishing only a generation bump per paint; drop the eager
      `elements`/`refs` state tables or gate them on actual template reads.
      Progress 2026-07-02: the state-side deep-compare portion is removed for
      unchanged metrics via full-JSON fingerprints (see previous item), but
      eager JSON construction remains. Progress 2026-07-03: the scripting
      runtime now caches the last successfully installed refs fingerprint, so
      unchanged paints skip JSON→Lua conversion and bound-proxy reinstallation.
      A release benchmark over 20k unchanged publications measured 90.711ms for
      the eager path versus 0.140ms for the fingerprint-gated path (~647x
      faster). Rust-side tree walking/JSON construction and lazy `refs` field
      resolution remain open. Progress 2026-07-04: metrics publication is now
      gated by the retained-tree diff. Paint/style/state-only frames skip the
      Rust tree walk, JSON maps, fingerprints, runtime lock, and proxy update;
      layout, attribute, child, insertion, and removal changes still publish.
      A 1,365-node release microbenchmark over 2,000 unchanged passes measured
      23.236s for rebuilding snapshots versus 1.188us for the dirty-summary
      gate. Lazy field resolution remains open for frames where metrics really
      changed. Progress 2026-07-05: metric usage is now split between
      `elements` and `refs`, so ref/id-only components keep publishing the
      public `refs` table and live proxies without also building the all-node
      `elements` snapshot. A 341-node release benchmark over 2,000 changed
      publications measured 6.069s for collect-both versus 3.823s for
      refs-only (1.6x faster). Progress 2026-07-05: the collector now builds
      full JSON snapshots lazily only for nodes that actually publish to
      `elements` or `refs`, while reading scroll offsets directly for traversal
      through unpublished ancestors. A sparse-ref 341-node release benchmark
      over 2,000 publications measured 1.872s for eager per-node snapshots
      versus 205.920ms for lazy snapshots (9.1x faster). Progress 2026-07-13:
      element snapshot scroll fields now read the runtime's typed
      `WidgetNode::scroll_metrics` via `resolved_scroll_metrics()` instead of
      parsing `_mesh_scroll_*` attributes when typed metrics are present, while
      preserving the legacy attribute fallback. A release microbenchmark over
      2M scroll metric reads measured 106.047ms for attribute parsing versus
      6.085ms for typed metrics (17.4x faster for that snapshot subpath). Lazy
      field resolution remains open for frames where metrics really changed.
      Progress 2026-07-13: `element_snapshot_json` now builds the public JSON
      object directly instead of building an `ElementSnapshot`, cloning
      attributes into a `BTreeMap`, and serializing it back through serde before
      adding tag-specific fields. Parity coverage compares the direct builder
      against the old serde shape; a release microbenchmark over 200k input
      snapshots measured 530.347ms for the serde roundtrip versus 459.070ms
      direct (1.2x faster). Progress 2026-07-13: the public
      `element_snapshot()` path now clones the node attribute map directly
      instead of rebuilding it through iterator collection; a release
      microbenchmark over 500k 16-attribute clones measured 360.630ms for
      collect-clone versus 193.524ms for `BTreeMap::clone()` (1.9x faster for
      that snapshot subpath).
- [ ] **Stringly-typed template expression values.** `eval_expr` returns
      `String` for everything (`frontend/compiler/src/expr.rs:26,162`);
      numeric ops re-`parse::<f64>` both sides per evaluation
      (`expr.rs:197`), `if` conditions compare against `"false"|"nil"|""|"0"`
      string literals, and every result is stored as an attribute `String`
      that downstream code re-parses. Introduce a small typed value enum
      (bool/number/string) for compiled-expression evaluation and only
      stringify at the attribute boundary — this also removes false
      attribute-hash dirtiness from float formatting.
  - [x] 2026-07-04: compiled expression evaluation now carries an internal
        bool/number/string value enum through boolean operators, ternaries,
        comparisons, concatenation, translation, and JSON variable reads, then
        stringifies only at the public `eval_expr` boundary. Numeric JSON
        comparisons avoid per-evaluation string allocation and `parse::<f64>`.
        A release benchmark over 500k numeric comparisons measured 36.848ms for
        the old string-parse shape versus 29.394ms for typed comparison (1.3x
        faster).
  - [ ] Attribute storage remains string-based until the downstream
        `WidgetNode`/style contracts are typed.

### B. Component communication & input

- [ ] **Handler dispatch overhead per event.** `call_namespaced_handler`
      locks the runtimes mutex, allocates 3 Strings for namespacing, and
      unconditionally runs `resync_binding_neighbors` over every linked
      instance after each handler (`shell/component/runtime.rs:494-560`).
      Track "did a cross-`_ENV` write actually happen" (a dirty bit set by
      the live-binding `__newindex`) and skip neighbor resync when clean;
      intern instance keys. Progress 2026-07-04: ordinary handler names now
      bypass legacy JSON-descriptor parsing unless the first byte is `{`;
      legacy pre-bound descriptors remain supported. A release benchmark over
      500k pointer-handler unpacks measured 43.866ms with failed JSON parsing
      versus 37.898ms with the syntax gate (1.2x faster). The binding-resync
      and instance-key allocation work remains open; a simple `__newindex`
      dirty bit is insufficient because Lua does not invoke it when replacing
      existing globals. Progress 2026-07-04: live `bind:this` proxies now set
      a per-runtime external-access flag only when another component writes
      through the proxy or calls a proxied function. Post-handler neighbor
      resync consumes that flag and skips untouched linked runtimes, while
      touched child-call semantics remain covered by
      `bind_this_event_handler_calls_child_live_and_resyncs_it`. A release
      benchmark over 2k untouched-neighbor checks measured 3.194ms for
      unconditional child resync versus 42.743us for the flag-gated skip
      (74.7x faster).
  - [x] 2026-07-04: plain handler argument unpacking now borrows the handler
        name and event args instead of cloning them into a fresh `String`/`Vec`
        on every dispatch; legacy JSON descriptors and typed pre-bound handler
        args still allocate only when merging is needed. A release benchmark
        over 500k plain handler transfers measured 40.768ms for clone-transfer
        versus 2.548ms for borrowed transfer (16.0x faster).
  - [ ] Instance-key interning remains open. Progress 2026-07-06:
        namespaced handler dispatch now borrows the parsed instance key and
        raw handler name directly from the already-namespaced handler string,
        and clones the component id only on the error/diagnostics path. A
        release benchmark over 500k target resolutions measured 44.668ms for
        clone-heavy resolution versus 10.224ms for borrowed resolution (4.4x
        faster). Full graph-wide instance-key interning remains open.
- [ ] **`bind:this`/live-binding writes mark the whole surface dirty.** Any
      handler that touches state invalidates via `invalidate_script_state()`
      → full template re-eval + tree rebuild (narrow path still rebuilds the
      full tree first — see v1.27 item). The typed state-dependency work
      (v1.18) should extend to handler writes: record which public members a
      template actually binds and skip rebuilds for writes nothing binds to.

### C. Events & service delivery

- [ ] **Per-event mutex churn in the observation gate.**
      `deliver_service_event` (`runtime/service_state.rs:167-184`) calls
      `observes_service_event` on every component per event, which locks that
      component's `runtimes` mutex and queries tracked-field maps
      (`shell_component.rs:261-268`); several tracked-field APIs clone whole
      maps/sets (`tracked_service_fields()`,
      `tracked_fields_for_service()` — `scripting/context/runtime.rs:478-497`).
      Maintain a shell-side subscription index (service → component indices),
      invalidated when a runtime's tracked fields/subscriptions change, so
      event routing is a lookup instead of N mutex acquisitions. Experiment
      2026-07-03: a component-local copied summary refreshed after paint/input
      was rejected and reverted. Its production-faithful single-runtime
      release benchmark improved the event gate only from 2.544ms to 2.221ms
      over 100k calls (1.1x), while refreshes had to clone the tracked maps.
      The shell-side index described above remains the viable design because it
      also eliminates the O(component count) scan rather than merely moving
      locks into refresh work.
- [ ] **Backend providers still exec-poll by default.** `spawn_backend_service`
      drives a tokio interval that re-runs `exec` subprocesses per tick
      (`runtime/backend/src/lib.rs:157-…`). The pipewire item above tracks one
      backend; the generic gap is push-based host API primitives (D-Bus
      signal subscribe, fd/socket watch, `pw-dump --monitor`-style stream
      adoption) so providers can be event-driven and the safety poll becomes
      the fallback, not the mechanism.

### D. Rendering & per-frame work

- [ ] **Fractional HiDPI forces full-surface repaint every frame.** `paint`
      sets `surface_pixels_invalid = true` whenever `scale` is non-integer
      (`shell/component/shell_component.rs:460-462`), so on a 1.25/1.5×
      output _every_ frame is a full clear+repaint+full-damage present —
      partial damage only exists at integer scales. Fix the underlying
      logical-vs-physical damage-clip mismatch (compute/clip damage in
      physical pixels through the painter) so the retained partial path works
      at fractional scale. Likely the single biggest win on fractional-scale
      setups. Experiment 2026-07-03: a physical-damage rect fix was correct
      byte-for-byte against forced-full repaint, but rejected as a performance
      change for now. End-to-end release benchmarks did not show a meaningful
      CPU-side win: one-box 1200×600 forced-full 28.066ms vs partial 28.367ms,
      and large 3600×1800 forced-full 74.918ms vs partial 73.910ms. Revisit
      with compositor/upload damage instrumentation before marking this done.
- [ ] **Per-frame full-tree fingerprinting even when clean.**
      `RetainedWidgetTree::update` re-walks every node, hashes ~50 style
      fields + every attribute/handler string (FNV byte-at-a-time), allocates
      a snapshot (with a `child_ids` Vec) per node into a scratch map, and
      clones snapshots on change (`runtime_tree.rs:98-163,293-392`). The
      v1.27 "generation-aware diff" item covers skipping clean subtrees; add:
      hash with a word-at-a-time hasher (fxhash), reuse snapshot allocations
      in place (index by slotmap key instead of rebuilding the `NodeId` map),
      and stop hashing shell-owned annotation attributes that already have
      typed change tracking (`_mesh_scroll_*`, `_mesh_key`). Partial
      2026-07-03: `RuntimeTreeHasher` now implements primitive `write_*`
      methods so numeric style fields are mixed word-at-a-time instead of
      falling back to byte-at-a-time hashing. A release benchmark over 500k
      style fingerprints measured 118.362ms for the old byte fallback versus
      63.946ms primitive-aware (1.9x faster). Snapshot allocation reuse and
      broader shell-owned annotation filtering remain open. Progress
      2026-07-04: `_mesh_key` is no longer included in the attribute hash
      because the same identity is already encoded by the retained `node.id`;
      structural movement still changes parent `child_ids`. A 10-level-key
      release microbenchmark over 2M fingerprints measured 98.724ms with the
      redundant key hash versus 44.799ms without it (2.2x faster). Scroll and
      other shell annotations remain hashed until they have equivalent typed
      change tracking. Progress 2026-07-13: retained attribute fingerprints now
      skip `_mesh_scroll_*` and `_mesh_content_*` annotations because
      `layout_fingerprint()` already tracks the same resolved typed scroll
      metrics. A standalone Rust benchmark matching the attribute-hash loop
      measured 475.5ms with redundant scroll/content attribute hashing versus
      32.9ms with the typed-annotation skip over 2M hashes (14.5x faster for
      that subpath); in-crate shell verification remains blocked locally by
      the missing `xkbcommon.pc` system dependency. Progress 2026-07-04:
      retained snapshot `child_ids` now
      use inline storage for up to eight children, eliminating the per-node
      heap allocation for normal UI trees while spilling safely for wider
      containers. A 4-child release microbenchmark over 2M snapshots measured
      9.811ms with fresh `Vec` allocation versus 2.810ms inline (3.5x faster).
      The transient retained dirty `SecondaryMap` now also swaps through a
      scratch slot instead of reallocating on each interaction update; a
      128-dirty-node release benchmark over 20k updates measured 6.622ms fresh
      versus 3.419ms reused (1.9x faster). Progress 2026-07-04: retained
      snapshot updates now remove stale nodes before draining the per-frame
      scratch map, then move changed/inserted `RetainedNodeSnapshot`s into
      slotmap storage instead of cloning them. Release benchmark:
      clone-transfer 216.847ms vs drain-move 177.698ms over 5.12M snapshot
      transfers (1.2x faster). Broader clean-subtree skipping and slotmap-keyed
      snapshot reuse remain open. Progress 2026-07-04: pre-bound event handler
      args in retained attribute fingerprints now hash `serde_json::Value`
      structure directly instead of allocating a serialized string for every
      arg. A release benchmark over 500k nested JSON arg fingerprints measured
      433.760ms for `to_string` hashing versus 92.355ms for direct typed
      hashing (4.7x faster). Progress 2026-07-10: `_mesh_focused` is no
      longer included in retained attribute fingerprints because the same
      state change is already tracked by the typed `ElementState` fingerprint.
      Added regression coverage and a release-only benchmark; the local
      dev-shell run measured 82.703ms with redundant hashing versus 58.788ms
      with the skip over 2M fingerprints (1.4x faster).
- [ ] **`WidgetNode` allocation profile.** Every node carries `tag: String`,
      `attributes: BTreeMap<String,String>`, `event_handlers:
    BTreeMap<String,String>` (`ui/elements/src/tree.rs:44-68`), rebuilt
      from the template on every script invalidation and deep-cloned by the
      input path. Interning (v1.23) plus a small-map type (attrs are
      typically <8 entries; `Vec<(Symbol, CompactString)>` beats a BTreeMap)
      and moving shell annotations (`_mesh_key`, scroll offsets, focus flags,
      selection coords — currently formatted floats in string attributes,
      `runtime_tree.rs:729-743`, `rendering.rs:697-728`) to typed fields
      would shrink both build and diff cost. Overlaps v1.23/v1.27; listed
      here because the _authoring_ of new annotations keeps growing the
      string surface.
- [ ] **`finalize_tree` runs ~8 full-tree walks per finalized frame** beyond
      the annotation fuse already tracked (v1.27): `annotate_runtime_tree`,
      `append_class_recursive` (exit/enter classes), `annotate_surface_shortcuts`,
      `annotate_overflow_tree`, `merge_runtime_primitive_defaults`,
      `collapse_promoted_popover_wrappers`, `constrain_error_placeholders`,
      `annotate_selection_tree` (`shell/component/rendering.rs:238-432`).
      Several are only relevant when a feature is active (no popovers → no
      collapse walk; no selection → skip). Gate the conditional walks on
      cheap presence flags and fold the unconditional ones into the fused
      traversal. Partial 2026-07-03: promoted-popover collapse and generated
      error-placeholder constraints are now guarded by component-level presence
      flags set at the actual marker creation sites; normal trees skip both
      marker walks. Surface shortcut annotation now also returns before loading
      keyboard settings when neither manifest nor legacy settings declare
      shortcuts. A release microbenchmark over 20k plain-tree finalizations
      measured 361.497ms for the two old marker walks versus 2.375us for the
      gated path (152k×). The always-needed annotation/restyle/layout walks and
      broader traversal fusion remain open. Rejected experiment 2026-07-04:
      fusing surface and child enter/exit class annotation into one full-tree
      traversal measured 87.249ms versus 52.903ms for the existing targeted
      searches/subtree walks (0.6x). The prototype was reverted; any future
      fusion needs to avoid scanning unrelated branches. Progress 2026-07-04:
      text-selection annotation now resolves the selected `_mesh_key` with the
      existing keyed node lookup and annotates only that node instead of running
      a selection-specific recursive tree walk. Release benchmark on a broad
      tree: recursive 4.072s vs keyed 3.857s over 10k iterations (1.1x faster).
      Progress 2026-07-10: targeted interaction restyle now carries both the
      full affected-descendant set for CSS matching and a root-only set for
      runtime primitive default merging, so hover/focus frames stop applying
      default fills across unrelated subtrees. Added regression coverage and a
      release-only benchmark; the local dev-shell run measured 2.593s for
      full-tree default merge versus 1.923s targeted over 5k synthetic
      interaction updates (1.3x faster). Progress 2026-07-12: the mandatory
      `annotate_runtime_tree` traversal no longer allocates a
      `source_element_tag(node).to_string()` for every node; it borrows the
      source tag only in the choice/select branches that need it. Added a
      release-only benchmark over 20k plain-tree walks; the local dev-shell run
      measured 250.396ms for eager allocation versus 58.000ms for lazy
      borrowed checks (4.3x faster for that annotation subpath). Progress
      2026-07-13: interaction changed-subtree collection now converts changed
      hover/focus runtime paths to stable `NodeId`s once, then matches
      `node.id` during the tree walk instead of probing string key sets for
      every node. A standalone Rust microbenchmark matching the collector shape
      measured 825.6ms for the string-key collector versus 498.4ms for the
      `NodeId` collector over 20k synthetic updates (1.66x faster); in-crate
      shell tests/benchmark remain blocked locally by the missing `xkbcommon.pc`
      system dependency.
- [ ] **Layout + display list**: Taffy tree rebuilt per layout pass and
      display-list subtree flattening per update are already tracked
      (v1.21). Reaffirmed as the dominant structural-frame costs behind
      restyle in this scan; no new sub-findings. Progress 2026-07-04:
      render-object sync now reuses the per-update `dirty_nodes` allocation
      and replaces the separate `visited` hash set with an epoch mark stored on
      each retained render object. Release benchmark: visited set 158.594ms vs
      epoch marks 89.300ms over 20k synthetic updates (1.8x faster). Rejected
      experiment: changing render-object child IDs from `Vec` to `SmallVec`
      measured slower (11.285ms `Vec` vs 21.992ms `SmallVec`) and was reverted.
      Progress 2026-07-10: dirty-ancestor collection for retained display-list
      subtree reuse now stops once all dirty nodes have been found instead of
      always walking the full tree. Added regression coverage for sparse dirty
      ancestor correctness and a release-only benchmark; the local dev-shell
      in-crate run measured 2.334s full-walk versus 5.292ms early-exit over
      the sparse-dirty workload (441.1x faster). Progress 2026-07-10:
      retained display-list
      damage diff now skips the previous-entry removal scan when the new entry
      set has no insertions and the map sizes match, because removals are then
      impossible. Added a release-only benchmark; the local dev-shell in-crate
      run measured 1.905s for the full previous-entry scan versus 185.220us
      for the guarded skip over 200k stable-key updates (10283.4x faster).
- [ ] **CPU Skia raster + SHM is the ceiling.** Painting is skia-safe CPU
      raster into `PixelBuffer` + SHM upload (`render/src/surface/painter/backend.rs`);
      blur/shadows/gradients are CPU per damaged pixel. GPU rendering is
      deferred (v1.25) — when it lands, prefer a `wgpu`/Skia-GPU surface per
      output with the retained display list as the command source, and keep
      SHM as fallback. Until then, the damage-path fixes above (especially
      fractional scale) are the effective lever.
### E. Style system — second-pass findings

- [ ] **Every declaration resolves through a String round-trip.** Theme
      tokens are stored as `TokenValue::Number` but resolution formats them
      (`format!("{n}")`, `resolve.rs:402`) and downstream re-parses
      (`parse_px`, `Color::from_hex` — `resolve.rs:446-461`); `var()`
      resolution walks embedded-reference string substitution per value.
      This is the inner loop of both build and restyle. Extends the v1.23
      typed-declaration item: resolve tokens to typed values
      (`Color`/`f32`/enum) once per theme load and make
      `apply_declaration` consume typed values, keeping strings only for
      diagnostics. Progress 2026-07-06: numeric style resolution now has a
      typed fast path for simple `var(...)`/`prop(...)` references and numeric
      theme tokens, avoiding `TokenValue::Number -> String -> parse_px`
      round-trips for numeric properties while preserving fallback string
      semantics for embedded CSS references. A release benchmark over 500k
      numeric token resolutions measured 83.468ms for the string round trip
      versus 47.757ms for the typed path (1.7x faster). Progress 2026-07-06:
      color style resolution now also has a borrowed fast path for simple
      literals, variables, props, and string theme tokens, avoiding a cloned
      resolved string before `Color::from_hex` when substitution is not needed.
      A release benchmark over 500k color token resolutions measured 44.999ms
      for the string-clone path versus 35.997ms for the borrowed path (1.3x
      faster). Progress 2026-07-06: keyword-style declarations now use the
      same borrowed simple-value fast path for direct comparisons across
      `display`, `position`, visibility, flex/alignment, text, and blend-mode
      properties. A release benchmark over 500k keyword token resolutions
      measured 38.788ms for the string-clone path versus 31.852ms for the
      borrowed path (1.2x faster). Progress 2026-07-06: dimension declarations
      (`width`, `height`, `flex-basis`) now parse borrowed simple values as
      well, avoiding allocation before `parse_dimension` for common tokenized
      sizes. A release benchmark over 500k dimension token resolutions measured
      48.995ms for the string-clone path versus 41.248ms for the borrowed path
      (1.2x faster). Progress 2026-07-06: overflow declarations
      (`overflow`, `overflow-x`, `overflow-y`) now parse borrowed simple
      values too. A release benchmark over 500k overflow token resolutions
      measured 57.668ms for the string-clone path versus 47.218ms for the
      borrowed path (1.2x faster). Progress 2026-07-06: transition/animation
      duration and delay declarations now parse borrowed simple values before
      `parse_first_time_ms`. A release benchmark over 500k time token
      resolutions measured 120.253ms for the string-clone path versus
      113.080ms for the borrowed path (1.1x faster). Progress 2026-07-06:
      `transition-property` now parses borrowed simple property lists before
      `parse_transition_properties`. A release benchmark over 500k transition
      property token resolutions measured 72.124ms for the string-clone path
      versus 65.417ms for the borrowed path (1.1x faster). Progress
      2026-07-06: `filter` and `backdrop-filter` now parse borrowed simple
      values before `parse_filter`. A release benchmark over 500k filter token
      resolutions measured 52.210ms for the string-clone path versus 43.185ms
      for the borrowed path (1.2x faster). Progress 2026-07-06:
      `background-image` now parses borrowed simple values before
      `parse_background_image`. A release benchmark over 500k gradient token
      resolutions measured 71.205ms for the string-clone path versus 62.134ms
      for the borrowed path (1.1x faster). Progress 2026-07-06: edge
      shorthands (`padding`, `margin`, `border-width`, `inset`) now parse
      borrowed simple values before `parse_edges_shorthand`. A release
      benchmark over 500k edge shorthand token resolutions measured 120.785ms
      for the string-clone path versus 112.609ms for the borrowed path (1.1x
      faster). Progress 2026-07-06: `border-radius` shorthand now parses
      borrowed simple values before `parse_corners_shorthand`. A release
      benchmark over 500k corner shorthand token resolutions measured 85.640ms
      for the string-clone path versus 82.162ms for the borrowed path (~4%
      faster). Progress 2026-07-06: `border` and `border-color` now parse
      borrowed simple values before `apply_border_shorthand` /
      `parse_border_color_shorthand`. A release benchmark over 500k border
      shorthand token resolutions measured 111.028ms for the string-clone path
      versus 96.613ms for the borrowed path (1.1x faster). Progress
      2026-07-06: `transform-origin` now parses borrowed simple values before
      `parse_transform_origin`. A release benchmark over 500k transform-origin
      token resolutions measured 137.328ms for the string-clone path versus
      129.423ms for the borrowed path (1.1x faster). Progress 2026-07-06:
      `transform` now parses borrowed simple values before `parse_transform`.
      A release benchmark over 500k transform token resolutions measured
      120.761ms for the string-clone path versus 104.237ms for the borrowed
      path (1.2x faster). Progress 2026-07-06: `box-shadow` now parses
      borrowed simple values before `parse_box_shadow`. A release benchmark
      over 500k box-shadow token resolutions measured 144.953ms for the
      string-clone path versus 130.210ms for the borrowed path (1.1x faster).
      Progress 2026-07-06: `flex` now resolves simple values through the
      borrowed path before applying shorthand semantics. A release benchmark
      over 500k flex token resolutions measured 80.558ms for the string-clone
      path versus 73.508ms for the borrowed path (1.1x faster). Progress
      2026-07-13: `apply_flex_shorthand` now reads the first three whitespace
      fields directly instead of collecting all tokens into a temporary `Vec`.
      A release microbenchmark over 2M flex shorthand parses measured
      150.266ms for `Vec` collect versus 129.296ms for iterator fields (1.2x
      faster for that parser subpath). Progress 2026-07-13:
      `parse_overflow_shorthand` now reads the first two whitespace fields
      directly instead of collecting into a temporary `Vec`. A release
      microbenchmark over 3M overflow shorthand parses measured 81.771ms for
      `Vec` collect versus 61.478ms for iterator fields (1.3x faster for that
      parser subpath). Progress 2026-07-13: `parse_transform_origin` now reads
      the first two whitespace fields directly instead of collecting into a
      temporary `Vec`. A release microbenchmark over 3M transform-origin
      parses measured 93.661ms for `Vec` collect versus 65.960ms for iterator
      fields (1.4x faster for that parser subpath). Progress 2026-07-13:
      animation keyword
      properties (`transition-timing-function`, `animation-name`,
      `animation-timing-function`, `animation-iteration-count`,
      `animation-direction`, `animation-fill-mode`, `animation-play-state`) now
      use the borrowed simple-value resolver before parsing first-list-item
      keywords. A release benchmark over 500k timing-function token
      resolutions measured 29.373ms for the string-clone path versus 25.064ms
      for the borrowed path (1.2x faster).
- [x] **Theme component defaults re-applied per node from string maps.**
      `apply_theme_component_defaults` parses `HashMap<String, String>`
      defaults on every node resolution (already visible in the
      post-2026-06-10 toggle profile note above). Pre-bake per-tag
      `ComputedStyle` prototypes once per theme change and start resolution
      from a memcpy of the prototype instead of re-applying string
      declarations.
  - [x] 2026-07-05: no-diagnostics theme default application now applies
        borrowed property names directly instead of constructing a temporary
        `Declaration` for every default on every node. A release benchmark over
        200k default applications measured 165.045ms for declaration allocation
        versus 154.446ms for direct property application (1.1x faster).
  - [x] 2026-07-09: each `StyleResolver` now pre-bakes and caches the resolved
        `ComputedStyle` plus custom-variable seed per `(module, tag)`, then
        clones that prototype before applying matched rules. Resolver-local
        caches naturally invalidate on theme/prop rebuilds and keep module
        defaults isolated. A release benchmark over 200k cache-hit resolutions
        measured 2.931s reapplying an eight-property string map versus 29.740ms
        from the prototype (~98.5x faster).
### F. Animation & layout per-frame overhead

- [x] **Retained Taffy layout still re-syncs every node's style per pass.**
      `compute_incremental` → `update_retained_node_styles` walks the whole
      tree rebuilding `taffy_style_for_node` and re-populating
      `node_map`/`text_nodes` HashMaps on every layout-dirty frame
      (`ui/elements/src/layout.rs:346-390`), even when one node changed.
      Feed the retained-tree dirty set (already computed in
      `RetainedWidgetTree::update`) into layout so only dirty nodes get
      `set_style` calls — Taffy caches internally, but MESH pays the full
      style-conversion walk. (Structural rebuild case is tracked at v1.21;
      this is the _non-structural_ per-frame cost.) Progress 2026-07-04 for the
      paint-only case: when available geometry and layout dirtiness are both
      unchanged, `compute_incremental` now returns before rebuilding node/text
      maps, converting styles, or calling Taffy `set_style`. A layout-dirty or
      resized frame still synchronizes the full retained tree immediately
      before layout, preserving deferred correctness. The 1,365-node release
      microbenchmark over 2,000 paint-only passes measured 378.430ms for the
      old synchronization walk versus 40.369us for the fast path (9,374x).
      Dirty-node-only synchronization within actual layout passes remains a
      possible follow-up once retained-tree dirty IDs are exposed here.
      Rejected experiment 2026-07-04: retaining and clearing the temporary
      `node_map`/`text_nodes` allocations made the end-to-end layout pass
      slightly slower (77.502ms scratch versus 77.033ms fresh, 0.99x), because
      full style synchronization and map clearing dominate. The prototype was
      reverted. Completed 2026-07-12 with the T-layout closure:
      non-structural layout frames now feed retained layout-relevant dirty
      `NodeId`s into `LayoutEngine::compute_incremental_with_dirty_nodes`, so
      Taffy `set_style`/`mark_dirty` is limited to dirty nodes while structural
      uncertainty still falls back to the full retained sync. `node_map` and
      text measurement context remain populated for all retained nodes because
      they are needed by layout output and measurement callbacks. Verification:
      `cargo test -p mesh-core-elements retained_layout` passes; shell crate
      verification remains blocked by the missing `xkbcommon.pc` system
      dependency.

### G. Lua runtime — state sync & handler overhead

### H. Presentation & memory

- [ ] **Extra full-buffer memcpy per present.** Skia paints into
      `PixelBuffer`, then `copy_bgra_to_canvas`/`copy_bgra_damage_to_canvas`
      memcpys into the SHM mapping (`presentation/src/wayland_surface/backend.rs:514-646`).
      The damage-scoped copy path is good, but full-present frames (first
      paint, resize, fractional scale until fixed) pay paint + full copy.
      Have Skia render directly into the mapped SHM canvas
      (`with_skia_canvas` over the pool slot) for the active buffer,
      keeping `PixelBuffer` only as the retained/compare copy — or adopt
      double-buffered direct paint once damage tracking is per-buffer.
- [ ] **SHM pool thrash on resize.** Any size change clears and re-creates
      all `SHM_BUFFER_POOL_DEPTH` buffers (`backend.rs:251-260`). A
      content-measured surface that animates its size (expanding popover,
      growing launcher list) reallocates the whole buffer set every frame.
      Round buffer allocation up to size classes (e.g. next-64px) and
      present with viewport crop, so gradual resizes reuse allocations.
- [ ] **Startup compiles modules serially.** Module discovery + `.mesh`
      parse + compile runs one directory at a time on the main thread
      (`shell/discovery.rs:126+`). Parse/compile are pure per-module —
      parallelize with rayon/spawn_blocking to cut shell start latency
      (matters for session startup perception vs. quickshell).
      Progress 2026-07-05: `FrontendCatalog::from_modules` now compiles the
      sorted frontend module set with an indexed Rayon pipeline, preserving
      deterministic collection/error order while running independent compile
      work concurrently. A release benchmark over 20 builds of the nine
      shipped frontend modules measured 63.48ms sequential versus 32.70ms
      parallel (~1.9x faster). Directory discovery and manifest parsing remain
      serial.

### I. Composition, display list & proxies — third-pass findings

- [ ] **No component-level render memoization — the strategic gap.** Every
      surface rebuild re-evaluates _every_ embedded/local component's
      template from scratch: `render_import`
      (`shell/component/composition.rs:12-100`) re-clones props into a fresh
      `HashMap<String, serde_json::Value>`, re-`format!`s instance keys,
      re-runs `bind_child_instance`, and re-renders the child subtree even
      when that instance's props and script state are untouched. This is why
      one reactive variable changing anywhere re-costs the whole surface.
      Each `EmbeddedFrontendRuntime` already has a
      `ScriptState::mutation_generation`; cache each instance's built
      subtree keyed by (props fingerprint, state generation, locale/theme
      generation) and reuse it wholesale on rebuild. This is the
      component-granular complement to the v1.27 node-level narrow re-eval
      and probably the single largest structural win for complex surfaces.
      Progress 2026-07-05: embedded/local runtime prop sync now uses a single
      runtime-map lock for existing instances and applies props directly to a
      newly-created runtime after its render hook, instead of inserting and
      looking it up again. A release microbenchmark over 1M existing-instance
      updates measured 26.851ms for `contains_key` + second `get_mut` lock
      versus 12.362ms for one `get_mut` lock (2.2x faster). Full subtree
      memoization remains open. Progress 2026-07-05: prop-bound handler
      matching now scans borrowed event-handler maps and only clones matched
      handler entries instead of cloning each node's full handler map before
      checking for matches. A 65-node no-match release benchmark over 50k
      passes measured 1.089s for clone-then-scan versus 877.216ms for borrowed
      scan (1.2x faster). Full subtree memoization remains open.
- [ ] **Display payload text still clones string attributes.** Display-entry
      comparison and paint-node creation still clone/deep-compare per-entry
      strings (`content`/`value`/`src`/`name`). Consider sharing node text via
      `Arc<str>` between `WidgetNode` and display entries, or introducing a
      compact interned attribute payload, after auditing the `WidgetNode` and
      renderer payload contract. Progress 2026-07-05: display primitive
      signatures now hash paint payload attributes by tag instead of hashing
      text/input/icon/slider attributes for every node. This also avoids
      display-list churn when irrelevant payload-like attributes change on
      generic nodes. A mixed 512-node release benchmark over 20k signature
      passes measured 925.336ms for all-payload-attrs hashing versus
      219.760ms for tag-aware hashing (4.2x faster). Actual string sharing
      remains open. Rejected experiment 2026-07-05: replacing payload-owned
      strings with `Arc<str>` made a two-string input-payload clone slower:
      67.591ms versus 63.400ms for owned `String` clones over 5 million
      iterations (6.6% regression). A useful solution must avoid node-to-payload
      allocation without adding atomic bumps to every command clone.
- [ ] **Storage reads clone per Lua access.** `self.storage.key` reads lock
      the storage mutex and clone the JSON value per access
      (`scripting/storage.rs:275-307`); render hooks that read storage pay
      this per frame. Minor today; becomes visible once handlers use
      storage more. Consider caching the storage table Lua-side and
      invalidating on write. Rejected experiment 2026-07-04: a private
      Lua-table cache for scalar values measured 50.238ms versus 39.202ms for
      the existing lock/clone/convert path (0.8x); the extra Lua lookup cost
      outweighed the saved Rust work, so the prototype was reverted. A future
      attempt should target shared immutable JSON values or lock avoidance
      without adding another Lua table lookup. Progress 2026-07-12:
      `self.storage.snapshot` is now installed as a table-owned Lua function
      once instead of being allocated from `__index` on every method lookup.
      This does not change scalar storage read cloning, but removes repeated
      function allocation for render hooks that call `self.storage:snapshot()`.
      Added coverage that snapshot lookup does not track a storage-key read
      plus a release-only benchmark; the local dev-shell run measured
      602.437ms for per-lookup function creation versus 548.181ms for the
      table-owned method over 100k snapshot calls (1.1x faster).
### J. Algorithmic complexity — quadratic hot-path patterns (fourth pass)

Targeted scan for accidentally-super-linear loops. These compound with each
other: an uncoalesced motion event multiplied by an O(depth × n) hover dispatch
multiplied by O(n) tree clones is where interaction latency actually goes.

- [ ] **Runtime key paths make deep trees O(n × depth).** Every node's key
      is the full slash-joined ancestor path built with
      `format!("{key}/{index}")` and FNV-hashed from scratch per node per
      frame (`runtime_tree.rs:616-622,281-291`) — a 10-deep list row hashes
      ~40-byte strings for every row every frame, and key length grows with
      depth. Derive ids by hash-chaining `(parent_id, child_index)` — O(1)
      per node, no string at all — and keep the string path only for debug
      builds / diagnostics. Progress 2026-07-04: runtime node IDs now hash-chain
      `(parent_id, child_index)` instead of rehashing each full ancestor path.
      IDs remain deterministic, nonzero, and sibling-distinct. A 10-level
      release microbenchmark over 500k iterations measured 36.394ms for full
      path hashing versus 5.755ms for parent chaining (6.3x faster). String
      paths are still built because interaction state and refs currently use
      them as public runtime keys; removing those allocations remains open.
      Progress 2026-07-10: runtime annotation now builds those public key
      paths with one mutable string buffer instead of allocating a formatted
      child key at every edge. Added key-string regression coverage and a
      release-only benchmark; the local dev-shell run measured 1.091s for
      `format!("{key}/{index}")` versus 421.848ms for append/truncate over
      20k broad-tree iterations (2.6x faster). Progress 2026-07-13: scroll
      overflow annotation now uses the same append/truncate path strategy and
      avoids cloning existing scroll-offset keys during steady-state lookups.
      A release benchmark over 20k broad-tree annotation passes measured
      988.379ms for recursive formatted child keys versus 710.105ms for the
      path buffer (1.4x faster).
- [ ] **`finalize_tree` closing-popover pass: O(closing-keys × tree)**
      `find_node_by_key_mut` per closing key (`rendering.rs:273-279`).
      Trivial count in practice; fold into the fused annotation walk (D)
      rather than fixing separately. Rejected experiment 2026-07-05: replacing
      the per-key searches with one full-tree inherited-state traversal was
      slower for the realistic small-key case (855.098ms existing per-key
      search vs 1.035s one-walk over 2k broad-tree iterations), so the
      prototype was reverted.
- [ ] **Slider drag worst case = every quadratic above at once.** Each
      uncoalesced motion during a drag runs slider-value tree walks ×3, a
      handler call (Lua + full `sync_state_from_lua`), then
      `invalidate_script_state()` → full template rebuild + restyle + layout + paint (`input/mod.rs:163-186`). With motion coalescing (above) plus
      routing slider drags through the STATE/interaction-restyle path
      instead of SCRIPT invalidation (the knob position is
      shell-owned state — `slider_values` — not script state), a drag frame
      should cost a targeted restyle, not a rebuild.
      Progress 2026-07-10: handlerless slider press/move frames now invalidate
      through interaction restyle instead of unconditional script rebuild,
      while sliders with `change`/`release` handlers preserve the script
      invalidation path so reactive labels still update. Added policy tests for
      both paths and a release-only repaint benchmark; the local dev-shell run
      measured 790.919ms for forced script rebuild versus 213.822ms retained
      interaction repaint over 200 handlerless drag frames (3.7x faster).

### K. Threading & repaint suppression (fifth pass)

MESH is effectively single-threaded for all UI work: script execution, tree
build, restyle, layout, Skia raster, and present for **every surface** run
serially inside `Shell::run` on the main thread (`shell/runtime/mod.rs:173+`).
The Tokio runtime (`runtime/mod.rs:182`) only hosts backend pollers and IPC.
QtQuick's render loop, by contrast, splits scene-graph sync from rendering.
The Lua VMs are `!Send` and must stay on the shell thread — but everything
after the display list is built does not.

- [ ] **Parallelize paint across surfaces.** After `finalize_tree`, painting
      is pure: display list + `PixelBuffer` in, pixels out. Surfaces are
      independent (own buffer, own damage). Restructure `render_components`
      into two phases — phase 1 (serial, VM-bound): script hooks, build,
      restyle, layout, display-list update per dirty surface; phase 2
      (parallel): `paint_pixel_regions` + SHM copy per surface via rayon
      scope. The painter's text/glyph/gradient caches are already
      `thread_local` (`painter/backend.rs:29`, `text.rs:28-48`), so worker
      threads get their own — verify cache hit rates don't crater with a
      pinned worker-per-surface mapping. Bar + popover + launcher painting
      concurrently roughly divides paint latency by the surface count.
- [ ] **Pipeline paint against the next frame's script work.** Even with one
      surface, phase 2 for frame N can overlap phase 1 of frame N+1 (double-
      buffer the `PixelBuffer`, hand the display list snapshot to a render
      thread, present from there). This is the classic guarded-render-loop
      design; it halves effective frame latency for rebuild-heavy frames.
      Bigger lift than per-surface parallelism — do that first.
- [ ] **Tile-parallel raster for large damage.** Within one buffer, split
      full-surface repaints (theme change, first paint, launcher open) into
      horizontal bands painted in parallel (disjoint `&mut [u8]` slices via
      `split_at_mut`; each band gets its own Skia canvas with a band clip).
      Only worth it above a damage-area threshold; measure with the v1.21
      profiles first.
- [ ] **Move blocking file IO off the shell thread.** `load_graph_i18n_catalogs`
      does `fs::read_to_string` per catalog on mount (`component/runtime.rs:136-171`),
      settings/theme reloads re-read files inline in the loop, and icon/SVG
      cache _misses_ rasterize on the paint path. Route one-shot IO through
      `spawn_blocking` with a completion event (the loop already wakes on
      eventfd), and make icon-cache misses paint a placeholder frame and
      fill in on the next wake instead of stalling the frame.
### L. Live performance debugging — design

Goal: see hotspots _live_ while interacting with the shell, with cause
attribution (which rule, which component, which invalidation), without the
measurement tool perturbing what it measures. Builds on what already exists:
`ProfilingStage` accumulators + `ProfilingSnapshot` (`runtime/profiling.rs`),
`ProfilingInvalidationSnapshot` (per-paint rebuild/retained/narrow/damage
counts), the `DebugOverlay` painter, `mesh.debug.*` IPC, and the
debug-inspector's profiling start/stop. Tiered by effort:

- [ ] **Tier 1 — in-shell perf HUD painted by the renderer, not a module.**
      A HUD that is itself a `.mesh` surface would pollute the numbers with
      its own rebuild/restyle cycle at every update. Instead extend the
      existing `DebugOverlay` (which already paints layout bounds directly
      into the buffer post-paint, `frontend/render/src/surface/debug_overlay.rs`)
      with a profiling mode, toggled by the existing `CoreRequest` debug
      path: - **frame waterfall strip**: last ~120 frames as stacked bars (script /
      build / restyle / layout / display-list / paint / SHM / present),
      color-coded, 16.6 ms budget line — the data is already in
      `ProfilingSurfaceSnapshot.recent_samples`, it just needs a ring
      buffer keyed by frame rather than by stage; - **live counters**: FPS, presents vs skipped, damage area % of
      surface, retained-path vs full-rebuild ratio, narrow-path hits — all
      already in `ProfilingInvalidationSnapshot`, currently only visible in
      the inspector module; - **paint flashing** (the Chrome/KWin repaint debugger): translucent
      colored overlay on each frame's damage rects, decaying over ~300 ms.
      This makes "we repainted the whole bar for a clock tick" _visible_
      instantly, and is the single best tool for the repaint-suppression
      work in K. Trivial to add: the damage rects are already in
      `last_present_damage_rects` when the overlay paints.
      HUD paint cost must be excluded from the recorded stages (paint it
      after `PaintTraversal` is recorded) and its damage must not feed back
      into the damage stats (flag its rects).
- [ ] **Tier 2 — cause attribution (top-N tables).** Stages say _what phase_
      is slow; attribution says _why_: - per-style-rule cumulative restyle time + match count (time
      `apply_declaration` per rule id in the cached index; report top 10
      selectors); - per-component-instance build time (wrap `render_import`/embedded
      instance eval — directly measures the memoization win in I); - per-node paint time bucketed by command kind (text/shadow/blur/
      gradient/icon) — the painter already returns `PaintMetrics` with
      shaping/raster micros, extend to per-kind totals; - wasted-work counters: rebuilds whose retained diff was empty,
      restyles with zero changed styles, service deliveries whose payload
      was identical (K), motion events coalesced vs dispatched (J).
      Surface these in the HUD's second page and in the IPC snapshot.
- [ ] **Tier 3 — streaming + offline analysis.** - `mesh.debug.profiling_stream`: push per-frame profiling records over
      the existing IPC bus so an external `mesh-tools-cli perf top`
      TUI can show live tables without any in-shell UI (and without the
      HUD's paint cost); - Chrome-trace/Perfetto JSON export of a captured window (the
      `ProfilingSample` ring buffers already hold timestamps+durations) for
      offline flamegraph comparison before/after each A–K fix; - wire the existing `DebugBenchmarkSnapshot`/`BenchmarkScenarioSnapshot`
      types to the canonical-workload profiles item (v1.21): scripted
      scenarios (idle 10 s, pointer sweep, slider drag, popover open/close,
      theme switch) that run headless and emit a JSON summary — this is the
      regression harness that keeps the wins from A–K from rotting.
      Compare runs in CI against a stored baseline with a tolerance band.

### M. Component composition & template evaluation — 2026-07-04 deep dive

Focused trace of compile → `build_tree_with_state` → `build_widget_node` →
`FrontendCompositionResolver::render_import` → finalize (see
`PERFORMANCE_SECTIONS.md` §1 for the section map). New findings not covered by
passes A–L; `file:line` as of this scan.

Performance:

- [x] **Full layout per embedded instance per rebuild.** `build_tree_with_state`
      always ends with `LayoutEngine::compute_with_measurer`
      (`frontend/compiler/src/lib.rs:203`) and `render_embedded_instance` calls
      it per embedded module instance mid-build; `finalize_tree` then re-lays-out
      the whole tree (`shell/component/rendering.rs:460`). Embedded subtrees get
      ≥3 layout passes per rebuild (+1 per nesting level). Verify nothing reads
      `node.layout` between build and finalize, then skip the build-time layout
      for `FrontendRenderMode::Embedded` (and likely the surface build too).
      Progress 2026-07-09: embedded builds now defer layout until the composed
      surface finalization pass; standalone surface/preview builds retain their
      existing eager layout contract. Added regression coverage for both modes
      and a release-only benchmark: a 513-node embedded tree over 200 builds
      measured 2.757s with eager layout versus 2.447s deferred (~1.13x faster).
- [x] **`{#for}` deep-clones the whole items array every rebuild.**
      `store.get(&for_node.iterable)` (`frontend/compiler/src/render.rs:429`)
      uses owned `get` although borrowed `get_ref` exists and is already used by
      `eval_path`. Switch to `get_ref`; trivial diff. Completed 2026-07-06:
      `{#for}` now borrows iterable arrays through `VariableStore::get_ref`
      when available, keeps an owned fallback for stores that only implement
      `get`, and stores loop items by reference in `LayeredStore`. Added
      regression coverage for the no-owned-root-clone path plus fallback
      coverage for owned-only stores. Release-only benchmark on a clone-heavy
      1k-item array showed borrowed iteration ~1.2x faster locally
      (5.12s -> 4.25s for 80 rebuilds); a small-payload full-render benchmark
      was layout/tree-build dominated and did not show a win, so this is
      specifically an allocation/clone-heavy iterable improvement.
- [x] **Post-hoc full-subtree walks per embedded instance.**
      `namespace_event_handlers` re-`format!`s every handler string on every
      rebuild (`ui/interaction/src/hit_test.rs:359`) even though
      `build_widget_node` already receives `instance_key` — namespace during
      `parse_attributes` instead and the walk disappears.
      `apply_prop_handler_calls` clones each node's whole `event_handlers` map
      and does an O(handlers × props) scan per node
      (`shell/component/composition.rs:213-239`).
      Progress 2026-07-09: imported and local embedded component handlers are
      now namespaced during attribute construction, removing both recursive
      post-build walks while preserving generic preview builds and prop-handler
      linkage. A 512-handler release benchmark over 2,000 rebuild-shaped tree
      clones measured 792.876ms with the post-build walk versus 697.654ms with
      inline namespacing (~1.14x faster). The `apply_prop_handler_calls`
      matching pass now builds one handler-value index per embedded subtree
      instead of repeating an O(handlers × props) scan at every node, while
      preserving first-prop-wins behavior for duplicate bindings. With 16
      handler props and 64 child nodes, 20k release iterations measured
      942.132ms repeated scan versus 524.108ms indexed (~1.8x faster). The
      dominant single-prop case bypasses hash-index construction and compares
      against one resolved handler directly; 50k release iterations over 64
      children measured 1.238s repeated map scans versus 1.159s specialized
      (~1.07x faster).
- [ ] **Per-rebuild prop churn.** `ensure_runtime`/`ensure_local_component_runtime`
      re-`set` every prop into script state per instance per rebuild with 2–3
      runtimes-mutex acquisitions (`shell/component/runtime.rs:408-415`);
      `render_import` rebuilds `props_json` maps and `format!`s instance keys per
      frame (`composition.rs:25-38,90-98`); host+component style-rule slices are
      re-cloned into a merged `Vec` per instance per rebuild
      (`render.rs:266-278`) — cacheable per (host, alias). Progress
      2026-07-06: local-component rendering now threads the already-resolved
      `ComponentFile` and host style-rule slice from `render_import` into
      `render_local_component`/`ensure_local_component_runtime`, removing the
      duplicate host-module and local-component catalog lookups from each local
      component rebuild. Prop map construction, prop state writes, and style-rule
      merge caching remain open. Progress 2026-07-09: embedded and local
      component prop synchronization now skips unchanged public-member writes,
      avoiding JSON-to-Lua conversion, `_ENV` mutation, and module-object
      rebuilding on steady-state parent rebuilds. A release benchmark over
      100k unchanged structured prop writes measured 119.964ms eager versus
      12.451ms equality-gated (~9.6x faster). Prop map construction and
      style-rule merge caching remain open. Progress 2026-07-09: CSS prop
      projection now borrows the runtime's `props` object and only falls back
      to an owned lookup for non-lending stores/live proxies, avoiding a deep
      clone of unrelated instance props per component build. With one declared
      prop in a 129-entry structured props object, 10k release projections
      measured 208.842ms owned versus 1.485ms borrowed (~140.6x faster).
      Progress 2026-07-10: imported/local component prop-map construction now
      uses a shared helper that pre-counts public props and allocates the
      runtime `HashMap` at the right capacity while consistently filtering
      internal binding channels. A release benchmark over 100k rebuild-shaped
      64-prop maps measured 591.288ms for filtered `collect` versus 429.191ms
      for the pre-sized helper (1.4x faster). Prop state writes and style-rule
      merge caching remain open.
- [ ] **Per-node build allocations.** `attach_module_id` inserts a fresh
      `_mesh_module_id` String on every node; `TrackingVariableStore` pushes two
      fresh Strings per dotted read per node; `resolve_event_handler_value` does
      an owned `store.get` per handler attribute. Folds into v1.23 interning but
      listed because composition keeps adding string attributes.
      Progress 2026-07-09: event-handler resolution now prefers
      `VariableStore::get_ref` and falls back to owned `get` only for stores
      that cannot lend a value (such as live proxies). A release benchmark over
      1M handler lookups measured 16.593ms with owned JSON cloning versus
      8.630ms borrowed (~1.9x faster). Consecutive duplicate service-field
      reads within one node are now coalesced before allocating service/field
      strings; 1M repeated reads measured 93.496ms eager versus 11.050ms
      coalesced (~8.5x faster). Progress 2026-07-09: non-consecutive duplicate
      service-field reads within one node are also coalesced before allocation,
      so interleaved expressions like `audio.percent`, `network.ssid`,
      `audio.percent` publish two dependencies instead of three. A release
      benchmark over 250k node-shaped read batches measured 22.424ms
      consecutive-only versus 21.995ms duplicate-scan (~1.0x faster) while
      reducing dependency entries from 1,000,000 to 750,000. Progress
      2026-07-12: `WidgetNode` now carries a typed `module_id` field with
      legacy `_mesh_module_id` attribute fallback, and frontend build/style
      resolution paths use the typed field for normal trees. This removes the
      fresh `_mesh_module_id` attribute insertion per built node while
      preserving hand-built test/tooling compatibility. A release benchmark
      over 500k build-shaped node clones with existing attributes measured
      152.730ms for attribute-map insertion versus 141.105ms for typed-field
      assignment (1.1x faster), before downstream savings from the smaller
      attribute map. Unique tracked-read string allocations remain open.

Structure / correctness:

- [ ] **`and`/`or` template expressions diverge from Lua semantics.**
      `eval_compiled` returns literal `"true"`/`"false"` for `And`/`Or`
      (`frontend/compiler/src/expr.rs:193-204`) instead of the operand values —
      `{name or "Anonymous"}` renders `true`/`false`; only the exact
      `cond and a or b` ternary shape is special-cased to work. Also
      `is_truthy` treats `"0"`/`""` as falsy (Lua does not), and
      `a or b and c` parses with inverted precedence (`and` split before `or`).
      Fix as part of the typed expression-value enum (section A, "stringly-typed
      template expression values") — that item is now correctness work, not just
      an optimization. Doc example using unsupported C-style `?:` fixed in
      `docs/frontend/mesh-syntax.md` 2026-07-04.
- [ ] **Build is not a pure function — prerequisite for render memoization.**
      `render_import` mutates shell state during build via RefCells
      (`pending_surface_states`, `portal_hidden_bindings`,
      `has_promoted_popover_wrappers`, live `bind:this` installation —
      `composition.rs:74-131`). Component-level memoization (section I) would
      silently skip these side effects when serving a cached subtree; make them
      explicit build outputs (a `BuildEffects` struct the caller applies) first.
- [ ] **Typed handler-call linkage matches by value equality.**
      `apply_prop_handler_calls` maps typed args onto child handlers by
      comparing resolved handler *values* to prop values
      (`composition.rs:221-235`); two props bound to the same handler name get
      the wrong args. Link by prop name through the child build instead.
- [x] **Remove the legacy JSON handler-descriptor path.** `unpack_handler_args`
      still parses `{"h":…,"a":…}` strings (`shell/component/runtime.rs:644-664`)
      after typed `EventHandlerCall` landed (section G). Per the
      no-backward-compat project rule, verify nothing produces them and delete.
      Completed 2026-07-06: verified the compiler/shell emit typed
      `EventHandlerCall`s and removed JSON descriptor parsing from dispatch;
      `unpack_handler_args` now always borrows the handler name and event args.
      Release benchmark over 200,000 pre-bound calls measured 95.353ms for the
      old JSON descriptor unpack versus 27.112ms for typed handler-call args
      (3.5x faster).
- [ ] **`{#if}`/`{#for}` always wrap children in a synthetic `column` node**
      (`render.rs:394,423`) — one extra node per conditional/loop paying layout,
      style, hash, and paint, and it forces column flow inside row parents.
      Needs a fragment/transparent-container concept.
- [ ] **No keyed list diffing.** `{#for}` identity is positional (`_mesh_key`
      paths), so any reorder/insert re-styles and re-hashes every following row.
      Add a `key=` attribute; pairs naturally with component memoization
      (section I) and the retained-tree diff work (v1.27).
- [ ] **Magic-string protocol at the composition boundary.**
      `__mesh_embed__::`, `__mesh_binding_*`, `__mesh_bind_this`,
      `_mesh_module_id`, the promoted-popover marker — stringly-typed channels
      between compiler and shell causing prefix parsing and false attribute
      dirtiness. The composition-boundary instance of v1.23 typed fields.
- [x] **Verify dynamic `class="{expr}"` bindings participate in build-time style
      resolution.** Completed 2026-07-09: build-time style matching now derives
      selector class/id identity from resolved dynamic attributes before calling
      `resolve_node_style_for_module_indexed`, so dynamic class styles are correct
      in the initial tree instead of depending on a corrective finalize restyle.
      Added `dynamic_class_participates_in_initial_style_resolution`.
- [x] Minor: `render_import`'s local-component branch does its catalog lookups
      twice (gate in `composition.rs:22-23`, again inside
      `render_local_component`, `runtime.rs:435-440`). Completed 2026-07-10:
      the local branch now threads the already-resolved `ComponentFile`,
      host manifest, and host style-rule slice into `render_local_component`,
      so the duplicate catalog lookup path is gone.

### N. Retained tree, render objects & display list — 2026-07-04 deep dive

Focused trace of annotate → `RetainedWidgetTree::update` → `RenderObjectTree`
→ `RetainedDisplayList` → damage (see `PERFORMANCE_SECTIONS.md` §2). New
findings beyond the D/I/J items; `file:line` as of this scan.

Performance:

- [x] **`ordered_entries` is built per display-list rebuild but consumed only in
      debug builds.** `collect_display_entries` pushes every `(key, entry)` pair
      into a Vec (`render/src/display_list.rs:770-774`) whose sole consumer is
      `compute_batch_metrics` behind `#[cfg(debug_assertions)]`
      (`display_list.rs:891-894`). Release builds pay a full per-entry Vec push
      every rebuild frame for nothing. Gate the collection itself (pass
      `Option<&mut Vec<_>>` or a debug-only sink). Free win.
      Completed 2026-07-05: release builds now compile out the ordered-entry
      scratch buffer and pass no debug sink during entry collection. Two
      release runs over 9.842 million collected entries measured 2.943s versus
      2.881s and 2.941s versus 2.930s (0.4-2.1% faster), with identical damage
      map entry counts.
- [x] **`RenderObjectTree` allocates per node per dirty frame.** `text_slot`
      clones the text `content` String (`render/src/render_object.rs:307`),
      `accessibility_slot` clones the label, `child_id_slot` allocates a fresh
      `Vec<NodeId>` per node (`render_object.rs:263-271`; the retained tree
      already uses an inline `SmallVec` for the same data), `geometry_slot`
      string-parses six `_mesh_scroll_*`/`_mesh_content_*` attributes per node
      (`render_object.rs:296-301`), and `update_inner` allocates two fresh
      `HashSet`s per update (`render_object.rs:97-98`) instead of scratch-reuse.
      This file predates the D-item optimizations and never got them.
      Progress 2026-07-05: geometry now consumes typed scroll metrics (34.5x
      faster than six string parses), dirty-node storage is scratch-reused, and
      text content, accessibility labels, and custom roles reuse retained
      `Arc<str>` allocations when unchanged. The retained-string benchmark
      measured 35.148ms for `String` clones versus 15.648ms for retained arcs
      over 5 million values (2.2x faster). An eight-entry `SmallVec` child-id
      experiment was rejected because it regressed the measured six-child case
      from 28.448ms to 37.879ms. Reusing the retained `Vec` allocation instead
      measured 54.663ms for fresh vectors versus 13.167ms for clear/refill over
      5 million six-child updates (4.2x faster), while preserving arbitrary
      child counts and reorder detection. All listed allocation sources are now
      addressed.
- [ ] **Triple full-tree fingerprinting on every dirty frame.** Three parallel
      diff systems each walk the whole tree and hash/compare overlapping data:
      `RetainedWidgetTree` snapshots (layout/style/attrs/children/state,
      `runtime_tree.rs:102-170`), `RenderObjectTree` paint-data slots
      (transform/clip/geometry/material/primitive/text, `render_object.rs:90-124`),
      and `RetainedDisplayList` per-(node, slot) entry signatures — which
      `collect_display_entries` recomputes for **every** node on any dirty frame
      (`display_list.rs:1384-1433`) even when the dirty set names one node.
      The retained-tree generation gates the clean-frame case only. Unify:
      make `RetainedWidgetTree` the single fingerprint pass and have the render
      object tree and display entries consume its per-node dirty flags,
      re-signing entries only inside dirty subtrees (plus scrolled/moved
      ancestors). This is the §2 complement of the v1.27 generation-aware diff.
      Progress 2026-07-05: display-list batch signatures now hash only the
      material fields relevant to each primitive slot, and entries that already
      carry a batch barrier skip batch-signature hashing entirely because the
      metric never compares them. A 512-node release benchmark over 50k
      background-slot signature passes measured 804.926ms for the previous broad
      material hash versus 69.806ms for the slot-aware hash (11.5x faster).
- [x] **Reused paint subtrees are cloned twice per clean node.**
      `build_paint_subtree`'s reuse path does `previous.clone()` then
      `next_subtrees.insert(id, reused.clone())`
      (`display_list.rs:1488-1491`) — Arc bumps plus span/kind vec copies for
      every clean node on every incremental rebuild. Insert once and return a
      cheap handle/index instead.
      Completed 2026-07-05: retained subtree maps now own whole-subtree `Arc`
      handles, so clean-node reuse clones one handle rather than cloning each
      shared command/kind/order field plus metadata. A release benchmark over
      10 million reuse clones measured 180.455ms fieldwise versus 28.002ms for
      the whole-subtree handle (6.4x faster).
- [x] **Two more full passes per display-list rebuild.**
      `build_command_spans(root, &subtrees)` walks the tree and
      `count_effect_overflow_commands` scans all commands
      (`display_list.rs:895-896`) on every rebuild; both derivable
      incrementally from the subtree reuse bookkeeping.
      Progress 2026-07-05: effect-overflow counts are now accumulated while
      building each retained subtree, composed from cached child counts, and
      reused directly on unchanged generations. A release benchmark over
      20,000 metric queries with nonzero effects measured 746.477ms for the
      former command scan versus 9.499us for retained reads, with identical
      totals. Command spans are now composed as subtree-relative metadata and
      exposed from the root through a shared handle, preserving exact span
      ordering while removing the second tree walk. A release benchmark over
      10,000 assemblies measured 608.869ms for traversal versus 36.806us for
      retained root handles, with identical 25.21 million aggregate spans.
- [x] **Scroll state round-trips float→string→float three times per node per
      frame.** Written as `"{:.2}"` strings in `annotate_runtime_tree`
      (`runtime_tree.rs:819-832`), re-parsed in `collect_display_entries`
      (`display_list.rs:1417-1426`), `build_paint_node` scrollbars (six
      `attr_f32` calls), and `geometry_slot` (six more). Also quantizes offsets
      to hundredths. The concrete §2 instance of the v1.23/v1.27 typed
      `WidgetNode` fields item.
      Completed 2026-07-05: `WidgetNode` now carries optional typed scroll
      metrics used by runtime annotation, overflow measurement, retained-tree
      invalidation, hit testing, render objects, display-list construction, and
      both painters. Legacy attributes remain a fallback for hand-built nodes.
      A release geometry benchmark over 2 million snapshots measured 186.504ms
      for six string parses versus 5.407ms for typed fields (34.5x faster), and
      offsets are no longer quantized to hundredths.
- [x] **Handler-call args re-serialize to JSON strings per fingerprint.**
      `attributes_fingerprint` does `arg.to_string()` per pre-bound arg per
      node per frame (`runtime_tree.rs:479`). Hash the `serde_json::Value`
      structurally instead.
      Completed 2026-07-05: typed handler arguments are hashed recursively by
      JSON variant and primitive value without allocating serialized strings.
      The release benchmark measured 415.585ms for `to_string` hashing versus
      94.309ms for structural hashing (4.4x faster).

Structure:

- [x] **The primitive-aware hasher improvement never reached the render crate.**
      `RuntimeTreeHasher` got word-at-a-time `write_*` methods (D, 1.9x), but
      `DisplaySignatureHasher` (`display_list.rs:1305-1325`) and
      `RenderObjectHasher` (`render_object.rs:51-70`) are still byte-at-a-time
      FNV copies. Either port the primitive methods or — better — share one
      hasher type; three hand-rolled FNV implementations is the maintenance
      smell that let this drift.
      Completed 2026-07-05: both render hashers now mix primitive values in one
      operation while preserving byte-wise hashing for strings and slices. A
      release benchmark over a representative primitive field mix measured
      3.072ms for the byte fallback versus 2.232ms word-at-a-time (1.4x
      faster across 5 million iterations).
- [x] **No `NodeId` collision detection.** Runtime ids are FNV/chained hashes
      of key paths (`runtime_tree.rs:346-365`) used as identity keys by all
      three retained systems and the display-list keys; a collision silently
      aliases two nodes (wrong reuse, wrong damage) with no diagnostic. Add a
      debug-build assertion where `node_keys` is populated.
      Completed 2026-07-09: retained snapshot collection now asserts in debug
      builds when two nodes produce the same `NodeId`, with regression coverage
      for duplicate-id detection. Release builds keep the previous zero-cost
      insert path.
- [x] **Identity travels as a string attribute.** `annotate_runtime_tree`
      writes `_mesh_key` into `attributes` (`runtime_tree.rs:711`) purely so
      interaction/refs/metrics can read identity back out of a string map,
      which in turn forced the `_mesh_key` hash-exclusion special case in
      `attributes_fingerprint`. Typed field on `WidgetNode` (v1.23) retires
      both.
      Completed 2026-07-12: `WidgetNode` now carries a typed `mesh_key`
      field with legacy `_mesh_key` attribute fallback for hand-built tests and
      tooling. Runtime annotation writes only the typed field, retained-tree
      fingerprints no longer special-case `_mesh_key`, and interaction,
      layout, style, animation, metrics/ref publication, keybind, widget
      navigation, and child-surface request paths read identity through
      `WidgetNode::mesh_key()`. A release microbenchmark over 500k assignments
      measured 48.154ms for `_mesh_key` BTreeMap insertion versus 37.245ms for
      typed field assignment (1.3x faster), before counting the downstream
      fingerprint/map-scan savings from the smaller attribute map.
- [ ] Minor: display-list `update_inner` is ~220 lines mixing diff, damage,
      and a ~30-field metrics struct assembly (`display_list.rs:742-961`);
      split when next touched.

### N addendum — 2026-07-04 second pass (display-list subtree internals)

- [ ] **Every rebuilt ancestor copies its entire descendant command list.**
      `PaintSubtreeBuilder::append_child` does
      `extend_from_slice(&child_subtree.commands)`
      (`display_list.rs:586-600`), so each ancestor's flat buffer holds copies
      of all descendant `DisplayPaintCommand`s, and `next_subtrees` retains a
      full flattened copy per node — O(n × depth) command storage and re-copy
      on every ancestor rebuild. This is the retained-memory face of the v1.21
      segment/rope item; fixing v1.21 should make per-node subtrees hold spans
      into shared storage, not owned flattened copies.
- [ ] **A dirty node rebuilds its entire subtree's paint segments.**
      `build_paint_subtree` passes `force_rebuild || node_is_dirty` down to all
      children (`display_list.rs:1563`), so a style-only change on a container
      (hover background) rebuilds every descendant's commands even though
      their geometry and content are unchanged. Only the dirty node's own
      commands need rebuilding when its layout/scroll/clip didn't change;
      children could be re-appended from the previous subtree. Progress
      2026-07-10: paint-only dirty parents now allow clean descendants to
      reuse retained subtrees, while layout/clip/transform/reorder dirty
      parents still force descendant rebuilds. Added regression coverage for
      both paths and a release-only benchmark; the local dev-shell run measured
      564.325ms for forced descendant rebuilds versus 226.123ms with clean
      descendant reuse over 1k paint-only dirty-parent rebuilds (2.5x faster),
      with rebuilt commands dropping from 514k to 2k.
- [x] **`DisplayPaintCommand` embeds a full cloned `DisplayPaintNode` per
      command.** `paint_node.clone()` per Node command
      (`display_list.rs:1524`), with the same node reused for the Scrollbars
      command — each clone copies text/placeholder Strings and the style
      block. Share via `Arc<DisplayPaintNode>` per node with per-command kind.
      Progress 2026-07-12: `DisplayPaintCommand` now shares
      `Arc<DisplayPaintNode>` handles, so node and scrollbar commands for the
      same widget reuse one paint-node payload and retained command-buffer
      clones copy an Arc handle instead of text/style payloads. The in-crate
      release benchmark is present, but this environment cannot link Skia test
      binaries because `freetype`/`fontconfig` are missing; a standalone
      release benchmark over 4M two-command clones measured 548.859ms for
      owned node clones versus 83.270ms for Arc-backed commands (6.6x faster).

### O. Style system & theming — 2026-07-04 deep dive

Focused trace of CSS parse → `StyleRuleIndex` → `StyleResolver` →
`ComputedStyle` (build, restyle, and diagnostics paths) plus theme defaults.
See `PERFORMANCE_SECTIONS.md` §3. `file:line` as of this scan.

Performance:

- [x] **Hidden second full restyle with per-node index construction on every
      rebuild frame.** `record_runtime_style_diagnostics` runs whenever a
      diagnostics sink is attached — which is always in production
      (`shell_component.rs:60`) — on every `"rebuild"`-trigger finalize
      (`rendering.rs:429-431`). It walks the whole tree re-resolving every
      node through the diagnostics path, which builds `StyleRuleIndex::new(rules)`
      **per node** (`resolve.rs:546`) — the exact O(nodes × rules) pattern the
      E-item fixed on the build path — plus a fresh `Vec<String>` classes clone
      and a fresh variables HashMap per node (`rendering.rs:584-590`,
      `resolve.rs:614`). Rebuild frames are the most common invalidation class
      (every service update / handler write). Fix in stages: thread the cached
      index through the diagnostics path; gate the pass on (rules generation,
      tree-structure generation) instead of every rebuild; long-term validate
      declarations once per rule at compile time and delete the runtime pass.
      Completed first stage 2026-07-06: added an indexed diagnostics resolver
      and threaded the existing cached `StyleRuleIndex` through the shell
      runtime style diagnostics walk, removing per-node index construction on
      rebuild diagnostics. Added parity coverage for indexed vs uncached
      diagnostics. Release-only microbenchmark showed cached-index diagnostics
      ~2.4x faster locally (652.3ms -> 270.3ms for 20k diagnostic resolutions
      over 80 rules). Remaining follow-up: gate diagnostics by rules/tree
      generation or move unsupported CSS validation fully static.
- [x] **Per-declaration static validation re-runs per node per pass.**
      `apply_declaration_no_diagnostics` runs `style_profile_status`,
      `is_supported_css_property`, `contains_deprecated_token_reference` (a
      string scan of the value), and `is_strict_animation_property` for every
      declaration of every matched rule on every node on every restyle
      (`resolve.rs:916-950`). All are pure functions of the declaration;
      precompute them once per rule into a validated/compiled declaration at
      rule-build time. Cheap first step toward the v1.23 typed-declarations
      item. Progress 2026-07-12: diagnostics resolution now consumes the same
      indexed declaration metadata as the no-diagnostics restyle path,
      preserving diagnostic messages while avoiding static property
      reclassification and per-declaration selector-string allocation on each
      matched node. Added parity coverage for unsupported properties and
      missing variables. Release benchmark over 200k diagnostic declaration
      applications measured 138.902ms for reclassification versus 76.267ms
      with indexed metadata (1.8x faster). Follow-up 2026-07-12:
      `StyleRuleIndex` now also precomputes selector diagnostic strings, so
      the diagnostics walk no longer formats compound selectors for every
      matched node. Release benchmark over 500k compound-selector lookups
      measured 45.176ms per-node formatting versus 214.902us from indexed
      strings (210.2x faster).
      Completed 2026-07-13: the remaining theme-default diagnostics and
      no-diagnostics paths now also apply through `IndexedDeclaration`
      metadata, and indexed style resolution uses one diagnostics-optional
      loop. Production restyle/default application no longer re-runs static
      declaration classification per matched node; the old uncached helper is
      retained only for direct declaration diagnostics/tests.
- [x] **`seed_module_theme_variables` allocates two Strings per module token
      per node per pass** — `format!("--{}", name.replace('.', "-"))`
      (`resolve.rs:857-876`). Precompute the CSS-variable-keyed token map once
      per theme load per module and seed by reference.
      Completed 2026-07-05: each `StyleResolver` now caches normalized
      CSS-variable keys and converted token values per module, preserving
      first-writer variable precedence. A release benchmark over 3.2 million
      token insertions measured 237.080ms for per-node normalization versus
      132.686ms from cached entries (1.8x faster).
- [x] **`seed_prop_variables` clones every prop key+value per node**
      (`resolve.rs:599-603`) even though props are per-instance constants for
      the whole pass. Seed once per pass or resolve through a layered lookup
      (props map consulted after scratch) instead of copying.
      Completed 2026-07-09: variable resolution now consults per-node scratch
      first and the resolver's immutable prop map second, preserving local
      custom-property override semantics without seeding clones. Embedded
      `var()`/`prop()` substitution uses the same layered lookup. With 32 props,
      200k release resolutions measured 582.988ms cloning per-node seeds versus
      20.304ms layered (~28.7x faster).
- [x] **`theme_reference_to_token_name` allocates and canonicalizes per
      `var()` reference per declaration per node** (`resolve.rs:1916-1922` +
      `css_custom_property_to_token_name` prefix tables). Double-key theme
      tokens by their CSS custom-property name at theme load, or intern the
      mapping, so hot lookups are a single hash probe.
      Progress 2026-07-09: `StyleResolver` now interns canonical token names
      per reference as `Arc<str>` and the simple string/color/number plus
      diagnostics paths reuse them. A release benchmark over 1M
      `--mesh-color-primary` mappings measured 49.820ms recanonicalizing versus
      15.449ms cached (~3.2x faster). Embedded multi-reference substitution
      now also routes through the resolver cache and the duplicate standalone
      helper was removed. A release benchmark over 300k embedded substitutions
      measured 96.285ms recanonicalizing versus 86.376ms cached (~1.1x faster).
      Follow-up 2026-07-12: `StyleResolver` now also caches resolved theme token
      values, so repeated global-token `var(--...)` paths skip both canonical
      name lookup and `Theme::token` lookup after the first hit. A release
      benchmark over 1M `--color-primary` lookups measured 33.179ms for
      cached-name + theme lookup versus 18.984ms from cached token values
      (1.7x faster). The corrected embedded-reference benchmark using real
      `--color-*` variables measured 202.594ms recanonicalizing versus
      158.496ms cached (1.3x faster). A theme-load double-keyed token map is no
      longer needed for the resolver hot path, but may still be useful if other
      call sites begin resolving CSS-variable token names directly.
- [x] Confirmed mechanism for the existing "pre-bake per-tag prototypes" item:
      `apply_theme_defaults_map_no_diagnostics` re-clones each default's
      property String and re-classifies its value per node per pass
      (`resolve.rs:901-914`), for "base" + tag + module-base + module-tag maps.
      Completed 2026-07-12: diagnostics style resolution now has the same
      per-tag/module theme-default prototype cache as the no-diagnostics path,
      including cached default diagnostics and seeded custom variables. A
      release benchmark over 200k diagnostic default resolutions measured
      1.304s replaying theme string maps versus 29.778ms from cached prototypes
      (43.8x faster).

Structure / correctness:

- [x] **Theme component defaults apply in nondeterministic order.**
      `ComponentDefaults = HashMap<String, String>`
      (`foundation/theme/src/lib.rs:12`) and `apply_theme_component_defaults`
      iterates it per node. A theme declaring an overlapping shorthand +
      longhand pair (e.g. `background` and `background-color`) on the same
      component resolves in random order per process run, and theme-CSS source
      declaration order is lost entirely at parse. Store defaults as an
      ordered `Vec<(String, String)>` preserving source order (CSS last-wins).
      Completed 2026-07-12: `ComponentDefaults` is now an ordered declaration
      collection with serde support for the existing map-shaped JSON/CSS theme
      format. CSS parsing preserves declaration order, duplicate properties
      move to their latest declaration position, and component-default
      iteration now applies authored CSS order deterministically.
- [x] **The diagnostics/no-diagnostics path duplication caused the drift.**
      Four near-identical function pairs (`resolve_node_style_with_attrs*`,
      `apply_theme_defaults_map*`, `apply_declaration_*`) exist so the
      diagnostics path could stay separate; that duplication is exactly where
      the per-node index rebuild survived. When fixing the first item, fold
      diagnostics into a sink parameter (`Option<&mut Vec<StyleDiagnostic>>`)
      on one path so the two cannot diverge again. Completed 2026-07-13:
      indexed style resolution now runs through one inner
      diagnostics-optional loop, indexed declaration application uses one
      optional diagnostics sink, and theme component-default application uses
      the same indexed declaration path for diagnostics and no-diagnostics
      prototype caches. The remaining uncached declaration-with-diagnostics
      helper is outside the production indexed restyle/default path. Verified
      with `cargo test -p mesh-core-elements indexed` and
      `cargo test -p mesh-core-elements theme_defaults`.
- [x] Design note (fine, but document): selector matching has no CSS
      specificity — candidate rules apply in source-index order (last wins),
      and descendant combinators are rejected at parse with a diagnostic
      (`ui/component/src/style.rs:100`). Worth one paragraph in
      `docs/spec/04-styling.md` so authors don't expect specificity semantics.
      Completed 2026-07-13: `docs/spec/04-styling.md` §6.1 already documents
      the supported selector subset, compile-time rejection of descendant/child
      combinators, and the no-specificity source-order cascade contract.

### P. Rendering & paint — 2026-07-04 deep dive

Focused trace of `paint()` → damage assembly → `paint_pixel_regions` →
display-list replay → Skia session → text/glyph/icon caches → buffer. See
`PERFORMANCE_SECTIONS.md` §4. `file:line` as of this scan.

Performance:

- [x] **File-backed icon draws stat() the filesystem every paint, even on
      cache hits.** Every draw computes `raster_file_key` → `file_freshness`
      → `std::fs::metadata` (`render/src/surface/icon.rs:134-145,179-190`),
      and SVG sources add a second freshness check via `svg_file_cacheability`
      (`icon.rs:211`). Freshness is part of the raster cache key, so a hit
      still pays the syscall; `cached_file_resource_opacity` (opaque-region
      derivation) stats again per present (`icon.rs:297-331`). A bar with ~10
      file icons at 60 Hz is 600–1800 blocking syscalls/s on the paint path,
      and a slow filesystem stalls the frame. Fix: TTL the freshness probe
      (re-stat at most every ~1s) or make invalidation event-driven through
      the shell's existing inotify hot-reload watcher, so steady-state paints
      do zero filesystem calls. Named-icon *font glyph* draws are unaffected
      (glyph caches key by path hash + axes). Completed 2026-07-06: file
      freshness probes now use a 1s TTL-backed LRU shared by image loading,
      raster-cache keys, SVG cacheability checks, and cached opacity lookups,
      so steady-state file-backed icon paints reuse the cached `(len, mtime)`
      instead of calling `metadata` per draw. Added focused regression coverage
      proving repeated raster keys reuse one freshness probe inside the TTL and
      still re-stat after expiry. Release-only benchmark over 50k raster keys
      measured 69.051ms with per-draw `metadata` versus 7.274ms with the TTL
      cache (9.5x faster, one real probe).
- [ ] **Child popup surfaces bypass the whole retained pipeline.**
      `paint_child_surface` (`shell/component/shell_component.rs:992-1027`)
      clears the entire child buffer and repaints the popover subtree through
      the immediate-mode `paint_frontend_tree_at_for_module` on every present,
      plus two full-tree walks (`find_node_by_key`, `find_node_bounds_by_key`)
      per child per frame. An open hover menu or quick-settings popover
      full-repaints at frame rate with no display list, no damage, no partial
      present. Route child targets through the same retained display-list +
      damage path as the parent (subtree-scoped), which also deletes the
      duplicate immediate-mode painter (structure item below).
- [ ] **Any non-clean frame bypasses all generation shortcuts.**
      `use_generation_shortcuts` requires `dirty_types.is_empty()`
      (`shell_component.rs:529-537,560-581`), so every interaction/animation/
      script frame runs `RenderObjectTree::update` and display-list entry
      collection as full-tree passes. This is the shell-side counterpart of
      the §N triple-fingerprint item — fixing §N must include widening this
      gate to per-node dirty scoping, not only the fully-clean case.
- [ ] **Rotation transforms allocate a temp `PixelBuffer` per node per
      frame** and recursively repaint the subtree into it before the rotated
      blit (`render/src/surface/painter/tree.rs:380-410`). Any animated
      rotation pays an allocation + full subtree repaint per frame; reuse a
      cached temp buffer keyed by size class. Low priority until rotation is
      used in shipped surfaces. Rejected experiment 2026-07-12: adding a
      reusable scratch `PixelBuffer` slot to `FrontendRenderEngine` preserved
      ownership/recursion safety, but an optimized standalone benchmark that
      mirrored `PixelBuffer` allocation/clear measured 2.586ms for fresh
      allocate+clear versus 341.011ms for scratch reuse over 2M 96x64 buffers.
      The retained buffer must be explicitly cleared every frame, while fresh
      zeroed allocation can be much cheaper in this workload. Do not retry this
      shape without measuring against real rotated subtree painting and memory
      bandwidth, or without a way to track dirty coverage inside the temp.
- [x] **Minor inner-loop allocations in the Skia backend.**
      `execute_commands_on_canvas` allocates clip/layer stacks per batch
      (`painter/backend.rs:479-480`); the gradient shader cache key includes
      absolute rect position (`backend.rs:18`), so an animated/moving gradient
      re-creates its shader every frame and can thrash the 64-entry LRU — key
      by size only and translate the canvas, or accept and document. Progress
      2026-07-06: linear-gradient shaders are now local to the gradient box
      and cached by `(from, to, width, height)`, with the canvas translated for
      drawing. Same-sized moving gradients now keep one cached shader while
      preserving sampled top/bottom colors. Release-only benchmark over 5k
      moving-gradient draws measured 13.653ms with position-churned shader
      creation versus 12.524ms with size-key reuse (1.1x faster, one shader
      creation). Rejected experiment 2026-07-12: replacing the per-batch
      `Vec::with_capacity` clip/layer stacks with a hand-rolled inline stack
      regressed isolated stack bookkeeping badly: 31.812ms for the existing
      Vec path versus 350.183ms inline over 8M four-clip/four-layer batches.
      Clip/layer stack allocation remains open; a future attempt should use a
      proven small-vector implementation or reuse scratch storage. Completed
      2026-07-13: `execute_commands_on_canvas` now uses `SmallVec` for the
      common shallow clip/layer stacks, with inline-capacity coverage and a
      release-only bookkeeping benchmark. `cargo check -p mesh-core-render`
      passes; render test binaries still cannot link in this environment
      because `freetype`/`fontconfig` are missing.

Structure:

- [ ] **Every widget is painted by two parallel implementations.** The
      immediate-mode path (`render_tree*`/`render_node_with_filter`,
      `render_input_node`, `render_slider_node`, `render_icon_node`,
      `render_scrollbars`) duplicates the display-list path
      (`render_display_*` twins in `painter/widgets.rs`, `painter/tree.rs`)
      for input, slider, icon, scrollbar, and text painting. Same
      pair-duplication hazard as §O's diagnostics split — behavior drift
      between parent surfaces (display list) and child popups/tooltips
      (immediate mode) is silent. Converge on the display-list path (unblocked
      by the child-surface item above) and delete the immediate-mode twins.
- [ ] Text stack is healthy (layout LRU + glyph atlas + ellipsis cache with
      `Cow` fast path); remaining text work is the cache-pressure visibility
      + locale-sensitive workload items already tracked from
      `TEXT_RENDERING_TODO.md`. No new text findings.

### Q. Interaction & input — 2026-07-04 deep dive

Focused trace of `handle_component_input` (pointer/scroll/keyboard),
hover/tooltip transitions, element actions, and focus/scroll helpers. This
section already absorbed the B/J optimization passes; findings below are what
remains. `file:line` as of this scan.

Performance:

- [x] **Keyboard input reads and JSON-parses settings files from disk on
      every key event.** `current_keyboard_settings()` calls
      `load_shell_settings()` (`input/keyboard.rs:340-344`), which does up to
      two `fs::read_to_string` + JSON parse + merge (`config/src/lib.rs:374-390`)
      — invoked per `KeyPressed`, per `KeyReleased`, and per `Char`
      (`keyboard.rs:41,167,516,531`, `input/mod.rs:343`). Typing in a launcher
      input costs 2–4 file reads + parses per keystroke, blocking the shell
      thread. Cache `KeyboardSettings` on the component (or shell) and
      invalidate through the existing settings hot-reload/inotify path — the
      same infra module settings reloads already use.
      `resolved_surface_shortcuts` (rebuilt per keypress with locale lookups)
      becomes cacheable the same way.
      Progress 2026-07-06: `FrontendSurfaceComponent::current_keyboard_settings`
      now caches the merged `KeyboardSettings` behind an mtime check of both
      the defaults and user settings paths, so unchanged files skip the
      `fs::read_to_string` + JSON parse + merge entirely and only pay two
      `stat()` calls. A release benchmark over 20,000 calls measured 218.6ms
      reloading every call versus 50.1ms mtime-cached (~4.4x faster).
      Completed 2026-07-06: `resolved_surface_shortcuts` now caches resolved
      keybinds by `KeyboardSettings` + active locale, so repeated key events
      reuse declaration/override/localized-trigger resolution. Release
      benchmark over 50,000 calls with 24 actions measured 693.588ms rebuilding
      each call versus 128.437ms cached (5.4x faster).
- [x] **Click press/release still runs ~5–8 separate full-tree walks.**
      Press: `selectable_text_target_key`, `pointer_event_target_key`,
      `find_node_bounds_by_key`, `find_focusable_at`, then per-kind probes
      (`is_slider_key`/`is_option_key`/`is_radio_key`/
      `is_checkable_choice_key`); release: `pointer_event_target_key`,
      `find_click_handler`, `build_click_event` (`input/mod.rs:52-179`).
      Clicks are rare so this is latency (not throughput), but on large trees
      it's the same pattern the motion path already fixed — extend
      `pointer_hit_test` to also return focusable/selectable/kind/handler info
      in its single traversal.
      Progress 2026-07-11: `pointer_event_target_key` already ran
      `find_focusable_at` once internally, and the press handler immediately
      called `find_focusable_at` a second time on the same point to pick the
      focus target — a duplicate full-tree walk every press. Replaced both
      call sites with a fused `pointer_event_target_with_focus` free function
      that returns the click-target key and the focusable key from one walk.
      Added parity tests (focusable-with-handler, click-only fallback,
      no-target) and a release-only benchmark; the local dev-shell run
      measured 3.156ms for the duplicate-walk path versus 1.438ms fused over
      20k presses on a 200x12 grid (2.2x faster). The remaining walks
      (`selectable_text_target_key`, `find_node_bounds_by_key`, per-kind
      probes, release-path `pointer_event_target_key`/`find_click_handler`)
      are still separate; full fusion into `pointer_hit_test` remains open.
      Progress 2026-07-11: `selectable_text_target_key` ran `find_node_path_at`
      once to get the depth-many key path, then called `find_node_by_key`
      separately for *each* key in that path — twice over (once for the
      interactive-ancestor check, once for the selectable-text search) —
      turning every press into an O(depth × tree) walk instead of O(depth).
      Rewrote it to resolve the whole path in one `find_nodes_by_keys` call
      (already used by hover-transition dispatch for the same reason) and
      converted it to a free function alongside the fused press-target lookup.
      Added correctness tests (plain selectable text, interactive-ancestor
      short-circuit, non-selectable text, and old-vs-new parity on a deep
      chain) and a release-only benchmark; the local dev-shell run measured
      169.032ms for the per-key walk versus 28.557ms fused over 2k lookups on
      a 40-deep chain with 200 padding siblings (5.9x faster).
      Progress 2026-07-12: pointer press targeting now uses a fused
      `pointer_press_hit` traversal that returns the press target, focusable
      target, target node, and bounds together. The press handler consumes the
      resolved target for pointer-down bounds, slider detection/change-handler
      checks, option/radio/checkable classification, and initial slider value
      calculation instead of immediately re-walking the tree for each. Added
      parity coverage for focusable targets and non-focusable clickable
      ancestors plus a release-only benchmark; the local dev-shell run measured
      3.844s for the old multi-walk press lookup versus 3.170ms fused over 20k
      clickable-grid presses (1212.6x faster). Remaining release-path
      `pointer_event_target_key`/`find_click_handler`, click-event node
      resolution, and some activation helpers still do separate walks.
      Follow-up 2026-07-12: pointer release capture now short-circuits when the
      pointer is still inside the stored press bounds, avoiding an unnecessary
      release hit-test on ordinary clicks. The click dispatch path also reuses
      stored press bounds and an already-resolved target node for menu/item
      classification, click-event construction, and click/activate handler
      checks. Added release-capture parity tests plus a release-only benchmark;
      the local dev-shell run measured 1.529ms for unconditional release
      hit-testing versus 168.599us with the bounds short-circuit over 20k
      releases (9.1x faster). Remaining: release outside press bounds still
      falls back to `pointer_event_target_key`, and some activation helpers
      still do their own ancestry/descendant walks.
      Follow-up 2026-07-12: release fallback outside the stored press bounds
      now uses the fused `pointer_press_hit` path instead of the legacy
      `pointer_event_target_with_focus` multi-walk. Added a release-only
      non-focusable clickable-grid benchmark; the local dev-shell run measured
      2.238s for the legacy multi-walk fallback versus 2.884ms with the fused
      press hit over 20k releases (775.9x faster). Remaining activation helper
      ancestry/descendant walks are still separate.
      Follow-up 2026-07-12: press targets now snapshot their release-dispatch
      metadata (bounds, tag/current-target payload, activation classification,
      click/activate handlers and pre-bound args) so ordinary same-bounds
      release dispatch avoids re-walking the tree for node lookup, bounds
      lookup, classification, and handler resolution. Added a release-only
      metadata benchmark; the local dev-shell run measured 2.503s for the old
      tree lookup/reconstruction path versus 18.997us from the press snapshot
      over 20k worst-case releases (131766.2x faster for that isolated
      metadata path). Remaining activation helper ancestry/descendant walks
      are still separate.
      Follow-up 2026-07-12: checkable switch/checkbox press activation now
      reads `checked` state from the already-resolved press target node instead
      of re-walking the tree by key inside `toggle_checked_value`. Added a
      release-only benchmark; the local dev-shell run measured 4.989s for the
      key lookup path versus 1.076ms with the resolved node over 200k
      worst-case toggles (4637.5x faster for that isolated state lookup).
      Remaining option/radio activation ancestry/descendant walks are still
      separate.
      Follow-up 2026-07-12: option/radio pointer activation now also consumes
      the already-resolved press target for the initial disabled/value read,
      while preserving the existing ancestor/descendant group behavior. Added
      a release-only benchmark; the local dev-shell run measured 5.068s for
      the key lookup path versus 3.942ms with the resolved node over 200k
      worst-case activations (1285.6x faster for that isolated target-state
      lookup). Broader option/radio group indexing remains a future structural
      optimization rather than part of the original press/release multi-walk
      hot path.
- [x] **Scroll events do two extra walks** — `find_scrollable_at` then
      `find_node_by_key` for limits (`input/mod.rs:307-309`); fold the
      scrollable ancestor + limits into the fused hit-test result.
      Progress 2026-07-10: added `find_scrollable_at_with_limits`, preserving
      the legacy key-only API while letting wheel input consume the scrollable
      key and max offsets from the same traversal. Added parity coverage and a
      release-only benchmark; the local dev-shell run measured 1.578s for
      key-then-lookup versus 311.309ms fused over 200k scroll hits (5.1x
      faster).
      Follow-up 2026-07-12: scroll-handler dispatch now uses a fused
      `pointer_event_handler_hit(..., "scroll")` traversal that returns the
      nearest handler node and bounds directly. This removes the path
      allocation, per-key `find_event_handler` tree walks, duplicate handler
      probe, node lookup, and bounds lookup before calling `onscroll`. Added
      parity coverage and a release-only benchmark; the local dev-shell run
      measured 2.610s for the old path/key-walk dispatch lookup versus 1.600ms
      fused over 20k scroll-handler hits (1630.5x faster).
- [x] Minor: `apply_element_actions` cloned the whole `ref_node_keys`
      HashMap per action batch (`interaction_state.rs:91`). Progress
      2026-07-10: action application now temporarily moves the ref lookup map
      out of the `RefCell` and restores it after resolving the drained batch,
      avoiding clone-per-batch while preserving the same resolver semantics.
      Added ref-table preservation coverage and a release-only benchmark; the
      local dev-shell run measured 1.837s for cloning a 512-entry map versus
      1.494ms for move/restore over 100k batches (~1230x faster).
- [x] Minor: hover-change path cloned `Vec<String>` paths
      (`input/mod.rs:214-240`). Progress 2026-07-10: pointer move now takes
      ownership of the path produced by `pointer_hit_test`, swaps it into
      component hover state, and pointer-leave takes the stored path instead
      of clone-then-clear.
      Added a release-only benchmark; the local dev-shell run measured 1.600s
      for the old clone shuffle versus 1.042s for move/replace over 500k path
      updates (1.5x faster).
      Follow-up 2026-07-12: hover transition dispatch now borrows the incoming
      `new_path` before moving it into component state, removing the remaining
      `current_path = self.hovered_path.clone()` snapshot while preserving
      state restoration before handler errors propagate. Updated the
      release-only benchmark; the local dev-shell run measured 1.574s for the
      old clone-shuffle path versus 495.624ms for move/replace without the
      dispatch snapshot over 500k path updates (3.2x faster).
- [x] Confirmation for the tracked slider-drag item (J): the unconditional
      `invalidate_script_state()` per coalesced drag motion is at
      `input/mod.rs:193-200` with a comment explaining why state-dirty
      detection was insufficient — the fix needs slider knob position painted
      from shell-owned `slider_values` via the STATE path plus a paint-only
      text update, exactly as the J item describes.
      Confirmed complete 2026-07-12: handlerless slider press/move paths now
      use interaction restyle, while sliders with change/release handlers keep
      script invalidation for reactive labels. Existing policy coverage asserts
      both paths, and the release-only benchmark reports 790.919ms forced
      script rebuild versus 213.822ms retained interaction repaint over 200
      handlerless drag frames (3.7x faster).

Structure:

- [ ] Interaction identity is string-keyed end to end (`hovered_path:
      Vec<String>`, `focused_key`, `scroll_offsets`, `input_values`,
      `slider_values` all keyed by `_mesh_key` strings). This is the consumer
      side that keeps the §N "identity travels as a string attribute" problem
      alive; the NodeId migration should convert these maps together with the
      metrics/refs publication so the string keys can finally disappear.
- [ ] Otherwise healthy: pointer-motion is fused single-traversal with
      coalescing, hover dispatch resolves all transitioning nodes in one walk,
      scroll animations early-out when idle, stale-target pruning is
      probe-based. No further structural findings.

### R. Script runtime & Lua boundary — 2026-07-04 deep dive

Focused trace of `call_handler` → `sync_state_from_lua` → `ScriptState` →
`refresh_module_object`, plus the VM pool and backend runtime. The G-item
optimizations (write-log discovery, side-channel flag, cached self table,
proxy seen-cache) are confirmed in place; findings below are what remains.
`file:line` as of this scan.

Performance:

- [x] **`refresh_module_object` re-serializes the entire component state per
      handler call for every proxy-bearing component.** Any component that
      `require`s a service interface registers state proxies, so
      `has_proxies()` is true and the generation skip never applies
      (`context/runtime.rs:1777-1793`). Every handler and render hook then
      pays: `state.snapshot()` with proxies — which **bypasses the snapshot
      cache and deep-clones every variable's JSON** plus invokes every proxy
      getter (`context/state.rs:222-231`) — followed by a full JSON→Lua
      conversion and a `module.state` table write. And per
      `docs/modules/frontend/core/README.md:64`, `module.state` is a legacy
      v1.12 compatibility lane; no shipped module reads it. Verify no internal
      consumer remains, then delete the refresh (and the lane) per the
      no-backward-compat rule — likely the single largest remaining boundary
      cost for service-connected components.
      Completed 2026-07-09: verified no shipped module consumes
      `module.state`/`module.exports`, removed both legacy tables and all
      refresh/export synchronization, and retained `module.events` as the
      supported named-event API. Host-seeded values remain direct component
      globals. A proxy-bearing 65-field release benchmark over 20k legacy
      mirrors measured 1.476s for snapshot + JSON→Lua serialization versus
      9.497µs of remaining generation bookkeeping (~155k× difference).
- [ ] **The sync "fast path" still round-trips every known global per
      handler.** For each user global: env read + `from_value` Lua→JSON
      conversion + `state.set` deep-compare, changed or not
      (`context/runtime.rs:1678-1687`). The write log fixed discovery only.
      Because Luau `__newindex` does not fire for existing table keys, a true
      per-write log needs `_ENV` to become a forwarding proxy (empty table
      with `__index`/`__newindex` to a backing store) — or invert ownership:
      keep values in Rust and expose globals through the proxy so there is no
      sync at all. Measure script read-through cost first; pairs with the
      v1.17 per-thread-VM work.
      Progress 2026-07-09: known scalar globals now compare borrowed Lua
      bool/integer/number/string values directly against current JSON state and
      skip `from_value` plus `ScriptState::set` when unchanged; tables and
      changed values retain the conservative conversion path. With 512
      unchanged numeric globals over 5k handler syncs, release time fell from
      468.867ms to 410.951ms (~1.14x faster). Avoiding the `_ENV` read itself
      still requires the forwarding-proxy architecture described above.
- [x] **`ScriptState::snapshot()` with proxies has no caching.** The
      non-proxy branch caches by generation; the proxy branch rebuilds and
      deep-clones everything on every call (`state.rs:196-231`). Even after
      the `module.state` deletion, remaining `snapshot()` callers pay this —
      cache the variables portion by generation and overlay proxy getters.
      Completed 2026-07-09: `snapshot()` now always obtains the local-variable
      object through the generation cache, then overlays fresh proxy getter
      values for proxy-bearing states. Added regression coverage proving proxy
      values remain live while local variables are preserved. A release
      benchmark over 20k proxy-bearing snapshots with 128 local values measured
      455.840ms rebuilding locals versus 385.254ms from the cached variable
      snapshot (~1.2x faster).
- [x] Minor: `sync_module_exports_from_lua` runs per sync (module table read
      + `from_value` + `set`) even for components that export nothing
      (`runtime.rs:1765-1775`); record "has exports" once at script load and
      skip. Removed with the legacy `module.exports` lane on 2026-07-09.

Structure:

- [x] **Legacy `module.state` / `module.exports` lanes.** Documented as
      compatibility-only (`docs/modules/frontend/core/README.md`), but they
      still drive per-handler work (items above). Audit consumers and remove
      per the no-backward-compat rule; if `module.exports` is still the
      mechanism behind component exports, rename/keep that half explicitly
      and document it as current, not compat.
- [ ] Healthy: `LuaVmPool` sandboxing with baseline-global capture, cached
      lifecycle self table, flag-gated side channels, storage read tracking,
      interface-proxy seen-field cache, backend snapshot only on emit paths.
      No further findings.

### S. Events, services & backends — 2026-07-04 deep dive

Focused trace of `broadcast_service_event` → dedup/validation → delivery,
the `InterfaceRegistry`, and the backend service loop. `file:line` as of this
scan.

Performance:

- [x] **`InterfaceRegistry::resolve` deep-clones the entire interface catalog
      on every call.** `resolve()` goes through `catalog()`, which clones the
      full contracts map **and** providers map (every contract's state fields,
      events, and commands for every interface)
      (`extension/service/src/interface.rs:54-56,86-91`), then clones the
      matched contract again (`interface.rs:126-133`). It is called per
      accepted service state update (`validate_service_state_shape`,
      `shell/runtime/service_state.rs:228`), per named interface event
      (`service_state.rs:243`), and per service command dispatch
      (`shell/runtime/request.rs:774,814`). Every audio update and every
      volume command deep-clones every registered contract. Fix: resolve
      directly under the read lock and return `Arc<InterfaceContract>`;
      keep `catalog()` for the debug/discovery paths that genuinely want a
      snapshot.
      Completed 2026-07-05: registry contracts are stored behind `Arc`, and
      `resolve()` now looks up contracts/providers directly under read locks
      instead of materializing a catalog snapshot. `has()` and
      `list_interfaces()` likewise query registry maps directly. Explicit
      `catalog()` calls retain owned snapshot semantics for diagnostics and
      script setup. A release benchmark with 64 contracts (32 methods each)
      over 10k resolutions measured 879.6ms for snapshot resolution versus
      1.20ms for direct registry resolution (~730x faster).
- [ ] **Contract validation re-derives typed information per event.**
      `json_value_matches_contract_type` allocates a lowercased String per
      field per update (`service_state.rs:401-415`), and named-event payloads
      re-parse the inline schema **string** on every event
      (`parse_inline_object_schema`, `service_state.rs:345,375-395` — also
      hand-rolled string parsing, which project policy treats as migration
      debt). Precompile contract field types and event schemas into typed
      enums at contract-registration time; validation becomes match arms with
      zero allocation.
      Progress 2026-07-05: primitive type matching no longer allocates a
      lowercased `String`, and inline event-schema parsing borrows field names
      and types instead of allocating two strings per field. A release
      benchmark over 1M type checks measured 12.56ms allocating versus 5.96ms
      allocation-free (~2.1x faster). Registration-time typed schema
      precompilation remains open. Progress 2026-07-10: shell-side contract
      validation now caches compiled inline event schemas and contract type
      classifications, so steady-state named interface events reuse typed
      field descriptors instead of reparsing the schema string. A release
      benchmark over 300k four-field event validations measured 45.182ms
      parse-per-event versus 15.958ms cached (~2.8x faster). Moving the
      compiled descriptors onto the registered contract remains open.
- [ ] Minor: `canonical_interface_name` / `service_name_from_interface`
      allocate fresh Strings 2–3× per event across normalize/record/profiling
      (`service_state.rs:44,92`, `interface.rs:95-118`); thread the canonical
      name through instead of re-deriving, or intern interface names (v1.23).
      Progress 2026-07-09: canonical names now use a `Cow<str>` helper that
      borrows already-qualified interfaces, normalized service events carry
      the canonical name forward into record/profiling, and allocation occurs
      only when a short alias needs the `mesh.` prefix or state is inserted.
      Over 2M canonical-name calls, release time fell from 20.827ms owned to
      6.446ms borrowed (~3.2x faster). Runtime event observation now also
      borrows the `mesh.`-stripped service segment rather than allocating once
      per component/runtime check; 2M projections measured 1.921ms owned versus
      1.267ms borrowed (~1.5x faster). Progress 2026-07-10: service-event
      normalization now reuses the owned incoming name when it is already
      canonical, avoiding one extra allocation before provider/status checks.
      A release benchmark over 2M canonical runtime names measured 32.137ms
      borrowed-to-owned versus 25.147ms owned-reuse (~1.3x faster). Some
      callers that retain names still use the generic owned API. Progress
      2026-07-10: per-interface service capability caches now include the
      control capability, and service-command dispatch borrows the cached
      capability while reusing one canonical interface name through support,
      optimistic-state, provider, and debug-call bookkeeping. A release
      benchmark over 1M control-capability lookups measured 27.754ms formatting
      versus 23.994ms cached clone (~1.2x faster). Remaining work is broader
      API cleanup for retained interface names outside the command path.
      Progress 2026-07-10: service update delivery now threads the cached short
      service name into state writes for live delivery, locale seeding, and
      cached replay into new runtimes, avoiding another canonical-interface to
      service-name projection per receiving runtime. A release benchmark over
      200k state writes measured 66.567ms projected-name versus 61.055ms
      borrowed-name (~1.1x faster). Progress 2026-07-12:
      `apply_service_update` now also uses the borrowed `Cow` projection for
      generic callers instead of allocating an owned projected name before
      immediately passing it by reference. A release benchmark over 200k state
      writes measured 64.151ms owned projection versus 61.845ms borrowed
      projection (~1.04x faster); explicit pre-borrowed-name state writes
      remain slightly faster at 61.472ms.

Structure:

- [ ] Concrete citation for the tracked "eliminate service-specific Rust
      branches" item: the hardcoded `mesh.audio` optimistic-mute merge lives
      in `normalize_service_event` (`service_state.rs:66-75`) and
      `apply_optimistic_audio_muted_state` (`service_state.rs:137-165`).
      The generic replacement is an optimistic-state declaration in the
      interface contract (field + command linkage) so core stays
      service-agnostic.
- [ ] Healthy/confirmed: shell-boundary payload dedup before delivery,
      wake-level coalescing with barriers, backend-side dedup
      (`publish_changed_update` + `last_payload`), stream line batching per
      program, `Arc<Event>` bus. The open C items (shell-side subscription
      index, push-based host API primitives) remain the section's structural
      backlog.

### T. Layout — 2026-07-04 deep dive

Focused trace of `compute_incremental` → retained style sync → Taffy compute →
text measurement. Confirms the F-item paint-only fast path is in place
(`layout.rs:347-355`). `file:line` as of this scan.

Performance:

- [x] **Unconditional `set_style` per node defeats Taffy's internal caching on
      every layout-dirty frame.** `update_retained_node_styles` converts all
      ~60 style fields (`taffy_style_for_node`) and calls
      `state.tree.set_style` for **every** node whenever layout is dirty
      (`ui/elements/src/layout.rs:811-855`), and `set_style` invalidates that
      node's Taffy layout cache — so one changed node forces Taffy to
      recompute as if everything changed. The retained tree already computes
      per-node STYLE/LAYOUT dirty flags (§N); feeding them here so only dirty
      nodes get converted + `set_style` is the mechanism that makes the
      existing "dirty-node-only sync" item (F) pay off twice: skips the
      conversion walk *and* preserves Taffy's caches for clean subtrees.
      Rejected experiment 2026-07-12: checking `tree.style(id) == new_style`
      before `set_style` without per-node dirty bits measured slower in an
      optimized direct Taffy microbenchmark (46.115ms unconditional
      `set_style` versus 51.642ms skip-equal over 2k×512 equal-style updates),
      so this needs the retained dirty-bit feed rather than an equality guard.
      Completed 2026-07-12: `RetainedWidgetTree` now derives a
      layout-relevant dirty `NodeId` set from the finalized tree before the
      retained-tree update, falling back to the full path on structural
      uncertainty. `LayoutEngine::compute_incremental_with_dirty_nodes` uses
      that set to call `set_style`/`mark_dirty` only for dirty nodes while
      still refreshing text measurement context and output layout maps for
      all retained nodes. Added retained-layout parity coverage for the
      narrow dirty-node path. Verification: `cargo test -p mesh-core-elements
      retained_layout` passes; shell crate verification is still blocked by
      the missing system `xkbcommon.pc` dependency from
      `smithay-client-toolkit`.
- [x] **Text measurement clones the content String twice per node per pass.**
      `update_text_context`/`build_taffy_tree` clone every text node's
      `content` into `TextMeasureData` per layout-dirty and structural pass
      (`layout.rs:857-884,580-596`), and `TextMeasureKey::new` clones it
      **again** per measure probe — including on cache hits, since the owned
      key is built just to probe the LRU (`layout.rs:119-130`). Fix: share
      content as `Arc<str>` (the §N `Arc<str>` payload item's layout face) and
      probe the intrinsic cache with a borrowed/hashed key instead of an owned
      one.
      Completed 2026-07-09: `TextMeasureData` and `TextMeasureKey` now share
      content through `Arc<str>`, leaving one node-attribute-to-measurement
      allocation per layout sync and making cache-key construction a pointer
      clone. A 376-byte string benchmark over 1M iterations measured 28.481ms
      for two `String` clones versus 9.820ms for `Arc` build+clone (~2.9x
      faster). Text content-change measurement regression coverage passes.
- [x] **Structural reconcile is string-keyed and clone-heavy.**
      `reconcile_retained_taffy_node` clones each node's `_mesh_key` String
      (`layout.rs:773-810`), `collect_mesh_keys` clones every key into a
      `HashSet<String>` per structural pass (`layout.rs:901-908`), and the
      stale sweep clones + length-sorts keys (`layout.rs:706-722`).
      Progress 2026-07-09: existing retained nodes now borrow `_mesh_key`
      during lookup and clone only on insertion; the live-key set borrows from
      the widget tree, so only genuinely stale keys are cloned for ordered
      removal. Add/remove/reorder parity tests pass. A 1,024-key release
      benchmark over 5k collections measured 219.204ms cloned versus 70.786ms
      borrowed (~3.1x faster). Completed 2026-07-09: retained Taffy identity is
      now keyed by stable `NodeId`; structural stale removal finds stale roots
      through Taffy's parent links and removes each subtree once. Five retained
      add/remove/reorder/style/layout parity cases pass. A 5M-lookup release
      benchmark measured 86.858ms for long String keys versus 47.315ms for
      `NodeId` (~1.8x faster).

Structure:

- [x] **The LAYOUT-03 string-keying rationale is obsolete.**
      `PerSurfaceLayoutState.node_map` is keyed by `_mesh_key` String with a
      comment "NOT ephemeral NodeId per LAYOUT-03" (`layout.rs:144-146`) — but
      runtime NodeIds are no longer ephemeral: they are stable hash-chained
      ids derived from the same key paths (§J progress, `runtime_tree.rs`).
      Re-keying `node_map` by `NodeId` removes every string clone above and
      the per-node hash of long key strings in `retained_taffy_id`
      (`layout.rs:910-915`). Do together with the §Q interaction-map NodeId
      migration so `_mesh_key` strings have no remaining hot consumers.
      Completed for retained layout on 2026-07-09; interaction/refs maps remain
      separate follow-up consumers of `_mesh_key`.
- [x] Healthy/confirmed: paint-only frames skip all layout sync; fresh
      `node_map`/`text_nodes` maps per pass were measured (scratch reuse
      rejected 2026-07-04); intrinsic text cache is LRU-bounded; Taffy
      diagnostics are report-gated. Confirmed 2026-07-12 while closing the
      retained dirty-node style-sync path.

### U. Presentation & memory — 2026-07-04 deep dive

Focused trace of `present_with_damage` → SHM pool copy → `attach_shm_buffer` →
commit, plus popup promotion, scale/blur/input-region protocol handling, and
input normalization in `crates/core/presentation`. Existing H items (direct
Skia-into-SHM paint, size-class pools, `copy_bgra_to_canvas` cites) still
stand; findings below are additional. `file:line` as of this scan.

Performance:

- [x] **Per-buffer pending damage is a single bounding rect, which forces the
      SHM copy to be a union.** `SurfaceShmBuffer.pending_damage` is
      `Option<DamageRect>` (`presentation/src/wayland_surface/backend.rs:73-76`),
      accumulated via `union_damage` (`backend.rs:270-283`), and
      `present_with_damage` folds the frame's multi-rect damage into one union
      before the copy (`backend.rs:1174-1183`, the "Pitfall 1" comment). Two
      small disjoint changes on one surface — clock text at the left of a bar
      plus a volume icon at the right — memcpy the entire span between them
      every frame, even though the `damage_buffer` protocol calls downstream
      are correctly per-rect. Making `pending_damage` a small bounded rect list
      (same 16-rect cap as `MAX_PROTOCOL_DAMAGE_RECTS`) lets the copy loop run
      per rect and shrinks steady-state SHM traffic to the actual changed
      pixels. Pairs with the H direct-paint item; whichever lands first should
      carry the rect-list change.
      Completed 2026-07-05: each SHM buffer now retains an inline bounded list
      of up to 16 physical damage rects and copies each region independently;
      overflow collapses safely to one union. Protocol damage includes every
      region actually refreshed on a reused buffer. A two-edge-rect benchmark
      over 1,000 1920x100 frames measured 16.25ms for bounding-union copies
      versus 3.47ms for rect-list copies (~4.7x faster in the debug profile).
- [x] **kde_blur region is re-created and re-committed on every present while
      blur is active.** `present_with_damage` unconditionally creates a fresh
      `Region`, calls `set_region` + `commit` per frame whenever
      `entry.blur_region` is `Some` (`backend.rs:1192-1215`) — the shell-side
      gate (`last_region_state` in `runtime/render.rs:900-930`) only gates
      `update_blur_region`, not the per-present protocol churn, because the
      backend re-commits from stored state each frame. A surface with an
      animated element and a static backdrop-blur pays wl_region create +
      2 protocol requests per frame for a region that never changes. Track the
      last-committed rect on `SurfaceEntry` and skip when unchanged (the
      `input_region_dirty` pattern two blocks below it is the right shape —
      blur is the one region type that didn't get it).
      Completed 2026-07-05: `SurfaceEntry` now tracks blur-region dirtiness;
      unchanged presents skip region creation and KDE blur protocol commits,
      while region changes and removal still commit exactly once. Regression
      coverage proves 1,000 unchanged presents require one blur update.
- [ ] **Input normalization allocates a String per event via a linear surface
      scan.** Every pointer/keyboard event calls `surface_id_for_wl_surface`,
      which iterates all surfaces comparing `wl_surface` handles and clones the
      id String (`wayland_surface/state.rs:314-322`, called from
      `handlers.rs:217` per pointer-frame event, `handlers.rs:438,461` for
      keyboard focus). Motion events then carry that String into
      `DevWindowEvent`, the shell re-allocates it again in `dispatch_wayland`
      (`event_surface_id(&event).to_string()`, `runtime/wayland.rs:24`), and
      key repeat clones surface-id + key name per synthesized event
      (`state.rs:19-29`). Coalescing caps what reaches the shell but every raw
      Wayland event still pays the scan + clone. Store the surface id as
      `Arc<str>` on `SurfaceEntry` (or a numeric id + side table) so the lookup
      is a pointer clone; `keysym_name`/`normalize_keysym_name` String
      allocation per key event (`handlers.rs:561-585`) and the lowercase alloc
      in `is_non_repeating_key` (`state.rs:336-348`) fold into the same pass.
      Progress 2026-07-08: Wayland state now maintains a `wl_surface`
      `ObjectId` → surface id index alongside the owned surface map, so raw
      pointer, keyboard-focus, layer-close, and popup-dismissal events avoid
      linearly scanning all surfaces before routing. Public `DevWindowEvent`
      payloads remain `String`-based, so the remaining clone/API cleanup and
      key-name allocation pass are still open. A release-only benchmark over
      500k lookups across 128 surfaces measured 13.141ms for the scan path
      versus 0.715ms indexed (18.4x faster).
      Progress 2026-07-08: key-repeat filtering no longer lowercases each
      key name to detect modifiers/non-repeat keys; it now uses borrowed ASCII
      case-insensitive equality/window checks. Release benchmark over 1M
      eight-key batches measured 323.835ms for lowercase allocation versus
      61.248ms borrowed (5.3x faster). `keysym_name` allocation and public
      event payload `String` clones remain open. Progress 2026-07-12:
      key press repeat setup now builds repeat state from borrowed
      `surface_id`/key-name inputs and clones only when a repeat state is
      actually retained; non-repeating key events move the owned event
      surface/key strings directly into `DevWindowEvent` instead of cloning
      them and dropping the originals. Added repeat-state coverage and a
      release-only benchmark; the local dev-shell run measured 16.578ms for
      the old clone-before-schedule shape versus 7.198ms borrowed over 500k
      non-repeating key presses (2.3x faster for that isolated repeat setup
      path). `keysym_name` allocation and public event payload `String` clones
      remain open. Progress 2026-07-13: Wayland `keysym_name` now returns a
      `Cow<'static, str>` and `normalize_keysym_name` borrows common xkb names
      through repeat filtering/release matching, only owning the key name at
      the public `DevWindowEvent` boundary or for raw numeric fallbacks.
      Focused presentation tests remain blocked in this environment by the
      missing `xkbcommon.pc` system dependency; source check confirms
      `xkeysym::Keysym::name()` returns `Option<&'static str>`. The remaining
      cleanup is the public `String` event payload shape.
- [ ] **Child popup targets force a full-buffer present every frame.**
      `paint_and_present_child_surface` sets `force_full_present = true`
      unconditionally after each child paint (`shell/runtime/render.rs:789-791`),
      so even if the popover subtree gained retained damage tracking (§P child
      item), presentation would still upload the full buffer. This is the
      presentation-side half of the §P "child popups bypass the retained
      pipeline" item — fix them together, otherwise the display-list work
      shows no SHM win.
      Progress 2026-07-08: child popup targets now force full present only for
      first/resized buffers or backend scale/full-redraw requests. Steady child
      frames carry child-local damage translated from the parent component's
      retained present damage, and the test backend records damage payloads for
      regression coverage. The focused child-popup test path reduced a steady
      child present from full `72x32` damage (2,304 logical px) to `12x7`
      damage (84 logical px), a 27.4x smaller present/upload region. The
      deeper child-subtree retained-paint path remains open: child buffers are
      still repainted eagerly before presenting sparse damage.
- [x] **`wait_for_surface_configure` runs up to 10 blocking roundtrips on the
      shell thread.** Called from `present_with_damage` (`backend.rs:1130`)
      and `surface_size` (`backend.rs:1324`) whenever the surface is not yet
      configured (`backend.rs:1405-1432`). Fine for first map, but a
      compositor that delays configure (or a dead popup) stalls the whole
      frame loop for 10 round trips; every other surface's present waits
      behind it. Bound it by deadline instead of roundtrip count, or return
      not-ready and let the render loop retry on the next Wayland wake (the
      loop already wakes on the connection fd).
      Progress 2026-07-08: configure waiting is now deadline-bounded at 2ms.
      The backend flushes and dispatches pending events, reads Wayland events
      only while the connection is ready within the remaining budget, and then
      returns to the render loop if the surface is still unconfigured. This
      removes the previous worst case of 10 blocking roundtrips on the shell
      thread; delayed/dead configure now costs at most the 2ms local budget per
      attempt before other surfaces and IPC work can continue. Completed
      2026-07-13 verification: the shipped backend is deadline-bounded by
      `SURFACE_CONFIGURE_WAIT_DEADLINE` and polls the Wayland fd until either
      the surface configures or the deadline expires. The deadline is now
      500ms, not 2ms, because the shorter budget regressed first-configure
      sizing on startup; the old fixed 10-roundtrip loop is gone.
- [x] Minor per-present allocations: `attach_shm_buffer` builds two
      `Vec<DamageRect>` per present (`backend.rs:334-343`) and
      `protocol_damage_rects` re-allocates via `to_vec` even in the ≤16
      passthrough case (`backend.rs:569-582`) — smallvec/iterate-in-place;
      `surface_config_fingerprint` is a fourth hand-rolled byte-at-a-time FNV
      hasher (`backend.rs:142-161`), the presentation face of the §N
      hasher-drift item.
      Progress 2026-07-05: physical scaling and clipping are fused into one
      vector build, and the common <=16-rect protocol path borrows the damage
      slice instead of allocating with `to_vec`. A debug microbenchmark over
      1M four-rect passthroughs measured 36.09ms cloned versus 8.97ms borrowed
      (~4.0x faster); the release run was blocked by a full build volume.
      Progress 2026-07-08: the fused clipped-damage scratch in
      `attach_shm_buffer` now uses inline `SmallVec` storage, keeping the
      common <=16 rect path allocation-free through scaling, clipping, copied
      damage extension, and protocol-damage borrowing. Release benchmark over
      1M six-rect clipped scratch builds measured 45.497ms for heap `Vec`
      versus 16.218ms for `SmallVec` (2.8x faster).
      Progress 2026-07-08: `surface_config_fingerprint` now overrides
      primitive `Hasher::write_*` methods instead of falling back to
      byte-at-a-time hashing for every numeric config field. Release benchmark
      over 2M config fingerprints measured 27.644ms for byte writes versus
      5.309ms for primitive writes (5.2x faster).

Structure:

- [x] Healthy/confirmed: SHM pool reuse with per-buffer pending-damage
      refresh and the busy-buffer overflow slot (`backend.rs:265-316`);
      surface config fingerprint gating with the keyboard-only reconfigure
      carve-out (`backend.rs:198-227,454-469`); popup reconcile gated on
      `PopupConfig` equality shell-side (`runtime/render.rs:629-649`); opaque/
      input/blur region *updates* gated by display-list generation shell-side
      (`runtime/render.rs:900-930`); input region applied lazily with a dirty
      flag so it survives configure/remap ordering (`backend.rs:1220-1239`);
      frame-callback wait treated as a hint with a 50 ms escape hatch
      (`backend.rs:63,401-406,1132-1146`); `wait_for_events` blocks on Wayland
      fd + shell eventfd together with no spin (`backend.rs:1510-1602`);
      pointer/scroll coalescing at the engine boundary
      (`presentation/src/lib.rs:427-481`). The dev-window backend is dev-only
      and was not audited for hot-path cost. Confirmed 2026-07-13 against the
      current presentation backend; existing regression coverage includes
      disjoint pending-damage preservation/collapse, keyboard-only configure
      retention, and unchanged blur-region dirtiness.

### V. Shell orchestrator, threading & startup — 2026-07-04 deep dive

Focused trace of `Shell::run` (event loop, wake scheduling, message
coalescing), `render_components` orchestration, `dispatch_wayland`, reload
gating, and the discovery → catalog → mount startup path. The K threading
items (parallel paint, pipelining, blocking IO off-thread) and H startup item
(parallel module compile) still stand; findings below are additional.
`file:line` as of this scan.

Performance:

- [x] **Every top-level surface gets a deep clone of the entire compiled
      frontend catalog at startup.** `FrontendCatalog` is a plain `Clone`
      struct holding every `CompiledFrontendModule` (parsed templates, styles,
      scripts for *all* frontend modules; `shell/component/catalog.rs:11-21`),
      and `load_frontend_components` passes `frontend_catalog.clone()` to
      each `FrontendSurfaceComponent` (`shell/discovery.rs:366-377`) after
      `top_level_surfaces()` has already cloned every matching entry once more
      (`catalog.rs:148-163`). With N surfaces the shell holds N+1 full copies
      of every compiled module for the life of the process — startup time and
      resident memory both scale as catalog × surfaces. Wrap the catalog in
      `Arc<FrontendCatalog>` (it is read-only after build; hot reload can
      rebuild-and-swap the Arc). Same call site also hands each component its
      own deep `interfaces.catalog()` clone (`discovery.rs:375`,
      `extension/service/src/interface.rs:86-91`) — the startup face of the
      §S resolve-clone item; fix both with the same `Arc` treatment.
      Progress 2026-07-05: the compiled `FrontendCatalog` is now shared by all
      top-level surfaces through `Arc`; source reload uses copy-on-write via
      `Arc::make_mut`. This removes the persistent N+1 deep copies of every
      compiled frontend module. Completed later 2026-07-05: the interface
      catalog is also constructed once and shared through every surface,
      embedded `ScriptContext`, and Lua lookup closure. A release benchmark
      with 64 contracts (32 methods each) over 10k clones measured 799.7ms for
      deep catalog clones versus 27.3us for `Arc` clones (~29,000x faster).
- [x] **`interfaces.resolve()` catalog deep-clones also fire from the command
      dispatch path.** `service_command_is_supported` and
      `service_command_is_coalescable` each call `self.interfaces.resolve()`
      (`runtime/request.rs:773-779,813-820`) — two full catalog clones per
      `ServiceCommand` request (every slider drag tick that passes the
      throttle, every button command), and `flush_throttled_commands` resolves
      again per flushed command. Already covered mechanically by the §S
      "resolve under the read lock" item; listed here so the orchestrator-path
      call sites get retired with it.
      Completed 2026-07-05 by the direct `InterfaceRegistry::resolve` lookup;
      command support/coalescing checks now clone only the selected contract
      `Arc` and provider record.
- [ ] **Startup is fully serial on the main thread.** Confirmed the H item:
      `discover_modules` scans + parses manifests dir-by-dir
      (`discovery.rs:124-136,209-304`), then `FrontendCatalog::from_modules`
      compiles every frontend module one at a time (`catalog.rs:45-69`),
      then backends spawn. Manifest load, `.mesh` parse, and compile are pure
      per-module — rayon over `module_ids` in `from_modules` is the smallest
      first cut. (Graph load is cached via
      `load_installed_module_graph_cached`; its `clone()` uses at
      `discovery.rs:365` and `backend/spawn.rs:18` are startup-only and fine.)
      Progress 2026-07-05: frontend compilation now runs concurrently through
      Rayon and measured ~1.9x faster on the shipped catalog. Discovery and
      manifest loading remain serial, so the broader startup item stays open.
- [x] **Per-event allocations in `dispatch_wayland`.** Each dispatched event
      allocates the physical surface id String (`runtime/wayland.rs:24`),
      clones the routed target id (`wayland.rs:53`), calls
      `surface_size_changed` per event even when the size cannot have changed
      (`wayland.rs:180`), and wraps every emitted request in its own
      single-element `VecDeque` (`wayland.rs:216-219`). Bounded by the
      32-events-per-frame cap and input coalescing, so this is allocation
      hygiene, not a hot bug — retire together with the §U `Arc<str>`
      surface-id change so ids stop being re-allocated at each layer. Progress
      2026-07-06: `dispatch_wayland` now borrows the physical event surface id
      instead of allocating it per event, clones keyboard focus only for
      keyboard routing, and drains emitted requests as one `VecDeque` instead
      of wrapping each request separately. Target-surface clone and redundant
      size-change calls remain open. Progress 2026-07-10: added a
      single-request drain helper and used it for Wayland global shortcuts and
      popup pointer-leave hides, removing the remaining single-element
      `VecDeque::from([request])` allocations in `dispatch_wayland`. Progress
      2026-07-10: stable-size input events now compare against the component's
      current content input size before calling `surface_size_changed`, so the
      redundant per-event size-change call is skipped unless the routed content
      size actually changed. Rejected experiment 2026-07-10: replacing the
      emitted-request `VecDeque::from(Vec<CoreRequest>)` adapter with direct
      `Vec::drain` looked plausible but measured slower: 74.102ms for the
      `VecDeque` adapter versus 82.505ms for direct vector drain over 1M
      four-request batches (0.9x), so the prototype was reverted. Follow-up
      2026-07-12: `dispatch_wayland` now splits incoming `WindowEvent`s by
      value into `(surface_id, payload)` and routes by borrowed `&str`, so
      the event-owned surface id is reused instead of cloning the routed
      target id for every event. Pointer-leave hide requests still allocate
      only when they need an owned `CoreRequest` surface id. Added payload
      preservation coverage and a release-only benchmark; the local dev-shell
      run measured 12.274ms for the old target-id clone path versus 7.151ms
      for split-by-value over 500k events (1.7x faster for the isolated
      dispatch id step). The listed `dispatch_wayland` allocation sources are
      now addressed; broader public `WindowEvent` surface-id API cleanup
      remains tracked under input normalization.
- [x] Minor idle-loop hygiene in `render_components`: the surface id String
      is cloned for every component before the `wants_render` gate
      (`runtime/render.rs:23`), and `component.id().to_string()` runs per
      rendering component per frame (`render.rs:66`); `reconcile_child_surface
      _requests` rebuilds `requested_keys`/`closing_keys` HashSets and
      re-clones entering-key sets per frame while any popover is open
      (`render.rs:432-499,524-527,669-673`). All small; fold into the v1.23
      interning pass. Progress 2026-07-06: `render_components` now checks
      `wants_render()` before cloning the parent surface id, so idle components
      do not allocate a surface-id `String` on each render pass. The remaining
      per-rendering-component id clone and popover reconciliation sets remain
      open. Progress 2026-07-10: rendering components now reuse the already
      cloned parent `surface_id` as the component/profiling id instead of
      calling `component.id().to_string()` again. A release-only benchmark over
      2M id reads measured 15.359ms for the extra clone versus 1.368ms for the
      borrowed surface id path (11.2x faster). Popover reconciliation set
      churn remains open. Progress 2026-07-10: child-surface reconciliation now
      stores requested child keys in a stack-backed `SmallVec` for the usual
      small-popover case instead of allocating a `HashSet<&str>` each frame.
      A release-only benchmark over 500k three-popover reconciliation
      membership checks measured 39.237ms for the `HashSet` path versus
      372.816us for `SmallVec` (105.2x faster). Closing-key owned set
      construction remains open. Follow-up 2026-07-12: `ShellComponent` now
      exposes a borrowed `set_closing_child_keys_from_slice` path, and child
      reconciliation compares stack-backed closing-key slices against the
      component's existing set before allocating owned strings. The local
      release benchmark measured 81.703ms for owned `HashSet<String>`
      construction versus 23.756ms for borrowed `SmallVec` comparison over
      500k steady-state checks (3.4x faster). The listed idle-loop allocation
      sources are now addressed.

Structure:

- [ ] **`legacy_backend_candidates_from_discovery` is a compat lane.** The
      graph-load failure fallback spawns backends from discovery-time module
      scanning (`backend/spawn.rs:48-59`, `backend/candidates.rs:300+`),
      duplicating the graph-driven candidate logic. Per the
      no-backward-compat rule: decide whether a missing/broken
      `config/module.json` should be a hard startup error (matching the
      manifest migration-diagnostics stance) and delete the legacy lane, or
      document why a degraded-mode boot is a product requirement. Currently it
      is a second candidate-selection implementation that can drift.
- [ ] Healthy/confirmed: the event loop is deadline-driven end to end —
      `next_runtime_sleep` computes exact deadlines from reload checks,
      command throttles, closing surfaces, popover hides, and component ticks
      (`runtime/mod.rs:76-151`) and blocks on Wayland fd + eventfd
      (`mod.rs:254-287`); all four reload checks park for 24 h when the
      inotify watcher is active and wake via `FilesystemChanged`
      (`reload.rs`, `theme.rs`, `mod.rs:391-397`); shell messages are drained
      with a 256 cap and coalesced with correct barrier semantics for
      lifecycle/interface-event ordering (`mod.rs:225-241,425-475`);
      `component_target_for_surface` rebuilds its index lazily on miss only
      (`mod.rs:46-66`); backend event bridges are per-provider Tokio tasks
      that wake the loop via eventfd writes (`backend/spawn.rs:100-241`);
      `flush_wayland` is TRACE-gated (`wayland.rs:225-244`). The remaining
      structural gap is the K phase-split (serial VM phase / parallel paint
      phase), unchanged by this pass.

### Suggested attack order

1. **Pointer-motion + scroll coalescing (J)** — one small diff in
   `dispatch_wayland_events`; divides all per-motion costs by the
   motion-to-frame ratio. Do this first.
2. Fractional-scale partial damage (D, first item) — biggest visible win on
   scaled outputs, bounded scope.
3. Per-node `StyleRuleIndex` rebuild on the build path (E) — turns every
   script-driven rebuild from O(nodes × rules) into O(nodes + rules); tiny
   diff.
4. Per-paint key/hit-test index (B + J) — kills the input-path tree clone,
   the 5-walk hover dispatch, and the per-paint `prune_stale` key sweep with
   one shared structure.
5. `sync_state_from_lua` write log (G) — removes per-handler full-globals
   conversion; helps every interaction.
6. Slider-drag reclassification + narrow-path gating (J) — makes drags cost
   a restyle instead of rebuild+diff overhead.
7. Element-metrics laziness (A) — removes per-paint JSON build/compare/convert.
8. Animation walk gating (F) — free win for the common no-animation surface.
9. Event routing index + payload `Arc` (C) — cheap, unblocks chatty backends.
10. Service-payload dedup + interaction rule-existence gate (K) — two small
    diffs that eliminate steady-state work at poll/hover frequency.
11. Per-surface parallel paint (K) — first threading step; needs the
    phase-split refactor of `render_components` but no new invalidation
    machinery.
12. Component-level render memoization (I) — largest structural win; plan it
    with the v1.18/v1.27 invalidation work since it shares the dependency
    bookkeeping.
13. State snapshot COW + typed expression/declaration values (A/E) — feeds
    the same invalidation work.
14. Paint/script pipelining + tile-parallel raster (K) — after the
    per-surface split proves the phase boundary; pairs naturally with the
    GPU work (v1.25).

---

## 2026-07-13 — Component-level render memoization shipped (section I)

`render_import` (`crates/core/shell/src/shell/component/composition.rs`) now
memoizes each imported/local component instance's built subtree
(`crates/core/shell/src/shell/component/memo.rs`). Entry key/validity:

- props fingerprint (props map + typed `EventHandlerCall`s, structural JSON
  hashing for args),
- the instance's own `ScriptState::mutation_generation` **and** every
  descendant instance's generation (descendants found by hierarchical
  instance-key prefix, so a nested child's state change invalidates every
  enclosing cached subtree),
- active theme `Arc` pointer identity (`refresh_active_theme` swaps the Arc
  only on real theme changes),
- active locale,
- container size the subtree was built against (first build runs pre-layout,
  so entries settle from the second build onward).

Build side effects are made replayable or vetoed via mark counters on
`FrontendSurfaceComponent`: promoted-popover wrappers and error placeholders
inside a cached subtree re-set their per-build presence flags on reuse;
surface-portal visibility writes (`pending_surface_states`) veto caching.
`bind:this` live bindings persist on the shared surface VM, so hits safely
skip re-installation. Tracked service reads accumulate on the `ScriptContext`
and cached nodes carry `service_field_reads`, so service observation and
`NodeServiceFieldDependencies` stay intact across hits. The cache clears in
`reset_render_caches` (theme change, locale change, source reload — the same
sites that clear runtimes).

Coverage: `component_memo.rs` integration tests for unchanged-sibling reuse
(identical pixels), prop-change invalidation with sibling reuse,
descendant-generation invalidation through an enclosing component, and
popover-promotion flag replay on cache hits. Release benchmark
(`memoized_rebuild_beats_full_child_reeval`): 200 full rebuild+paint cycles
of a 12-distinct-child surface measured 212.7ms forced-miss versus 134.5ms
memoized (1.6x end-to-end including the untouched restyle/layout/paint
stages; hits=2400/2400).

Known limits: repeated same-alias instances share one runtime (pre-existing
module-identity limitation), so their alternating prop application bumps the
shared generation and every lookup misses — correct but unaccelerated until
per-occurrence instance identity lands. `render_slot` instances are not yet
memoized. Only public script members are reactive (unchanged contract):
template expressions over private locals were never guaranteed to re-render.
