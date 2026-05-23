---
phase: 56-animation-and-transition-paint-integration
status: complete
created: 2026-05-23
research_type: inline-orchestrator
requirements: [ANIM-01, ANIM-02, ANIM-03]
---

# Phase 56 Research: Animation And Transition Paint Integration

## Research Question

What does the planner need to know to make current CSS/token animation support
drive retained painter updates without broad browser animation scope?

## Current Architecture

### Animation Ownership

Animation state is shell/component-owned, not painter-backend-owned.
`crates/core/shell/src/shell/component/animation.rs` applies transitions and
keyframes to `WidgetNode.computed_style` through:

- `AnimatedVisualStyle`
- `StyleAnimation`
- `FrontendSurfaceComponent::apply_style_animations_with_previous`
- `apply_node_style_animation`
- `apply_node_keyframe_animation`

This matches the v1.10 ownership decision: Skia receives ordinary painter
commands after MESH has already resolved style, layout, animation state,
display-list ordering, visual bounds, and damage.

### Existing Dirty Routing

`apply_style_animations_with_previous` currently chooses:

- `ComponentDirtyFlags::VISUAL_REPAINT` for active transitions where no active
  layout-affecting transition was detected.
- `ComponentDirtyFlags::STYLE_RELAYOUT` when active transitions affect layout or
  when keyframe animations are active.

The key gap is not the existence of repaint-vs-relayout flags; it is proving the
classification, narrowing keyframes where safe, and ensuring retained
render-object/display-list/damage inputs update from the current animated style.

### Style And Parser Support

`crates/core/ui/elements/src/style/types.rs` already exposes:

- `TransitionProperties`
- `TransitionStyle`
- `AnimationStyle`
- `AnimationPlayState`
- `Transform2D`
- `BoxShadow`
- `VisualFilter`

`crates/core/ui/elements/src/style/parse.rs` already parses:

- transition longhands and shorthand
- transition properties including opacity, colors, width/height, padding,
  margin, transform, box-shadow, filter, backdrop-filter, font metrics, gap, and
  inset properties
- animation longhands and shorthand

`crates/core/ui/component/src/style.rs` and parser code already validate
keyframe properties through `is_transition_safe_keyframe_property`.

### Retained Rendering Inputs

`crates/core/frontend/render/src/render_object.rs` tracks dirty slots for:

- transform
- clip
- opacity
- geometry
- material
- primitive
- text
- accessibility

`material_hash` and retained display-list paint node data are the bridge for
paint-only animated changes. Phase 56 should add tests proving animated changes
hit the intended dirty slots and avoid geometry/layout changes when geometry is
unchanged.

### Damage Inputs

`crates/core/shell/src/shell/component/shell_component.rs` computes selective
damage from retained paint metrics, render-object dirty node ids, surface
damage, reorder damage, and tooltip damage. The current
`damage_rect_for_widget_node` uses layout bounds only, so animated transforms or
effect overflow can under-damage if the current/previous visual bounds extend
beyond layout.

Phase 55 added visual-bounds proof for effects. Phase 56 should apply the same
principle to animated styles: damage should include the previous and current
animated visual bounds for moving or expanding visuals.

## Implementation Implications

### 1. Inventory And Classification First

The planner should start with an explicit classification artifact in code, not
just comments. Recommended shape:

- Add methods on `TransitionProperties`, or a shell-side helper near animation
  code, that can answer:
  - whether a property set affects layout
  - whether it is paint-only
  - whether it needs layer/effect visual bounds
  - whether it is unsupported/non-interpolable

This should keep existing parser semantics while making the dirty-routing logic
testable.

### 2. Paint-Only Transition Path

For transitions:

- paint-only active animations should keep using `VISUAL_REPAINT`
- layout-affecting active animations should keep using `STYLE_RELAYOUT`
- `transition-property: all` should stay conservative if any active animated
  property is layout-affecting
- transition start/restart/cancellation should preserve current behavior for
  stable `_mesh_key` nodes

Tests should prove both positive and negative cases.

### 3. Keyframes Need Conservative Narrowing

Keyframes currently force relayout when active. That is safe but too broad for
ANIM-02. Phase 56 should narrow this only when keyframe stops can be proven
paint-only.

Recommended approach:

- classify the render keyframe rule stops before setting
  `has_active_keyframe_animation`
- use repaint-only dirty routing for keyframes whose stops touch only paint-only
  fields such as opacity, transform, color, background color, border color,
  border radius, box shadow, and filters
- keep relayout for width/height/gap/font metrics/insets or unknown stop data
- preserve diagnostics for unresolved animation names and invalid animation
  tokens

### 4. Animated Visual Bounds And Damage

Phase 56 should add a helper that computes damage from visual bounds, not just
layout bounds, for dirty animated nodes. The helper should account for:

- transform translation/scale where currently represented
- box-shadow blur/spread/offset overflow
- filter/backdrop-filter blur overflow
- border width/radius where relevant
- previous and current bounds union to clear stale pixels

Broad z-index, clipping, and fallback-promotion redesign should remain Phase
57. Phase 56 should only ensure animation ticks do not leave stale pixels.

### 5. Shipped Navigation/Audio Proof

Navigation-bar shipped styles currently use:

- transition color/background/border-color/border-radius/transform
- transition gap in `main.mesh`
- keyframe `status-pulse` with opacity and transform
- audio popover transition-delay polish tracked from Phase 31

Phase 56 should include focused shell/component tests that prove:

- navigation keyframe animation remains active across rebuilds
- finite keyframes stop render requests
- token animation diagnostics still reach diagnostics
- audio popover show/hide transition timing does not reintroduce first-click,
  same-hover close, or first-drag regressions

## Validation Architecture

### Test Infrastructure

Use the existing Rust test harness.

Recommended quick command:

```bash
nix develop -c cargo test -p mesh-core-shell animation -- --nocapture
```

Recommended focused render command:

```bash
nix develop -c cargo test -p mesh-core-render render_object_tree_marks -- --nocapture
```

Recommended full Phase 56 command:

```bash
nix develop -c cargo test -p mesh-core-shell animation -- --nocapture
nix develop -c cargo test -p mesh-core-shell shipped_navigation -- --nocapture
nix develop -c cargo test -p mesh-core-render render_object_tree_marks -- --nocapture
```

### Required Proof

- transition property bucket tests cover ANIM-01 and ANIM-02
- paint-only transition dirty routing avoids layout when geometry does not
  change
- layout-affecting transition dirty routing still relayouts
- paint-only keyframes can repaint without relayout when stops are paint-only
- animated visual bounds/damage includes previous and current visual bounds
- shipped navigation/audio animation regressions stay within accepted behavior

## Risks And Constraints

- Keyframe narrowing can accidentally skip layout if rule-stop classification is
  incomplete. Keep unknowns on relayout.
- Transform damage can be subtly wrong if only current bounds are damaged. Always
  union previous/current bounds for moving visuals.
- Audio popover transition polish should remain bounded to existing lifecycle
  hooks. A full shell transition model is beyond Phase 56.
- Skia-specific types must not enter style, render-object, or display-list data.

## Research Complete

Phase 56 can be planned as five small slices: inventory/classification,
transition dirty routing, keyframe narrowing/diagnostics, animated
visual-bounds/damage, and shipped-surface/final validation.
