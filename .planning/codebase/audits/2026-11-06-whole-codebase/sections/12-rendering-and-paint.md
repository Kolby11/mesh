# Section 12 — Rendering and paint

No repository files edited, builds launched, or tests run. Findings are source-grounded; no new timings or speedups claimed. Review spans performance, redundancy, and correctness, but remains partial. Exact file accounting: /tmp/mesh-section12-coverage.json.

## Process tree
Retained widget deltas update display entries/commands, replay spans and damage. CPU sessions draw into retained pixels or defer immutable pictures for raster workers. GPU SceneCanvas records a complete immutable scene, retaining prepared image identities and bounded node pictures; bounded scene handoff coalesces by surface/commit while renderer-owned GPU resources stay on a worker. Resource workers publish async icon/glyph/image results and invalidate requesting consumers. Font/resource/alias revisions govern shape and picture reuse. Presentation owns buffers, protocol, and compositor commit.

## Performance findings

S12-PERF-21: Resource completion globally evicts unrelated scene recordings.
Confidence confirmed source path, unmeasured workload cost. prepared_scene.rs:14–29 holds a process-global resource completion epoch and global pending predicate; prepared_scene/node_cache.rs:61–69 clears a surface's entire picture cache when global epoch/revision differs, :79 checks it for every lookup, :107 rejects all cache insertions while ANY icon/glyph/image job is pending. A resource load on surface B therefore destroys future recording reuse on stable surface A. Safety motivation is real: don't let a cached missing-asset recording hide completed resources. Improvement needs consumer/resource dependencies or immutable resource snapshot identity rather than removing invalidation. Benchmark two surfaces, one 1,000-node stable tree and one consumer loading 1/10/100 assets; record reused/recorded nodes, recording CPU p50/p95, jobs and memory while checking resource readiness/pixels. Existing backlog resource-scoping/renderer invalidation work overlaps. Not measured as cause of user's four seconds.

S12-PERF-22: Per-module font alias switching unnecessarily advances that global epoch.
Confidence confirmed cross-section source path, unmeasured impact. surface/text.rs:1372–1390 interns alias maps but calls prepared_scene::resource_completed whenever active map differs. Shell runtime/render/mod.rs:858 installs aliases before each surface. Alternating module-specific map A/B/A can evict each surface's otherwise valid recordings on every pass, even though each surface resolves the same alias set as on its own previous paint. NodePaintKey carries scale/clip/module but no immutable alias-set identity. Remedy: include stable font alias snapshot identity in relevant recording keys and scope completion invalidation; preserve changed-face freshness and resource generations. Benchmark two stable surfaces with distinct aliases and alternating paints versus identical aliases, asserting reuse and correct glyph face selection. Related to preceding finding; don't blindly delete epoch bump.

S12-PERF-23: External-resource SVGs are rerasterized on subsequent independent paints.
Confidence confirmed async production path, no live measurement. icon.rs:760–764 marks SVGs containing external resource refs noncacheable. Completion drain :641–659 inserts them only into queue.ready; take_ready_variant :670–680 removes the sole ready entry. resolve_file_variant_async :1842–1885 then finds neither ready nor persistent cache on a later independent paint and schedules the same unchanged resource again. This can add decode/process work and completion-driven extra paints while other UI animation/updates continue. CPU repaints exercise resolver directly; GPU node pictures can mask repeat until invalidated. last_painted_raster fallback :1717–1726 usually retains visible pixels while Pending, so repeated blank frames are NOT established. Consuming Ready does not itself schedule another job, so an idle infinite reraster loop is NOT established. Keep bounded last-ready variants keyed by source and linked-resource freshness rather than discarding after one consumer. Production-path test: one SVG linked to unchanged PNG, wait first ready, repaint repeatedly with independent invalidation; count raster jobs and verify linked PNG replacement still refreshes. Unit tests use synchronous cfg(test) paths, so add production integration coverage. Not a known exact backlog item; overlaps async broker/cache work.

S12-PERF-24: Glyph atlas growth copies complete used page regions and globally invalidates pictures.
Confidence confirmed source behavior, already backlog item. glyph_pages.rs:77–98 snapshot_if_stale copies PAGE_SIZE x used_height bytes into new immutable Data whenever glyphs added; image_pages.rs:37–39 advances completion epoch on page publication. Batching limits snapshots but small additions still copy growing page content. Preserve old immutable scene snapshots and shared upload identity when exploring incremental GPU subupdates or persistent immutable page tiling. Measure 1/10/100 new glyphs per frame on near-full 1024 pages, CPU copied bytes, upload bytes and scene-recording reuse. Current backlog already says 'Glyph pages re-upload whole page when they grow'; don't add duplicate. Rejected TextureFromImage alias pinning is not a viable replacement.

## Dead code and redundancy
No newly confirmed dead-code deletion is recommended from this partial continuation. PreparedScene.images may look unread to compiler, but it deliberately pins image lifetimes and must not be removed solely for a dead-code warning. SceneRenderThread initial factory and Drop join synchronously; normal rendering is asynchronous/bounded, so this does not establish an ordinary four-second debounce. A hung native backend could delay teardown, but process/native-crash isolation is not promised and no reproduced hang was found. Device damage fractional conversion/disjoint clipping has substantive coverage and no new defect was confirmed. The existing uncommitted anyrender fixture and presentation changes were inspected as pending work, not newly open findings.

## File coverage
This agent fully read seven additional source files (including embedded tests): device_damage.rs, fractional_scale.rs, surface/canvas.rs, surface/glyph_pages.rs, scene_thread.rs, prepared_scene.rs, display_list/subtree.rs. It partially inspected paint_input.rs, display_list/batch_index.rs, surface/image_pages.rs, surface/resource_broker.rs, prepared_scene/node_cache.rs, surface/icon.rs and surface/text.rs. The predecessor had six fully read files and eight partial files, retained separately in coverage JSON. Most display-list algorithms, painter bodies, optional adapters, GPU backend and tests remain unreviewed or only historical evidence. Do not claim section12 complete. Required specs/philosophy, architecture, crate boundaries and rejected-experiments table were consulted; no performance change was implemented.

## Logic and core mechanics

Resource freshness and placeholder safety are invariants in PERF-21–23. Source-backed inefficiency alone does not justify dropping completion invalidation.

### S12-LOGIC-21 — Shape geometry is absent from the shared paint fingerprint

`paint_input.rs::hash_primitive` hashes input attributes and checkable indicators but has no shape branch. `ShapeGeometry::from_node` consumes `type`, `start`, `end`, `inner` and `points`; `display_list/paint_node.rs` uses that geometry to construct the shape payload. Both `render_object.rs::render_object_fingerprint` and `display_list/build.rs::collect_node_entries` derive paint identity from `PaintInput`. Equal bounds and entry signatures are treated as reused/no damage by `display_list/mod.rs::reconcile_entries`.

A same-size shape geometry change can therefore be invisible to ordinary retained fingerprint/damage discovery. Explicit producer damage can mask this in some update paths, so a live stale-shape reproduction is not claimed. **Confidence: confirmed missing paint inputs; high-confidence correctness risk, no regression run.** The older/test-only `signature.rs::hash_paint_content_attributes` does enumerate the shape fields, so tests exercising that helper do not prove the production contract. Shape fill/stroke colors are already covered by material hashing; they are not an additional missing-color finding.

Test fixed-size sectors/arcs with changed angles/inner radius and polylines/polygons with changed points through retained update and pixel comparison, including an update with no explicit forced damage. Require both render-object dirty classification and display-entry signatures to change. Share one authoritative paint-input policy rather than adding another independent list.

### S12-LOGIC-22 — Preclip bounds use the wrong transform for fixed descendants

`display_list/build.rs::subtree_bounds_reach_clip` passes the ordinary child transform to every descendant. Actual `build_paint_subtree_with_transform` resets fixed children to the root transform and viewport clip; `subtree_bounds_at_with_transform` also has that fixed-position exception. The preclip optimization can therefore reject an offscreen ancestor subtree whose fixed descendant belongs onscreen. **Confidence: source-confirmed transform-policy disagreement; high-confidence missing-paint risk, no pixel reproduction run.**

Construct an offscreen/transformed parent containing an onscreen fixed child. Compare preclip-enabled output and full traversal, then repeat under scroll/overflow clips and nested transforms. Preserve the established fixed-position semantics rather than weakening all preclipping. This is distinct from the existing generic transform-parity audit item and needs its own regression.

## Existing backlog or audit overlap

## November 7 painter and resource-cache continuation

### S12-LOGIC-23 — Persistent render engines retain repeated diagnostics without a bound

`surface/painter.rs:135,296–320` stores diagnostics in a Vec, appends new records and clones the entire history for inspection. Tree/backend paths append rejected-policy and over-budget blur diagnostics on repeated paints. Production wrappers use persistent thread-local engines (`surface/mod.rs:194–197,827–838`); repository callers only clear the collection in tests. Repainting an unchanged rejected command can therefore accumulate duplicate owned messages for the engine lifetime.

**Confidence: confirmed accumulator lifetime; memory cost unmeasured.** Define bounded retention, stable issue identity/deduplication and recovery while preserving new errors and surface provenance. Measure 1/100 rejected nodes over 1/1k/100k frames, diagnostic bytes/count, snapshot cost and newest-error visibility. Clearing indiscriminately before consumers inspect a frame could hide real errors; this is not a proposed blind clear. No execution checks were performed.

### S12-LOGIC-24 — Masked selection applies source UTF-8 offsets to ASCII mask positions

`surface/painter/widgets.rs:37–55` replaces each Unicode scalar with one ASCII mask character. Selection validation/slicing (`:85–105`) still uses the original input's byte offsets, although caret placement separately remaps offsets (`:192–207`). For `éx`, source selection 0..2 selects both masks; 0..3 is rejected against the shorter displayed string. The current production selection path is IME preedit annotation (`runtime_tree/annotate.rs:462–478`), not a claim that general pointer selection is shipped.

**Confidence: confirmed paint-input mismatch; live masked IME reproduction untested.** Share source-to-display endpoint mapping or explicitly define masked-preedit suppression. Cover accented text, emoji, combining sequences and reverse selections through retained payload and actual IME annotation, preserving original value and caret correctness.

### S12-LOGIC-25 — A single prepared backdrop can exceed the cache's retention budget

`surface/backdrop.rs:126–193` evicts existing entries while total bytes exceed the declared 96 MiB budget, then always retains the new image. An image larger than the budget remains cached after all other entries are removed. A 7680×4320 RGBA image is 132,710,400 bytes, exceeding that budget without integer overflow. This is distinct from the separate Skia image-cache oversize-upload item.

**Confidence: confirmed admission arithmetic; no large allocation probe.** Allow oversized one-shot painting without cache retention or use an explicit bounded preparation policy. Check admission with injected sizes rather than allocating giant fixtures, and verify ordinary rendering/output-size behavior and source/prepared accounting.

### Continuation coverage and refutations

Ten files were fully read: renderer manifest/lib/library adapters, surface mod/backdrop/buffer, painter, geometry/widgets/rounded. Tree lines 815–870 and 907–945 plus backend lines 1300–1338 were partially read. Cumulative coverage is 35 full, 8 partial and 35 unread of 78. A truncated mixed read was excluded from full credit. Existing rounded/clip caches have explicit bounded policies; no blanket unbounded-cache finding or safe public-wrapper removal was established. No source changes, tests, builds, probes or benchmarks were performed.

Consumer-scoped resource invalidation and glyph-page uploads overlap existing resource and rendering backlog items. The external-SVG transient-ready behavior is a more specific open finding. Older rendering reports remain historical evidence and were not treated as current proof.

## Refuted suspicions

The last-good raster fallback usually masks the next pending external-SVG job. Consuming a ready variant does not itself schedule a new job, so an idle self-sustaining loop or repeated blank frames is not established. PreparedScene image fields pin resource lifetimes despite compiler dead-code warnings.

## Tests and benchmarks needed

Run the per-finding workloads above with representative surface/node/resource counts, repeated release samples and recorded allocations, cache reuse and readiness. This read-only continuation ran no tests or benchmarks and makes no speedup claim.
