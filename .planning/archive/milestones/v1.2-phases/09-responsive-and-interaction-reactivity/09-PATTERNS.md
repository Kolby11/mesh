---
phase: 09
slug: responsive-and-interaction-reactivity
status: complete
created: 2026-05-05
---

# Phase 09 Pattern Map

## Existing Patterns To Follow

### Shell-owned stable runtime state

- Primary file: `crates/core/shell/src/shell/component.rs`
- Existing state maps (`input_values`, `slider_values`, `checked_values`, `scroll_offsets`) are keyed by stable `_mesh_key` paths. Continue this pattern for interaction-driven restyles because `WidgetNode::id` is regenerated on every rebuild.
- `annotate_runtime_tree` is the right boundary for applying shell runtime state to a freshly built widget tree.

### Render crate performs template evaluation and initial style resolution

- Primary files:
  - `crates/core/ui/render/src/lib.rs`
  - `crates/core/ui/render/src/render.rs`
  - `crates/core/ui/render/src/style.rs`
- `CompiledFrontendPlugin::build_tree_with_state` owns template evaluation, root surface sizing, inherited style context, and first layout pass.
- `render::build_element_node` currently resolves node style with `ElementState::default()`. Runtime pseudo-state is applied later by shell restyling.

### Elements crate owns generic style/layout/event primitives

- Primary files:
  - `crates/core/ui/elements/src/style.rs`
  - `crates/core/ui/elements/src/layout.rs`
  - `crates/core/ui/elements/src/events.rs`
  - `crates/core/ui/elements/src/accessibility.rs`
- `StyleResolver::restyle_subtree` is the existing primitive for applying pseudo selectors and container queries to a tree that already has `WidgetNode.state`.
- `LayoutEngine::compute_with_measurer` is the existing layout recomputation primitive.
- `InputState` is NodeId-based and should remain a lower-level event model/test reference unless stable keys are introduced there.

### Metrics and paint follow layout

- Primary file: `crates/core/shell/src/shell/component.rs`
- `collect_element_metrics` snapshots element bounds and refs from laid-out nodes.
- Paint and `last_tree` assignment happen after layout. Phase 09 should preserve that order and ensure it uses the final post-restyle layout.

## Phase 09 File Ownership

| Area | Files | Plan |
|------|-------|------|
| Runtime state annotation and pseudo-state restyle | `crates/core/shell/src/shell/component.rs`, `crates/core/ui/elements/src/style.rs` | 09-01 |
| Size/container invalidation | `crates/core/shell/src/shell/component.rs`, `crates/core/ui/render/src/lib.rs`, `crates/core/ui/render/src/style.rs` | 09-02 |
| Final layout, hit testing, metrics, accessibility sync | `crates/core/shell/src/shell/component.rs`, `crates/core/ui/elements/src/events.rs`, `crates/core/ui/elements/src/accessibility.rs` | 09-03 |
| State preservation and cleanup | `crates/core/shell/src/shell/component.rs` | 09-04 |

## Tests To Mirror

- Existing container query tests in `crates/core/ui/elements/src/style.rs`.
- Existing render parser/style tests in `crates/core/ui/render/src/lib.rs` and `render.rs`.
- Existing shell component tests in `crates/core/shell/src/shell/component.rs` that build real components and assert runtime behavior.

## Implementation Guidance

- Keep restyle logic deterministic and synchronous inside `build_tree`; avoid background invalidation or async restyle scheduling.
- Add small helper functions only when they make state invalidation or cleanup easier to test.
- Prefer tests that inspect `WidgetNode` style/layout/state snapshots over pixel assertions unless a rendering regression specifically requires paint output.
- Do not introduce a new persistent UI state store unless existing shell maps cannot represent the required behavior.
