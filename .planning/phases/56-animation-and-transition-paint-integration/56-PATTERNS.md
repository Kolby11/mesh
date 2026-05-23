# Phase 56 Pattern Map

## Purpose

Closest existing code analogs for Phase 56 planning and execution.

## Files And Analogs

| Target Area | Closest Existing Analog | Pattern To Reuse |
|-------------|-------------------------|------------------|
| Animation property classification | `crates/core/ui/elements/src/style/types.rs` `TransitionProperties` methods | Add small predicate methods with explicit names and unit tests. Keep classification data close to property flags. |
| Transition dirty routing | `crates/core/shell/src/shell/component/animation.rs` `apply_style_animations_with_previous` | Route through `ComponentDirtyFlags::VISUAL_REPAINT` vs `STYLE_RELAYOUT` after computing active animation categories. |
| Keyframe lifecycle | `crates/core/shell/src/shell/component/animation.rs` `apply_node_keyframe_animation` | Preserve existing `ActiveKeyframeAnimation` lifecycle, pause/fill/restart behavior, and diagnostics; add classification around it rather than replacing it. |
| Retained render-object proof | `crates/core/frontend/render/src/render_object.rs` tests named `render_object_tree_marks_*` | Assert exact dirty slot counts and dirty node ids for transform, opacity, material, geometry, and primitive changes. |
| Selective damage proof | `crates/core/shell/src/shell/component/shell_component.rs` tests around `select_damage_policy`, `damage_rect_for_node_ids`, and `merge_optional_damage` | Add focused unit tests for damage rectangles and previous/current visual bounds before integration proof. |
| Shipped animation proof | `crates/core/shell/src/shell/component/tests/interaction/animation.rs` and `tests/integration/real_surfaces.rs` | Use shipped navigation/audio fixtures and assert dirty flags, render requests, diagnostics, and no first-input regressions. |

## Concrete Excerpts To Respect

### Dirty Routing

`apply_style_animations_with_previous` currently does:

```rust
let flags = if has_layout_affecting_animation || has_active_keyframe_animation {
    ComponentDirtyFlags::STYLE_RELAYOUT
} else {
    ComponentDirtyFlags::VISUAL_REPAINT
};
self.invalidate_style_path(flags);
```

Phase 56 should keep this shape but replace the coarse keyframe boolean with a
classification-aware decision.

### Retained Dirty Slots

`RenderObjectDirtySummary` already has distinct fields:

```rust
pub transform: usize,
pub opacity: usize,
pub geometry: usize,
pub material: usize,
pub primitive: usize,
pub text: usize,
```

Phase 56 tests should assert these counts directly rather than only checking
that “something is dirty.”

### Damage Rect Baseline

`damage_rect_for_widget_node` currently derives damage from layout bounds:

```rust
let left = node.layout.x.floor().max(0.0) as u32;
let top = node.layout.y.floor().max(0.0) as u32;
let right = (node.layout.x + node.layout.width).ceil().max(0.0) as u32;
let bottom = (node.layout.y + node.layout.height).ceil().max(0.0) as u32;
```

Phase 56 should add visual-bound expansion for animated transforms/effects and
union previous/current bounds for moving visuals.

## Test Naming Pattern

Use behavior-first names:

- `animation_property_bucket_classifies_paint_only_properties`
- `animation_transition_dirty_uses_visual_repaint_for_paint_only_changes`
- `animation_transition_dirty_uses_relayout_for_geometry_changes`
- `keyframe_animation_paint_only_rule_uses_visual_repaint`
- `animation_damage_unions_previous_and_current_transform_bounds`
- `shipped_navigation_animation_proof_keeps_audio_popover_input_stable`

## Notes

Do not introduce a new animation framework. Extend the existing shell component
animation pipeline and retain the Skia-free renderer ownership boundary.
