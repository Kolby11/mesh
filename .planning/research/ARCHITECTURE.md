# Architecture Research

**Domain:** Retained CPU rendering pipeline for a Wayland shell UI framework
**Researched:** 2026-05-10
**Confidence:** HIGH

## Standard Architecture

### System Overview

```text
┌─────────────────────────────────────────────────────────────┐
│                 Runtime / Component Invalidations           │
├─────────────────────────────────────────────────────────────┤
│  Script state │ Hover/focus │ Layout inputs │ Theme/state  │
└───────────────┬─────────────┬───────────────┬──────────────┘
                │             │               │
┌───────────────▼─────────────────────────────────────────────┐
│                 Retained Widget / Style Tree               │
├─────────────────────────────────────────────────────────────┤
│ Stable node ids │ dirty summaries │ restyle/layout outputs │
└───────────────┬─────────────────────────────────────────────┘
                │
┌───────────────▼─────────────────────────────────────────────┐
│                  Retained Render / Paint Data              │
├─────────────────────────────────────────────────────────────┤
│ render-object slots │ retained command blocks │ cache keys │
└───────────────┬─────────────────────────────────────────────┘
                │
┌───────────────▼─────────────────────────────────────────────┐
│             Damage / Visibility / Raster Execution         │
├─────────────────────────────────────────────────────────────┤
│ cull policy │ damage query │ text/icon/image caches │ paint │
└───────────────┬─────────────────────────────────────────────┘
                │
┌───────────────▼─────────────────────────────────────────────┐
│                 Pixel Buffer / Presentation                │
└─────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| Retained widget tree | Preserve stable UI identity and dirty classifications | Existing `WidgetNode` tree plus retained restyle/layout fast paths |
| Render-object layer | Track paint-facing slots and subtree ownership | Existing `RenderObjectTree`, extended with narrower dirty propagation |
| Retained paint-command cache | Own command blocks/ranges keyed by retained subtree identity | Evolved `RetainedDisplayList` rather than rebuilding a full flat command list |
| Damage execution index | Map changed regions to affected commands | Node ranges, spatial buckets, or similar local index |
| Raster caches | Reuse text, glyph, SVG, image, and icon outputs | Existing text/glyph caches plus new retained SVG/bitmap caches |

## Recommended Project Structure

```text
crates/core/shell/src/shell/component/
├── rendering.rs          # Stage timing and render pipeline orchestration
├── shell_component.rs    # Surface paint flow and damage selection
└── runtime_tree.rs       # Dirty/runtime annotation into retained tree

crates/core/frontend/render/src/
├── render_object.rs      # Retained paint-facing object diff
├── display_list.rs       # Retained command cache and damage metrics
└── surface/
    ├── painter/tree.rs   # CPU command execution
    ├── icon.rs           # SVG/bitmap/icon raster path
    ├── glyph.rs          # Font glyph cache
    └── text.rs           # Text layout and shaping cache
```

### Structure Rationale

- **`shell/component/`** owns orchestration, profiling, and invalidation policy because it already coordinates runtime, frontend, render, and presentation.
- **`mesh-core-render`** should keep renderer-specific retained structures and caches so frontend compiler/runtime crates do not absorb paint-specific concerns.

## Architectural Patterns

### Pattern 1: Dirty-Subtree Synchronization

**What:** Update only the affected retained subtree when style/layout/content changes stay local.  
**When to use:** Any time stable node IDs and dirty summaries prove that unrelated branches are unchanged.  
**Trade-offs:** Requires more bookkeeping, but prevents whole-tree command recollection.

### Pattern 2: Damage-Scoped Command Execution

**What:** Keep a fast mapping from damaged regions to command ranges so partial paints skip unrelated commands.  
**When to use:** Any surface where local state changes are common and full traversal becomes visible to users.  
**Trade-offs:** Needs careful ordering and clip correctness guarantees.

### Pattern 3: Retained Raster Outputs

**What:** Cache rasterized text/glyph/icon/image outputs with invalidation keyed to the actual visual inputs.  
**When to use:** Repeated paints of unchanged content, especially text-heavy or icon-heavy surfaces.  
**Trade-offs:** More memory usage and eviction policy work in exchange for less CPU raster work.

## Data Flow

### Request Flow

```text
User/state change
    ↓
Dirty flags + retained tree mutation
    ↓
Render-object diff
    ↓
Retained command update
    ↓
Damage selection + command filtering
    ↓
CPU paint into PixelBuffer
    ↓
Present with damage
```

### Key Data Flows

1. **Hover/restyle flow:** stateful key update -> retained restyle -> maybe layout reuse -> render-object diff -> filtered paint.
2. **Scroll/animation flow:** transform/scroll offset update -> retained command reuse -> damage-scoped repaint -> present.
3. **Text/icon flow:** retained command reuse -> text/glyph/icon cache lookup -> raster on miss only.

## Anti-Patterns

### Anti-Pattern 1: “Partial rendering” that still walks everything

**What people do:** Keep damage rectangles but still rebuild or rescan the full command tree every update.  
**Why it's wrong:** Users still pay most of the CPU cost, so rendering remains laggy.  
**Do this instead:** Retain subtree ownership and add command filtering before paint.

### Anti-Pattern 2: Reparsing SVGs and resizing bitmaps during steady-state paint

**What people do:** Cache source files but not raster outputs.  
**Why it's wrong:** Decode, parse, and resize cost returns every frame.  
**Do this instead:** Cache raster variants keyed by size, tint, and source identity.

## Integration Points

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| `mesh-core-shell` ↔ `mesh-core-render` | Direct Rust API calls | Keep orchestration/policy in shell, render-specific caches in render crate |
| Retained tree ↔ render objects | Stable node ids + dirty summaries | Existing boundary is correct; granularity needs tightening |
| Display list ↔ painter | Retained commands + damage/clip metadata | Add range/index data so the painter can skip unrelated work cheaply |

## Sources

- Qt Quick Scene Graph Default Renderer
- Qt Quick Performance Considerations
- Local MESH renderer code:
  - `crates/core/shell/src/shell/component/rendering.rs`
  - `crates/core/shell/src/shell/component/shell_component.rs`
  - `crates/core/frontend/render/src/render_object.rs`
  - `crates/core/frontend/render/src/display_list.rs`
  - `crates/core/frontend/render/src/surface/painter/tree.rs`
  - `crates/core/frontend/render/src/surface/icon.rs`

---
*Architecture research for: retained CPU rendering pipeline*
*Researched: 2026-05-10*
