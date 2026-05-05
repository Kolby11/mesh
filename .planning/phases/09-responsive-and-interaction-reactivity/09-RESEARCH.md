---
phase: 09
slug: responsive-and-interaction-reactivity
status: complete
created: 2026-05-05
---

# Phase 09 Research - Responsive and Interaction Reactivity

## Goal

Make rendered components react to size and interaction changes without full plugin reloads and without losing runtime state.

## Findings

### Current render path

- `FrontendSurfaceComponent::paint` calls `build_tree`, applies style animations, measures content, publishes element metrics, paints, stores `last_tree`, and clears dirty/runtime change flags.
- `FrontendSurfaceComponent::build_tree` calls `CompiledFrontendPlugin::build_tree_with_state(theme, width, height, ...)`, then annotates stable runtime keys and state via `annotate_runtime_tree`, initializes overflow attributes, and calls `StyleResolver::restyle_subtree`.
- `CompiledFrontendPlugin::build_tree_with_state` creates a root surface style from the current width/height, resolves template styles with container contexts, and immediately computes layout with `LayoutEngine::compute_with_measurer`.
- `StyleResolver::restyle_subtree` can re-resolve styles for `WidgetNode.state` pseudo selectors and container queries, but layout must be recomputed after any restyle that changes layout-affecting properties.

### Current interaction state model

- `mesh-core-elements::events::InputState` tracks pseudo state using transient `NodeId`. It is useful as a low-level test/reference utility, but Phase 09 should continue the shell runtime model because rebuilt trees get new `NodeId` values.
- `FrontendSurfaceComponent` already tracks stable state with `_mesh_key` paths:
  - `hovered_key` / `hovered_path`
  - `focused_key`
  - `pointer_down_key`
  - `input_values`
  - `slider_values`
  - `checked_values`
  - `scroll_offsets`
- `annotate_runtime_tree` sets `ElementState` and updates runtime attributes for inputs, sliders, switches, checkboxes, focus, and scroll offsets.

### Current size/container model

- Container query filtering exists in both render style helpers and element style resolver.
- The top-level surface dimensions are passed into `build_tree_with_state`.
- Restyling after runtime annotation currently uses a single `StyleContext` rooted at the surface size. Children inherit updated contexts during recursive restyle.
- The missing guarantee is invalidation: a surface/container size change must force rebuild/restyle/layout even when no plugin state changed.

### Synchronization risks

- Restyling after the first layout can change dimensions, flex behavior, overflow, visibility, or spacing. Hit testing, accessibility bounds, published refs/elements metrics, and paint must all use a tree whose layout was recomputed after the final style pass.
- `handle_input` falls back to building a fresh tree when `last_tree` is absent, and otherwise uses `last_tree` for hit testing. If size or state changes produce a stale `last_tree`, hit testing can target old bounds.
- Focus, hover, active, and checked states must be keyed by `_mesh_key`, not by `NodeId`.

## Recommended Implementation Shape

1. Make `build_tree` produce a final, synchronized tree:
   - Build template tree with current width/height.
   - Annotate stable runtime keys and interaction/value state.
   - Restyle with pseudo state and current container context.
   - Recompute layout after restyle with the same text measurer and surface dimensions.
   - Annotate overflow after the final layout if scroll ranges depend on layout.
2. Track last rendered surface size in `FrontendSurfaceComponent`.
   - A size delta marks the component dirty.
   - Paint must rebuild when dirty, animated, or size changed.
3. Keep interaction state invalidation narrow.
   - Hover changes mark dirty only when stable hover key/path changes.
   - Focus and active changes mark dirty immediately.
   - Checked/disabled/value changes mark dirty before paint.
4. Preserve runtime state maps across restyles and clear only invalid targets after a final tree proves a key no longer exists.
5. Add focused tests at the renderer/style unit layer and shell component regression layer.

## Validation Architecture

Use Rust unit and integration-style tests through Cargo. Prefer targeted package tests while developing and full workspace tests before final verification.

- Quick command: `nix develop -c cargo test -p mesh-core-elements -p mesh-core-render -p mesh-core-shell responsive interaction restyle container`
- Full command: `nix develop -c cargo test`
- Primary coverage:
  - Container query restyle changes layout/render output on size changes without recreating plugin/runtime state.
  - Hover, focus, active, checked, disabled, and focus-visible states apply pseudo styles through stable keys.
  - Final post-restyle layout drives hit testing, accessibility bounds, refs/elements metrics, and paint.
  - Inputs, sliders, checked state, scroll offsets, service state, settings, locale/theme, and embedded runtime state survive size/state restyles.
- Sampling rule: every plan task that changes runtime behavior must add or update an automated test in the same task or in its plan's first task.

## Open Risks

- Disabled state currently appears attribute-derived rather than runtime-derived. The implementation should define the source of truth before applying `ElementState.disabled`.
- Focus-visible is currently equivalent to focused in selector matching. Phase 09 can preserve that behavior unless keyboard modality state already exists locally.
- Imported/embedded components have their own runtimes and instance keys. Any key cleanup must not delete runtime entries just because a host restyle temporarily changes layout.
