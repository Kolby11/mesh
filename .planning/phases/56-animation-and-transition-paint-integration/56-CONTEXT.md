# Phase 56: Animation And Transition Paint Integration - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning
**Mode:** Smart discuss fallback defaults accepted automatically because interactive questions are unavailable in this runtime mode.

<domain>
## Phase Boundary

Phase 56 preserves current CSS/token animation and transition behavior while
making animated visual changes flow correctly through the retained painter
pipeline. It should inventory existing animation support, classify properties
into layout-affecting, paint-only, layer/effect, and unsupported buckets, then
route paint-only animation ticks through retained render-object/display-list
invalidation without forcing unnecessary layout. It should ensure animated
shadows, opacity, transforms, colors, filters, borders, and related visual
properties update visual bounds and damage correctly. It should not redesign
the author-facing animation model, add browser-wide animation semantics, move
animation ownership into Skia, or complete the broader stacking/damage policy
work reserved for Phase 57.

</domain>

<decisions>
## Implementation Decisions

### Animation Property Buckets
- **D-01:** Keep the current bounded animation vocabulary as the compatibility
  baseline: transition/keyframe behavior already represented by
  `AnimatedVisualStyle`, `TransitionProperties`, `AnimationStyle`, token
  resolution, and shipped `.mesh` animation declarations remains in scope.
- **D-02:** Classify properties by invalidation cost before changing behavior.
  Paint-only properties should include opacity, background color, border color,
  text color, border radius when geometry is unchanged, transform, box shadow,
  filter, backdrop filter, and other visual material changes that do not
  require new layout geometry.
- **D-03:** Layout-affecting properties should continue to trigger relayout:
  width, height, min/max dimensions, padding, margin, font size, line height,
  letter spacing when it affects text metrics, gap, inset/position inputs, and
  any property whose interpolation changes measured geometry.
- **D-04:** Treat `transition-property: all` conservatively. It may animate all
  supported visual fields, but if any active property in the node is
  layout-affecting, the tick should stay on the relayout path.
- **D-05:** Unsupported or non-interpolable animation properties should be
  visible through diagnostics rather than silently pretending support. Existing
  parser/resolver diagnostics for animation tokens and unsupported properties
  should be extended instead of adding a parallel diagnostic path.

### Retained Painter Invalidation
- **D-06:** MESH remains the owner of animation state and tick scheduling. Skia
  receives already-lowered painter commands; it should not know whether a style
  value came from a transition, keyframe, or static rule.
- **D-07:** Paint-only animation ticks should use `VISUAL_REPAINT` and update
  retained render-object/display-list material, transform, opacity, shadow,
  filter, border, and background signatures without rebuilding the Luau tree or
  forcing layout.
- **D-08:** Keyframe animations currently flow through `AnimatableStyle` and are
  marked as relayout when active. Phase 56 should narrow that behavior only
  where tests prove keyframe stops touch paint-only properties; otherwise keep
  the conservative relayout fallback.
- **D-09:** Transition start, restart, cancellation, delay, pause, fill-mode,
  and keyframe lifecycle behavior must remain deterministic for stable
  `_mesh_key` nodes. If a node loses identity, animation state may reset rather
  than attempting cross-node continuity.
- **D-10:** Direct widget-tree rendering and retained display-list replay must
  keep command-class parity while animated styles are active. Any paint-only
  tick that updates direct output must have an equivalent retained update or an
  explicit documented gap.

### Animated Bounds And Damage
- **D-11:** Animated visual bounds must be computed from the current animated
  style, not only the target static style. This includes shadow/filter overflow,
  opacity/layer effects, transforms, borders, and image/gradient backgrounds
  where they affect the painted region.
- **D-12:** Damage should include both previous and current animated visual
  bounds for moving or expanding effects so stale pixels are cleared. This is
  especially important for transforms, shadows, blur/filter radius changes, and
  border radius changes that expose/cover pixels.
- **D-13:** Keep Phase 56 damage work focused on animation correctness and
  repaint metrics. Broad stacking, z-index, clipping policy, and deterministic
  fallback-promotion tuning remain Phase 57.
- **D-14:** Add deterministic tests that assert paint-only animation ticks avoid
  layout when geometry does not change, and that layout-affecting animation
  ticks still trigger relayout.
- **D-15:** Add proof around repaint metrics or dirty flags so regressions are
  observable through existing profiling/debug payloads rather than requiring
  visual inspection only.

### Shipped Surface And Todo Handling
- **D-16:** Fold the pending Phase 31 audio popover transition-delay polish into
  Phase 56 as compatibility proof, not as a broad surface-transition redesign.
  The concrete requirement is to ensure current navigation/audio animation
  regressions stay within accepted behavior while painter animation changes are
  introduced.
- **D-17:** The audio popover work should recheck that transition timing does
  not reintroduce first-click, same-hover close, or first-drag input loss. If
  shell-owned show/hide lifecycle changes are needed, keep them minimal and
  tied to existing `hide_transition_ms` / `closing_until` behavior.
- **D-18:** Do not fold the module install requirement-resolution todo into
  Phase 56. It is module/planning architecture scope, not painter animation
  integration scope.

### the agent's Discretion
The planner may choose the exact bucket names, helper APIs, and task ordering.
Prefer a sequence that first locks an inventory and diagnostics, then narrows
paint-only invalidation, then proves animated visual bounds/damage, and finally
runs shipped navigation/audio regression proof. If a property cannot be safely
classified in Phase 56, document it as unsupported or conservative-relayout
behavior rather than widening the animation model.

### Folded Todos
- **Audio Popover Transition Delay Polish** —
  `.planning/todos/pending/2026-05-13-phase31-audio-popover-transition-delay.md`.
  Fold as Phase 56 proof debt because it directly concerns transition timing
  and navigation/audio animation regressions. Keep it bounded to accepted
  behavior and existing surface-transition lifecycle hooks.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project And Requirements
- `.planning/PROJECT.md` — Current v1.10 painter-engine goal, ownership split,
  and milestone scope.
- `.planning/REQUIREMENTS.md` — Phase 56 requirements ANIM-01, ANIM-02, and
  ANIM-03 plus Phase 57/58/59 boundaries.
- `.planning/ROADMAP.md` — Phase 56 roadmap entry, autonomous task seed, and
  success criteria.
- `.planning/spikes/MANIFEST.md` — Painter-engine direction and browser-scope
  exclusions from the Skia/painter roadmap spikes.

### Prior Phase Context
- `.planning/phases/53-element-and-display-list-primitive-coverage/53-CONTEXT.md`
  — Direct/retained command parity and deferred animation scope.
- `.planning/phases/54-skia-shape-path-text-highlight-and-border-migration/54-CONTEXT.md`
  — Skia paint-backend boundary, text boundary, and backend-neutrality proof.
- `.planning/phases/55-effects-layers-shadows-blur-images-and-gradients/55-CONTEXT.md`
  — Layer/effect/image/gradient decisions, animated-scope deferral, and visual
  bounds boundaries.

### Folded Todo
- `.planning/todos/pending/2026-05-13-phase31-audio-popover-transition-delay.md`
  — Accepted audio popover transition-delay polish to fold into Phase 56 proof.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/shell/src/shell/component/animation.rs` owns
  `AnimatedVisualStyle`, transition/keyframe application, active animation
  tracking, interpolation helpers, and current dirty-flag choice between
  `STYLE_RELAYOUT` and `VISUAL_REPAINT`.
- `crates/core/ui/elements/src/style/types.rs` defines
  `TransitionProperties`, `TransitionStyle`, `AnimationStyle`,
  `AnimationPlayState`, `Transform2D`, `BoxShadow`, and `VisualFilter`.
- `crates/core/ui/elements/src/style/parse.rs` parses transition and animation
  shorthands, token-resolved timings/easing, transition property names, and
  visual effect properties.
- `crates/core/frontend/render/src/render_object.rs` already tracks retained
  dirty categories for transform, opacity, material, geometry, text, and
  accessibility slots.
- `crates/core/frontend/render/src/display_list.rs` and
  `crates/core/frontend/render/src/surface/painter/tree.rs` own retained paint
  node data, command lowering, visual bounds, and painter replay.
- `crates/core/shell/src/shell/runtime/request.rs` already has
  `hide_transition_ms`, `closing_until`, and delayed hide behavior that can
  anchor bounded audio popover transition proof.
- Shipped animation examples live in
  `modules/frontend/navigation-bar/src/main.mesh` and component files under
  `modules/frontend/navigation-bar/src/components/*.mesh`.

### Established Patterns
- Animation mutates style/layout state, not script state; the hot path should
  avoid rebuilding the Luau tree for every tick.
- Unsupported or unresolved animation/style behavior should produce diagnostics
  instead of silent missing visuals.
- Renderer data structures below style/layout ownership must remain
  backend-neutral and Skia-free.
- Tests are colocated in Rust modules with descriptive behavior names and should
  run through `nix develop -c cargo test ...` when renderer native libraries are
  involved.

### Integration Points
- Animation application: `FrontendSurfaceComponent::apply_style_animations*` in
  `crates/core/shell/src/shell/component/animation.rs`.
- Dirty routing: `ComponentDirtyFlags::STYLE_RELAYOUT` and
  `ComponentDirtyFlags::VISUAL_REPAINT` through shell component invalidation.
- Style parsing/resolution: `crates/core/ui/elements/src/style/parse.rs`,
  `crates/core/ui/elements/src/style/resolve.rs`, and
  `crates/core/ui/elements/src/style/types.rs`.
- Retained render objects: `crates/core/frontend/render/src/render_object.rs`.
- Retained display and painter replay:
  `crates/core/frontend/render/src/display_list.rs` and
  `crates/core/frontend/render/src/surface/painter/tree.rs`.
- Shipped regression proof:
  `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs`
  and navigation/audio `.mesh` modules.

</code_context>

<specifics>
## Specific Ideas

The target proof should read like:

```text
current CSS/token transition or keyframe
  -> current animated ComputedStyle
  -> classify active properties as paint-only vs layout-affecting
  -> update retained render object/display-list signatures
  -> damage previous + current animated visual bounds
  -> painter backend executes ordinary commands with no animation-specific API
```

Start with the properties already present in `AnimatedVisualStyle` and
`TransitionProperties`. Do not add new author-facing animation syntax unless a
current shipped style or requirement already depends on it.

</specifics>

<deferred>
## Deferred Ideas

- Broad stacking, clipping, z-index, visual-bounds, and repaint-policy redesign
  belongs to Phase 57.
- Backend capability, selection, diagnostics, and rollback observability belongs
  to Phase 58.
- Full shipped-surface proof and renderer documentation closure belongs to Phase
  59.

### Reviewed Todos (not folded)
- **Define module install requirement resolution** —
  `.planning/todos/pending/2026-05-15-define-module-install-requirement-resolution.md`.
  Reviewed because `todo.match-phase` surfaced it, but deferred because it is
  module graph/resource architecture scope rather than painter animation
  integration scope.

</deferred>

---

*Phase: 56-Animation And Transition Paint Integration*
*Context gathered: 2026-05-23*
