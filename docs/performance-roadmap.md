# Rendering Performance Roadmap

This document records the target direction for making the MESH shell feel closer
to a native Qt/Qt Quick engine while preserving the flexibility of the current
`.mesh` plus bounded CSS authoring model.

The core principle is simple: Qt-like performance comes primarily from not doing
unnecessary work. A GPU backend helps most after the renderer stops rebuilding,
relayouting, clearing, and repainting the whole surface for small state changes.

## Current Gap

The current pipeline is still mostly whole-tree and whole-surface:

- `WidgetNode` is still rebuilt from evaluated template/script state on full
  dirty renders.
- Runtime nodes now receive deterministic stable IDs from their `_mesh_key`, so
  logical node identity survives full rebuilds when the tree shape is stable.
- The style-only path can skip the script-driven tree build and now mutates the
  cached `WidgetNode` tree instead of cloning it. It still reruns runtime
  annotation, restyle, layout, metrics publishing, full buffer clear, and full
  paint.
- Paint allocates a fresh `PixelBuffer`, clears the entire surface, paints the
  whole tree, and presents the whole buffer.
- `WidgetNode` stores generic `HashMap<String, String>` attributes and string
  tags, which is flexible but expensive in hot render, style, layout, hit-test,
  and paint loops.

This means the shell has the first retained-tree boundary, but it is not yet a
complete retained native scene graph.

## Implementation Status

Improvement 1 status: implemented for the widget-tree layer.

- Deterministic runtime `NodeId`s derived from stable runtime keys.
- Retained style-only path moves the cached `WidgetNode` tree out of
  `last_tree`, mutates it, paints it, then stores it back.
- Retained widget-tree snapshot index records stable node membership and dirty
  categories per frame: inserted, removed, layout, style, attributes, children,
  and state.
- Full dirty rebuilds no longer animate from the previous retained visual style
  snapshot; transition snapshots are reserved for the retained style-only path.
- Focus-visible annotation for focused text inputs is deterministic even when a
  test or runtime path seeds logical focus directly.

Still pending:

- Persistent render-object tree separate from `WidgetNode`.
- Using retained dirty summaries to skip clean subtrees.
- Incremental full style recomputation for retained nodes.
- Incremental layout.
- Retained display list.
- Damage tracking and partial repaint.
- GPU upload/batching.

## Priority Order

| Priority | Change | Effort | Expected Win | Reason |
| --- | --- | --- | --- | --- |
| 1 | Retained widget tree with stable node identity and dirty summaries | done for widget layer | 3-10x with many components once consumed by later stages | Establishes the architecture needed for incremental updates instead of full rebuilds. |
| 2 | Dirty-type invalidation model | days-weeks | 2-10x depending on interaction | Separates state, style, layout, paint, text, accessibility, and surface dirtiness so small changes do not trigger unrelated work. |
| 3 | Incremental style and layout propagation | weeks | 2-8x | Recomputes only affected nodes and ancestors instead of running whole-tree restyle/layout for hover, focus, scroll, and animation ticks. |
| 4 | Retained display list plus damage tracking | weeks | 2-5x when little changes | Keeps paint primitives around and repaints only changed regions instead of clearing and repainting the full surface. |
| 5 | Text shaping and glyph cache | days-weeks | large for text-heavy UI | Native UI performance depends heavily on cached shaped text runs and glyph data. Text can dominate shell surfaces. |
| 6 | Typed attribute/style slots and interned identifiers | days-weeks, broad | 1.5-3x in hot loops | Replaces repeated string/hash lookups with compact typed data for common attributes, tags, classes, and event/state fields. |
| 7 | Selector indexing for restyle | days | about 2x restyle | Pre-buckets rules by tag, class, id, pseudo-state, and container dependency so restyle does not scan every rule for every node. |
| 8 | Display-list batching | weeks | 2-5x before GPU, more with GPU | Groups same-kind primitives such as rects, borders, glyphs, and icons. This improves software rendering and prepares the GPU backend. |
| 9 | GPU backend through wgpu/Vulkan/OpenGL | weeks-months | 5-20x after retention | Moves compositing and raster-heavy paths to the GPU after the retained display list is stable enough to avoid wasteful re-uploads. |
| 10 | Parallel paint/layout where data ownership allows it | days-weeks | scales with cores | Useful after `RefCell`/shared mutation bottlenecks are removed, but it should not mask avoidable whole-tree work. |

## Recommended Implementation Sequence

1. Add stable runtime node identity and a retained tree boundary. Done for the
   widget-tree layer.
2. Add dirty flags by work type: script/state, style, layout, paint, text,
   accessibility, metrics, and surface configuration.
3. Make hover, focus, active, checked, scroll, and animation updates mark only
   the smallest valid dirty scope.
4. Make style resolution incremental and selector-indexed.
5. Make layout incremental, with upward propagation only when geometry can
   affect ancestors or siblings.
6. Lower the retained tree into a retained display list.
7. Add damage rectangles and partial buffer repaint.
8. Add text shaping/glyph caches and invalidate them only on text/font-affecting
   changes.
9. Add batching, then GPU upload paths for retained display-list batches.

## Crate Direction

These are the default crate choices for future retained rendering and
responsiveness work. They should be treated as implementation accelerators, not
as a substitute for the retained invalidation architecture above.

| Area | Crate direction | Use |
| --- | --- | --- |
| Retained widget/render tree | `slotmap` | Store retained nodes with stable generational keys and use `SecondaryMap` for parallel per-node data. Prefer this over long-term `HashMap<NodeId, ...>` storage. |
| Dirty invalidation | `bitflags`, `slotmap::SecondaryMap`, custom engine logic | Keep dirtiness engine-specific, but represent dirty categories with flags and store per-node dirty state beside retained nodes. |
| Incremental layout | `taffy` | Strong candidate for real CSS-like Flexbox/Grid layout with custom measurement. Evaluate as a replacement for or supplement to the current custom `LayoutEngine`. |
| Retained display list | `kurbo`, `peniko`, possibly `vello_encoding` or `vello` | Use Linebender stack concepts. If staying custom, use `kurbo`/`peniko` types with our own display list. If moving to GPU/vector rendering, prototype `vello` before committing. |
| Damage tracking | `euclid`, custom rect coalescing | Use `euclid` geometry types. Keep damage union/coalescing custom unless it becomes more complex than expected. |
| Text shaping and glyph cache | `cosmic-text`, later possibly `glyphon` or `parley` | Keep `cosmic-text` now. If a `wgpu` backend lands, evaluate `glyphon` because it renders text through `wgpu` and builds on `cosmic-text`. Evaluate `parley` for richer text layout. |
| Typed attributes and interned strings | `lasso`, `smol_str`, `rustc-hash` or `ahash` | Use `lasso` for interned tags/classes/attribute names, `smol_str` for short inline strings, and a fast non-adversarial hasher for hot internal maps. |
| Selector indexing | existing `lightningcss` / transitive `parcel_selectors` | Reuse or lower Lightning CSS selector data instead of inventing selector parsing. Matching and indexing still need custom integration. |
| GPU backend | `wgpu`, possibly `vello` | Use `wgpu` as the portable GPU layer. Prototype `vello` for 2D vector rendering before depending on it for the whole renderer. |
| Parallel paint/layout | `rayon` | Add only after retained data is immutable or split into disjoint buffers with a clean ownership model. |
| Texture/glyph atlas | `etagere` or `guillotiere` | Use for icon, glyph, and image atlas allocation when batching/GPU work starts. Prefer `etagere` initially for dynamic atlas allocation unless tests show `guillotiere` fits better. |

Shortlist for planning: `slotmap`, `bitflags`, `taffy`, `lasso`/`smol_str`,
`wgpu` plus either `vello` or a custom display-list renderer, `etagere`, and
later `rayon`.

## Near-Term Codebase Targets

- Consume retained widget-tree dirty summaries in style/layout/paint so clean
  subtrees can be skipped.
- Extend the retained widget-tree cache into a retained render-object cache.
- Track whether a style change is paint-only or layout-affecting before calling
  `LayoutEngine::compute_with_measurer`.
- Avoid full `PixelBuffer::clear` and full-tree paint when damage is smaller
  than the surface.
- Keep cached shaped text runs keyed by text content, font family, size, weight,
  line height, wrapping width, and locale/direction.
- Introduce typed fast paths for common node data while keeping a generic
  fallback map for uncommon/custom attributes.

## Benchmark Requirement

Performance work should be tied to repeatable measurements. At minimum, track:

- tree build time
- runtime annotation time
- style/restyle time
- layout time
- text measurement/shaping time
- paint time
- present/commit time
- redraw count and damaged area
- input-to-visible-response latency

Canonical scenarios should include hover changes, surface open/close, slider
drag, keyboard traversal, scroll, animation ticks, and backend-driven state
updates on shipped surfaces.

## Non-Goal

The target is not a full browser engine. MESH should keep its bounded UI/CSS
profile, lower author-friendly syntax into typed internal data, and preserve a
small predictable runtime renderer.
