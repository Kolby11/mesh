# Phase 50 Research: AccessKit Runtime And Broad Adoption Gates

## Sources Read

- `accesskit 0.24.0` local crate source in Cargo registry
- `crates/core/frontend/render/src/proof.rs`
- `crates/core/frontend/render/src/library_adapters.rs`
- `crates/core/ui/elements/src/accessibility.rs`
- `crates/core/frontend/compiler/src/render.rs`
- `crates/core/ui/interaction/src/focus.rs`
- `docs/renderer-migration.md`
- `docs/renderer-ownership.md`
- `docs/frontend/renderer-contract.md`

## Relevant AccessKit API

- `accesskit::NodeId(pub u64)` maps directly from MESH `NodeId = u64`.
- `accesskit::Node::new(Role)` constructs nodes.
- Useful node setters: `set_label`, `set_description`, `set_value`, `set_children`, `set_bounds`, `set_disabled`, `set_toggled`, `set_selected`, `set_numeric_value`, `set_min_numeric_value`, `set_max_numeric_value`, and `add_action`.
- `accesskit::Tree::new(root_id)` plus `accesskit::TreeUpdate { nodes, tree: Some(tree), focus, ..Default::default() }` is the feature-enabled runtime update shape.
- `accesskit::Rect` is re-exported by AccessKit, so no direct geometry dependency is needed.

## Existing MESH Inputs

- `WidgetNode.accessibility` already carries role, label, description, focusable, focused, state, and keyboard shortcut.
- Compiler tag mapping already marks button/input/slider/checkbox/switch roles and focusability.
- `FocusedProofSnapshot` currently has string-oriented `FocusedAccessKitUpdate`; Phase 50 should keep it as compatibility evidence and add real AccessKit conversion under `renderer-accesskit`.

## Implementation Direction

- Add `crates/core/frontend/render/src/accesskit_adapter.rs`, gated by `renderer-accesskit`.
- Convert retained `WidgetNode` trees directly to AccessKit `TreeUpdate` so parent/child relationships come from retained node children.
- Add `build_accesskit_runtime_update(root: &WidgetNode) -> accesskit::TreeUpdate` as a feature-gated public render helper.
- Keep default builds unchanged.
- Update docs to classify AccessKit runtime updates as the production accessibility adapter boundary, with platform publication deferred.

## Pitfalls

- Do not build real AccessKit updates from `FocusedAccessibilityEvidence` alone; it lacks child relationships unless expanded.
- Do not imply screen-reader/platform publication is complete; this phase is retained-node runtime update construction only.
- Keep role mapping explicit and conservative.
