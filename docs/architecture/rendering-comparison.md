# Rendering Architecture Comparison

## Purpose

This document compares the current MESH rendering architecture with an
experimental persistent scene/database renderer. It is an architecture note,
not a change to the public .mesh contract and not a commitment to replace the
current production renderer.

The central research question is whether MESH can extend its existing retained
identity beyond layout and display-list construction so that a small UI change
becomes a small scene mutation all the way down to rasterization and
presentation.

The proposed direction can be summarized as:

> Keep MESH's existing retained frontend, but move the retention boundary from
> "retained until paint commands" toward "retained until pixels."

The current Skia renderer remains the visual reference and rollback authority
while this direction is measured.

## Current MESH pipeline

The current implementation already has a substantial retained architecture:

~~~text
.mesh + Luau
      |
      v
component compiler
      |
      v
retained WidgetNode/runtime tree
      |
      +-- style resolution
      +-- retained Taffy layout
      +-- interaction and accessibility
      |
      v
RenderObjectTree
      |
      v
RetainedDisplayList
      |
      +-- retained paint subtrees
      +-- stable NodeId association
      +-- command spans
      +-- dirty-node scoped updates
      +-- damage rectangles
      +-- effect/layer scopes
      +-- repaint selection
      |
      v
backend-neutral paint commands
      |
      v
Skia raster backend
      |
      v
PixelBuffer
      |
      v
Wayland presentation
~~~

Important current implementation points include:

- crates/core/ui/elements owns the retained widget tree, style state, and
  Taffy-backed retained layout.
- crates/core/frontend/render/src/render_object.rs owns retained render-object
  synchronization and dirty summaries.
- crates/core/frontend/render/src/display_list owns retained paint entries,
  retained subtrees, replay spans, effect scopes, damage, and frame plans.
- FramePaintPlan is already an immutable hand-off describing paint inputs,
  topology, transforms, effects, replay coverage, and damage.
- DisplayListKey combines NodeId with a primitive slot such as Background,
  Border, Text, Icon, or Generic.
- the painter boundary is backend-neutral even though Skia is currently the
  authoritative raster implementation.
- crates/core/presentation preserves damage through Wayland SHM presentation.
- Taffy, Parley, AccessKit, AnyRender, and vello_encoding integration/proof
  seams already exist in the workspace.

This means MESH is not starting from a conventional immediate-mode renderer.
The current architecture already performs retained layout, retained render
synchronization, retained display-list reuse, and damage-aware partial paint.

## Proposed persistent scene/database pipeline

The experimental renderer would preserve the existing frontend but add a
lower-level retained representation:

~~~text
.mesh runtime state
        |
        v
retained WidgetNode tree
        |
        v
RenderObjectTree
        |
        v
Visual Scene IR / PersistentScene
        |
        +-- Geometry[]
        +-- Material[]
        +-- Transform[]
        +-- Clip[]
        +-- GlyphRun[]
        +-- Instance[]
        |
        +-- stable primitive identity
        +-- scene patches
        +-- dependency metadata
        |
        v
tile dependency database
        |
        +-- primitive -> tiles
        +-- material -> instances
        +-- effect -> expanded tile region
        |
        v
persistent raster/GPU resources
        |
        v
dirty tiles / changed resources only
        |
        v
compositor and presentation
~~~

The key difference is the unit of work.

The current renderer ultimately asks which paint commands must be replayed for
a frame. The proposed renderer asks which retained scene records changed since
the last committed scene.

For example, changing the background of one button should ideally become:

~~~text
NodeId 481 / Background
        |
        v
InstanceId 8912
        |
        v
MaterialId 91 changed
        |
        +-- update one material record
        +-- invalidate the tiles that reference it
        |
        v
raster/composite only those tiles
~~~

Geometry, glyph data, clip topology, and unrelated instances remain untouched.

## Side-by-side comparison

| Area | Current MESH | Persistent scene/database direction |
| --- | --- | --- |
| Author contract | .mesh + Luau + bounded CSS-like styling | unchanged |
| Public renderer types | hidden from authors | unchanged |
| Stable UI identity | NodeId | keep and extend into primitive/scene IDs |
| Layout | retained Taffy | keep |
| Style resolution | retained MESH style state | keep |
| Accessibility | MESH semantics + AccessKit adapter | keep |
| Render synchronization | RenderObjectTree fingerprints/dirty scopes | keep, then emit scene patches |
| Paint identity | DisplayListKey and retained paint subtree | map to stable InstanceId/GeometryId/MaterialId |
| Frame representation | FramePaintPlan + retained command topology | use as migration source, later share a Visual Scene IR |
| Damage | logical damage rectangles and replay widening | keep, then add primitive/tile dependency damage |
| Raster authority | Skia software paint | Skia remains reference; experimental backend runs beside it |
| Resource residency | CPU caches and PixelBuffer-oriented paint | persistent CPU/GPU geometry, material, glyph, image resources |
| Transform change | can require retained command/replay work | mutate a Transform record independently |
| Color/theme change | resolved into paint data and replayed | mutate semantic material/token data independently |
| Scrolling | retained interaction state with paint implications | compositor-owned transform where valid |
| Animation | MESH animation state drives paint updates | selected paint-only transforms/materials can advance below script |
| Arbitrary vector path | Skia | reuse Vello/AnyRender-compatible path strategy rather than inventing one |
| Partial update granularity | dirty nodes, retained subtrees, command spans, damage rectangles | changed scene records and affected tiles |
| Presentation | Wayland SHM today; GPU migration planned | reuse existing presentation contracts; GPU path is separate from scene model |
| Rollback | current renderer | always retain Skia/reference path until promotion gates pass |

## What should be reused

The persistent renderer should not replace the parts of MESH that already solve
the difficult frontend problems.

### Retained NodeId identity

NodeId is already propagated through layout, interaction, accessibility, render
objects, and display-list data. The scene layer should derive stable primitive
identity from it instead of introducing an unrelated identity system.

DisplayListKey is particularly useful because it already models a stable
primitive slot:

~~~text
(NodeId, Background)
(NodeId, Border)
(NodeId, Text)
(NodeId, Icon)
(NodeId, Generic)
~~~

An experimental scene synchronizer can map those keys to stable InstanceId
values.

### Retained Taffy layout

The layout engine already keeps a Taffy tree and updates dirty nodes
incrementally. The scene renderer should consume final geometry; it should not
own layout or duplicate Taffy.

### RenderObjectTree

The render-object layer is already the point where retained visual state and
dirty categories are synchronized. It is the natural place to produce narrower
scene changes once the scene model exists.

### RetainedDisplayList and FramePaintPlan

The display list already contains correctness work that should not be discarded:
paint ordering, effect isolation, layer scopes, damage expansion, backdrop
regions, replay coverage, batching metadata, and diagnostics.

The first persistent-scene prototype should consume FramePaintPlan rather than
replacing the display list. That allows both renderers to observe exactly the
same authoritative frame intent.

Longer term, a shared Visual Scene IR may become the source from which both a
Skia display-list view and a GPU scene view are derived. That should happen only
after measurements prove the scene model.

### Existing Skia backend

Skia should remain the fidelity oracle, production renderer, and fallback while
the new architecture is experimental. The existing Skia-GL/EGL production
migration and the scene-database research lane solve different problems:

~~~text
production migration:
current paint semantics -> Skia GPU -> EGL/Wayland

renderer research:
retained visual state -> persistent scene -> tile/GPU database
~~~

They can proceed independently.

## What changes

### 1. Add a stable scene representation

The first new layer should model visual state independently from paint command
replay:

~~~rust
PersistentScene {
    instances,
    geometries,
    materials,
    transforms,
    clips,
    glyph_runs,
    primitive_map,
}
~~~

Conceptual IDs include:

- InstanceId
- GeometryId
- MaterialId
- TransformId
- ClipId
- GlyphRunId

These IDs are internal implementation details. NodeId remains the author/runtime
identity.

### 2. Represent updates as patches

A scene update should describe what changed rather than reconstructing a frame:

~~~text
InsertInstance
RemoveInstance
ReplaceGeometry
UpdateMaterial
UpdateTransform
UpdateClip
UpdateGlyphRun
ReorderInstance
~~~

A color-only update should not replace geometry. A transform-only animation
should not rebuild text shaping. A theme update should not force layout unless
the changed token affects layout.

### 3. Add dependency-indexed damage

Current damage is already sparse in logical surface coordinates. The
experimental renderer would add a second index:

~~~text
InstanceId -> tile set
MaterialId -> dependent instances
GeometryId -> dependent instances
Effect/layer -> expanded dependency region
~~~

This enables work proportional to the changed scene data and affected output
tiles rather than proportional to the complete command stream.

### 4. Preserve semantic primitives longer

The renderer should avoid converting every UI object immediately into a generic
path.

Useful retained primitive forms include:

- rectangle
- rounded rectangle
- border
- glyph run
- image
- icon/vector path
- shadow
- selection highlight
- caret
- clip
- compositing/effect group

Rectangles, rounded rectangles, borders, and simple gradients can use
specialized analytic paths. Complex vectors can use an existing vector
implementation such as the Vello-compatible path instead of a custom path
rasterizer.

### 5. Separate geometry from material

A central experiment is to decouple coverage/geometry from appearance.

One rounded rectangle can retain geometry while its material changes from a
hover state or theme token. One icon geometry can be reused with several
semantic colors.

This is especially relevant to MESH because semantic theming is a platform
requirement.

### 6. Push semantic theme references lower

The experimental scene may eventually retain a material source such as:

~~~text
Material.background = ThemeToken(surface-raised)
~~~

rather than only the already-resolved RGBA value.

That would let a theme generation update a compact token/material table and
invalidate dependent instances without rebuilding their geometry.

This should be introduced only after the current theme precedence and style
contracts remain demonstrably unchanged.

## Compiler/runtime opportunity

The renderer is only half of the incremental chain.

The current runtime already has dirty-node tracking, service-field observation,
component memoization, and retained-tree diffing, but some narrow state changes
still require broader template evaluation before the retained renderer
discovers the small visual change.

A later optimization can connect authored dependencies to retained properties:

~~~text
state/service field
        |
        v
compiled expression dependency
        |
        v
NodeId + property
        |
        v
layout/style/paint dirty category
        |
        v
scene record
        |
        v
affected tiles
~~~

This is the Svelte-like part of the architecture: use compilation to know which
property depends on which state rather than relying only on whole-template
rerender and later diffing.

That work should remain language-independent. The current runtime is Luau and
the platform specification explicitly leaves TypeScript/JavaScript undecided.

## Expected advantages

If the model works, common operations could become substantially narrower:

- hover: material mutation
- opacity transition: material/compositor mutation
- translation animation: transform mutation
- text value change: glyph-run replacement plus bounds/layout only if metrics
  change
- icon recolor: material mutation
- system theme change: token/material generation update
- scrolling: compositor transform plus newly exposed tile work
- static idle surface: zero scene mutation and no raster work

The target property is not merely "GPU accelerated." It is that cost should
trend toward:

~~~text
O(changed retained records + affected tiles)
~~~

instead of scaling with the whole widget tree or complete display list for
small updates.

## Risks

The architecture does not make difficult graphics problems disappear.

The largest correctness risks are:

- transparent ordering and blend barriers;
- filters and backdrop filters whose dependencies extend beyond local bounds;
- clip and effect-stack topology changes;
- fractional-scale text and cached glyph sharpness;
- resource lifetime and GPU allocator fragmentation;
- preserving exact Skia-visible behavior while specialized primitives use
  different raster paths;
- excessive tile-cache memory;
- over-engineering GPU scheduling for workloads where a CPU update is cheaper.

These are reasons to build the scene layer first as a measurable shadow model,
not reasons to replace the current renderer immediately.

## Architectural conclusion

The useful distinction is:

~~~text
MESH today:
retained through layout, render objects, and paint-command selection

experimental direction:
retain visual identity through raster resources and output dependencies
~~~

The project already has most of the prerequisites: stable NodeId identity,
retained Taffy layout, retained render objects, sparse dirty-node scopes,
retained paint subtrees, damage, backend-neutral painter commands, profiling,
semantic theming, AccessKit integration, resource caches, and Wayland damage
presentation.

The research work should therefore evolve the current renderer instead of
replacing the frontend. The first proof should add a PersistentScene beside the
existing FramePaintPlan and measure scene mutations without changing pixels on
screen.

See the execution design in
../../.planning/todos/pending/2026-09-20-persistent-scene-renderer.md.
