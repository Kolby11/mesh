# Persistent Scene / Database Renderer — implementation plan

Created: 2026-09-20
Area: rendering
Status: design for an experimental, reversible renderer lane

Related architecture:
- docs/architecture/rendering-comparison.md
- docs/frontend/renderer-contract.md
- .planning/renderer/migration.md
- .planning/todos/pending/2026-07-15-gpu-rendering-backend.md

## Goal

Test whether MESH can extend retained identity from the current widget,
render-object, and display-list layers into a persistent visual scene whose
updates are expressed as small mutations and whose raster work is limited by
dependency-indexed damage.

The experiment must answer this question before any production migration:

> For representative MESH workloads, does a persistent scene database reduce
> CPU work, allocation, resource upload, and rasterized area enough to justify
> the added complexity while preserving the current renderer contract?

This is not a whole-renderer rewrite. The authoritative Skia path remains
available throughout the experiment.

## Non-goals

This plan does not:

- change .mesh syntax;
- commit MESH to TypeScript/JavaScript;
- replace Luau;
- replace Taffy layout;
- move style resolution into the GPU;
- replace AccessKit or interaction ownership;
- invent a new general-purpose vector rasterizer;
- remove the retained display list at the start;
- replace the existing Skia-GL/EGL production GPU plan;
- weaken current damage, diagnostics, accessibility, or presentation
  contracts.

## Design principles

### Reversible from the first commit

Every phase must either be observation-only or remain behind an experimental
feature/configuration gate. The existing Skia renderer is the rollback
authority.

### Reuse current stable identity

NodeId and DisplayListKey are the starting identities. The scene layer may
introduce internal IDs, but it must preserve a direct mapping back to MESH
nodes and primitive slots for diagnostics and profiling.

### Measure before moving work to the GPU

The project should first prove that the persistent representation produces
small patches on the CPU. GPU residency is a later phase.

### Keep correctness decisions above backend code

MESH continues to own visual ordering, style/layout semantics, effect topology,
dirty categories, damage policy, diagnostics, and presentation contracts.
Backend code owns efficient realization of the already-decided scene.

### Prefer specialized UI primitives with a generic fallback

Rectangles, rounded rectangles, borders, glyph runs, images, and common shadows
should retain their semantics. Arbitrary paths should use an existing
Vello/AnyRender-compatible implementation where possible.

## Proposed ownership

The first implementation should stay inside mesh-core-render:

~~~text
crates/core/frontend/render/src/
    scene/
        mod.rs
        id.rs
        geometry.rs
        material.rs
        transform.rs
        clip.rs
        instance.rs
        patch.rs
        sync.rs
        metrics.rs

    tile/
        mod.rs
        grid.rs
        dependency.rs
        damage.rs
        program.rs       # later phase

    gpu/                 # only after CPU scene proof
        mod.rs
        buffers.rs
        resources.rs
        renderer.rs
~~~

The existing display_list, render_object, surface, and presentation ownership
stay unchanged initially.

## Core model

The initial CPU-only model should be intentionally small.

~~~rust
struct PersistentScene {
    instances: SlotMap<InstanceId, Instance>,
    geometries: SlotMap<GeometryId, Geometry>,
    materials: SlotMap<MaterialId, Material>,
    transforms: SlotMap<TransformId, Transform>,
    clips: SlotMap<ClipId, Clip>,
    glyph_runs: SlotMap<GlyphRunId, GlyphRun>,

    primitive_map: HashMap<DisplayListKey, InstanceId>,
}
~~~

A scene instance refers to independently retained records:

~~~rust
struct Instance {
    owner: NodeId,
    slot: DisplayPrimitiveSlot,
    geometry: GeometryId,
    material: MaterialId,
    transform: TransformId,
    clip: ClipId,
    z_order: u32,
    flags: InstanceFlags,
}
~~~

The first geometry enum should cover only the primitives needed to prove the
model:

~~~rust
enum Geometry {
    Rect(...),
    RoundedRect(...),
    Border(...),
    GlyphRun(GlyphRunId),
    Image(...),
    Path(...),
}
~~~

The patch model should be explicit and observable:

~~~rust
enum ScenePatch {
    InsertInstance(InstanceId),
    RemoveInstance(InstanceId),
    ReplaceGeometry(GeometryId),
    UpdateMaterial(MaterialId),
    UpdateTransform(TransformId),
    UpdateClip(ClipId),
    UpdateGlyphRun(GlyphRunId),
    ReorderInstance(InstanceId),
}
~~~

The exact Rust representation may change after measurement; the separation of
geometry/material/transform/clip is the property being tested.

## Phase 0 — measurement contract

Objective: define the metrics before changing renderer behavior.

Add scene-experiment metrics to the existing profiling/debug model. At minimum
capture:

- scene instances total;
- instances inserted/removed;
- geometry records changed;
- material records changed;
- transform records changed;
- clip records changed;
- glyph runs changed;
- scene patch count and encoded bytes;
- scene synchronization time;
- affected logical area;
- affected tile count once tiling exists;
- estimated/actual resource upload bytes once GPU residency exists.

The metrics must identify the surface/component and preserve NodeId attribution
where practical.

Gate: no visual behavior changes. Existing renderer tests remain identical.

## Phase 1 — shadow PersistentScene from FramePaintPlan

Objective: prove stable scene identity without rendering from it.

After RetainedDisplayList produces FramePaintPlan, feed the same immutable plan
to SceneSynchronizer. Build/update a PersistentScene beside the Skia path.

The synchronizer should derive a stable key from DisplayListKey and command
topology. The experiment should preserve stable InstanceId values for unchanged
primitives across frames.

At this stage:

~~~text
FramePaintPlan
     |
     +----> current Skia path -> pixels
     |
     +----> SceneSynchronizer -> PersistentScene -> metrics only
~~~

No scene output reaches presentation.

Required proof cases:

- first frame inserts all expected primitives;
- no-op frame produces zero scene patches;
- background color change produces a material-oriented patch rather than
  replacing unrelated records;
- transform-only update changes the transform record;
- text change replaces only text/glyph-related records unless its measured
  size forces geometry/layout changes;
- node removal removes its primitive instances;
- subtree insertion creates stable keys for descendants;
- effect/layer topology changes are observable and conservatively classified.

Promotion criterion: stable identity and patch classification are deterministic
under the current test suite.

## Phase 2 — canonical primitive separation

Objective: determine whether the current paint payload can be decomposed into
independently mutable scene records without losing semantics.

Refine the synchronizer so changes are classified into:

- topology;
- geometry;
- material;
- transform;
- clip;
- text/glyph;
- resource;
- effect scope.

Do not infer these only from raw hashes. Use typed current render/display data
where possible so a field change has an explicit category.

This phase should expose where the current DisplayPaintNode representation
couples values that the scene wants to retain independently.

Promotion criterion: the common interaction workloads produce narrow patch
types and no correctness fallback is hidden.

## Phase 3 — tile dependency index

Objective: convert scene mutations into bounded output dependencies.

Introduce a configurable logical/device tile grid for the experiment. Start
with one or two fixed sizes such as 32x32 and 64x64 and measure both.

Maintain:

~~~text
InstanceId -> covered tile set
TileId     -> ordered contributing instances
~~~

When a scene record changes, derive affected tiles from both the old and new
visual bounds.

Effects must widen dependency coverage. Blur, shadow, backdrop readback, and
compositing groups cannot use only the primitive's local box.

The existing damage rectangles remain authoritative for presentation. Tile
damage is experimental evidence until parity is proven.

Promotion criterion: unioned tile damage contains current logical damage for
all canonical tests, or any intentional difference is documented and proven
safe.

## Phase 4 — retained tile programs on the CPU

Objective: test whether keeping per-tile replay topology is useful before
building a GPU backend.

For tiles whose topology is unchanged, retain a compact ordered program or
instance list. Rebuild a tile program only when an insertion/removal/reorder or
effect topology change affects that tile.

Material or transform changes should normally keep the tile program itself
stable and only mark it for execution.

Conceptually:

~~~text
Tile 381:
  PushClip C4
  Draw I71
  Draw I72
  DrawText I73
  PopClip
~~~

Do not optimize the bytecode prematurely. The first goal is to measure how
often tile topology remains stable in real shell workloads.

Promotion criterion: retained tile-program rebuild count is substantially
lower than executed dirty-tile count in mutation-heavy workloads, and memory is
bounded.

## Phase 5 — CPU reference executor

Objective: make the scene model independently testable without committing to a
GPU API.

Implement a minimal scene/tile executor for a deliberately small primitive
subset, or translate selected scene primitives back through the existing
backend-neutral painter interface.

Recommended first subset:

- solid rect;
- rounded rect;
- uniform border;
- glyph/text proof path;
- image proof path;
- clip.

Keep complex filters, blend modes, and unsupported primitives on the Skia
reference path or mark the experimental scene as incomplete.

The value of this phase is architectural validation and deterministic tests,
not production speed.

Promotion criterion: selected canonical scenes produce pixel-equivalent output
within defined tolerance and preserve ordering/clip semantics.

## Phase 6 — persistent GPU resource mirror

Objective: prove that scene patches become small GPU uploads.

Only after the CPU scene model is stable, introduce wgpu or the selected GPU
abstraction behind an experimental feature.

Keep persistent buffers/tables for:

- instances;
- geometry descriptors;
- materials;
- transforms;
- clips;
- glyph/image resource descriptors;
- theme/token data if that experiment is enabled.

Updates should write only changed ranges where practical.

New metrics become mandatory:

- bytes uploaded per frame;
- bytes uploaded per scene patch;
- buffer reallocations;
- resource evictions;
- GPU memory used by scene/tile caches.

Promotion criterion: simple mutations such as hover, recolor, opacity, or
translation produce bounded small uploads instead of whole-scene rebuilds.

## Phase 7 — specialized raster paths plus vector fallback

Objective: test a heterogeneous UI renderer instead of routing every primitive
through one generic path algorithm.

Candidate realization:

| Primitive | Candidate path |
| --- | --- |
| rect | analytic quad |
| rounded rect | analytic coverage |
| simple border | analytic |
| gradient | material shader |
| image | textured quad |
| glyph run | cached glyph/coverage path |
| simple shadow | specialized implementation after parity proof |
| complex path/icon | Vello/AnyRender-compatible vector path |
| unsupported/effect-heavy content | Skia/reference fallback during experiment |

MESH should not implement a general SVG/path engine before measurements show
that the reused vector backend is a bottleneck.

Promotion criterion: specialized primitives preserve accepted visual behavior
and beat the generic/reference path on their intended workloads.

## Phase 8 — GPU/tile renderer comparison

Objective: compare the new scene execution model against the current renderer
using representative workloads.

The benchmark matrix should include at least:

1. large static settings-style tree at idle;
2. one button hover in a large tree;
3. one text value changing repeatedly;
4. theme switch;
5. continuously animated transform/opacity;
6. long scrolling surface;
7. icon-heavy grid;
8. text-heavy localized surface;
9. blur/filter-heavy surface;
10. fractional-scale surface with partial damage.

For each workload record:

- tree/node count;
- primitive count;
- changed state shape;
- build profile;
- repeated-run ranges;
- CPU prepare/sync time;
- raster/GPU time;
- total frame time;
- allocations;
- damage area;
- tile count;
- scene patch count;
- GPU upload bytes;
- memory;
- worst-frame latency.

Performance claims must be recorded in .planning/log/performance-log.md under
the repository's normal measurement rules.

## Phase 9 — compositor-owned transforms and animations

Objective: use the retained scene model to remove script/frame work only where
semantics permit it.

Candidates:

- scroll offset;
- opacity;
- translation;
- scale;
- selected color/material interpolation.

A running native animation should be submitted once with its target scene
record and timeline, while the Rust/GPU rendering layer advances it.

Do not move layout-affecting animation or application state semantics below the
runtime merely for speed.

Promotion criterion: animations remain correct if script execution stalls
briefly, and reduced-motion/accessibility policy still has one authoritative
owner.

## Phase 10 — compiler/runtime dependency bridge

Objective: connect MESH's existing state/service dependency tracking to exact
retained properties so the renderer does not need a broad template rebuild to
discover every small visual mutation.

This work belongs across compiler/runtime/retained-tree boundaries, not inside
the GPU backend.

Desired chain:

~~~text
changed script/service member
        |
        v
affected compiled expression
        |
        v
NodeId + typed property
        |
        v
style/layout/paint invalidation category
        |
        v
scene patch
        |
        v
affected tiles
~~~

This must remain compatible with Luau. A future TypeScript/JavaScript decision
may provide a different frontend compiler, but the dependency/scene contracts
should not require that language decision.

Promotion criterion: representative narrow service/state changes avoid the
current broad template-evaluation path where semantics allow it.

## Phase 11 — authority decision

Objective: decide whether the experimental renderer has earned a production
role.

Possible outcomes are all valid:

1. reject the experiment and keep the current renderer;
2. keep the scene database only as a profiling/invalidation optimization;
3. use the scene database as an intermediate representation feeding Skia;
4. promote the new GPU renderer for a subset of primitives with Skia fallback;
5. promote it as the primary renderer after complete parity.

A production promotion requires:

- renderer-contract parity;
- accessibility/input behavior unchanged;
- effect and blend correctness;
- fractional-scale correctness;
- bounded resource use;
- diagnostics parity;
- reliable fallback/rollback;
- measurable wins on representative workloads;
- no unacceptable build/startup regression.

## Relationship to the existing GPU backend plan

The existing 2026-07-15 GPU plan remains the conservative production migration:

~~~text
Skia raster -> Skia GL/Ganesh -> EGL damage-aware present
~~~

This plan is a separate research lane:

~~~text
retained MESH state -> persistent scene -> dependency tiles -> GPU scene renderer
~~~

The production lane answers how to move today's paint semantics onto the GPU
with low migration risk. The research lane asks whether a different retained
execution model can reduce the amount of work performed in the first place.

The research lane must not block the production lane unless measurements show a
clear shared prerequisite.

## Suggested first implementation slice

The first code change should be intentionally small:

1. add scene IDs and PersistentScene data structures;
2. add SceneSynchronizer consuming FramePaintPlan;
3. map DisplayListKey to stable InstanceId;
4. emit ScenePatch records;
5. add scene metrics to the existing profiling snapshot;
6. add focused tests for no-op, material-only, transform-only, text, insertion,
   removal, and topology changes;
7. keep scene output completely disconnected from painting.

That slice answers the most important architectural question with minimal risk:
whether MESH's existing retained frame data can be converted into stable,
fine-grained scene mutations.

If it cannot, the experiment can stop without changing production rendering.
If it can, tile dependencies and GPU residency become evidence-driven next
steps.
