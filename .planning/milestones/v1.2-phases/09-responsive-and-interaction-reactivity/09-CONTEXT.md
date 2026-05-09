# Phase 9: Responsive and Interaction Reactivity - Context

**Gathered:** 2026-05-05
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase makes already-rendered frontend components restyle and relayout when their surface/container size or interaction state changes. It covers container-query re-evaluation, pseudo-state restyling for hover/focus/active/disabled/checked/focus-visible, synchronization between layout, hit testing, accessibility bounds, and paint, and preservation of runtime state during restyles.

This phase does not add new CSS properties, text selection, keyboard traversal, custom key handlers, animation tokens, custom keyframe scheduling, or the full navigation-bar proof. Those remain Phase 10 through Phase 13 work.

</domain>

<decisions>
## Implementation Decisions

These decisions were captured using the Codex fallback path because interactive question UI was unavailable in Default mode. They are conservative builder defaults based on Phase 8 decisions and current code, and should be reviewed before planning if the user wants different behavior.

### Restyle Trigger Granularity
- **D-01:** Treat surface size changes and child container-size changes as first-class style invalidation triggers. Container queries must be re-evaluated with the current container width and height before layout and paint.
- **D-02:** Avoid full plugin reload for size-driven or state-driven restyles. Rebuild or re-resolve the widget tree as needed, but keep the existing frontend `ScriptContext`, service state, input values, slider state, scroll offsets, focused node identity, and embedded component runtime state.
- **D-03:** Prefer explicit invalidation at the component boundary over hidden global polling. The planner should identify where `FrontendSurfaceComponent::paint`, `build_tree`, and surface-size dispatch can compare prior/current dimensions and mark the component dirty.
- **D-04:** A restyle that changes layout-affecting fields must be followed by layout recomputation before hit testing, accessibility metric publishing, and paint. Visual-only state changes may still use the same safe pipeline unless planning finds a contained optimization.

### Interaction State Authority
- **D-05:** The source of truth for interaction state should be the shell component's stable `_mesh_key` / path-based runtime state, not transient `NodeId`, because `WidgetNode` trees are rebuilt and node IDs are not stable across frames.
- **D-06:** `mesh-core-elements::InputState` is useful as a lower-level reference/test utility, but Phase 9 should not blindly make `NodeId`-based state authoritative for frontend surfaces unless stable IDs are introduced.
- **D-07:** Hover, focus, active, disabled, checked, and focus-visible should map into `ElementState` before style resolution so existing selector matching in `StyleResolver` remains the single pseudo-state style path.
- **D-08:** Pointer hover should update only when the hit path/key changes. Focus should persist until an explicit focus transfer, surface hide/reset, or later keyboard traversal behavior changes it in Phase 11.

### State Preservation Contract
- **D-09:** Restyles must preserve user-entered input values, slider values, checkbox/switch checked values, scroll offsets, hover/focus/active identities where still valid, service payloads, settings, locale/theme state, and imported component runtime state.
- **D-10:** If a restyle removes or hides the previously hovered/focused/active node, the corresponding state should be cleared deterministically and any generated blur/leave behavior should follow existing event semantics.
- **D-11:** Restyle should update `refs` and `elements` metrics after layout so scripts read fresh bounds on the next render tick. Metrics must not describe stale pre-restyle layout.
- **D-12:** State-driven style transitions should use existing transition metadata and style animation plumbing only where it already applies; custom keyframe scheduling remains Phase 12.

### Proof Surfaces and Test Shape
- **D-13:** Phase 9 should include focused engine-level tests for container query changes, pseudo-state style changes, hit-test synchronization, accessibility/metric synchronization, and state preservation.
- **D-14:** Add at least one real component/surface regression in `FrontendSurfaceComponent` tests, because the important risk is losing runtime state during rebuild/restyle cycles.
- **D-15:** Navigation-bar can be referenced as a future proof target but should not be fully migrated or used as the main Phase 9 proof. Phase 13 owns the comprehensive navigation-bar behavior.

### the agent's Discretion
- Planner/researcher may choose the exact invalidation API and data structures, as long as stable runtime keys remain the interaction-state authority for rebuilt trees.
- Planner/researcher may decide whether to re-resolve styles in place, rebuild trees with injected state, or combine both paths, as long as user/runtime state is preserved and metrics/hit testing stay synchronized.
- Planner/researcher may choose the minimal real-surface fixture for state preservation tests.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Phase Scope
- `.planning/PROJECT.md` — v1.2 rendering goals and practical shell UI boundary.
- `.planning/REQUIREMENTS.md` — REACT-01 through REACT-04 requirements and later-phase boundaries.
- `.planning/ROADMAP.md` — Phase 9 goal, success criteria, and dependencies.
- `.planning/STATE.md` — Current milestone state and completed Phase 8 context.
- `.planning/phases/08-practical-css-coverage/08-CONTEXT.md` — Phase 8 CSS contract and explicit deferral of live restyling to Phase 9.
- `.planning/phases/08-practical-css-coverage/08-05-SUMMARY.md` — LSP/docs/nav focused proof handoff and Phase 13 boundary.

### Codebase Maps
- `.planning/codebase/STRUCTURE.md` — UI/render/component crate locations and where reactivity work belongs.
- `.planning/codebase/STACK.md` — Rust, Wayland, `lightningcss`, `cosmic-text`, and test infrastructure.
- `.planning/codebase/ARCHITECTURE.md` — Shell/component/render/style layering and data flow.

### Reactivity and State Code
- `crates/core/shell/src/shell/component.rs` — `FrontendSurfaceComponent` paint/input flow, dirty rendering, runtime state stores, hover/focus keys, scroll offsets, metrics publishing, style animation plumbing.
- `crates/core/shell/src/shell/mod.rs` — Wayland event dispatch, surface size lookup, paint/present loop, and component routing.
- `crates/core/shell/src/shell/types.rs` — `ShellComponent`, `ComponentInput`, and component lifecycle contract.
- `crates/core/ui/render/src/lib.rs` — `build_tree_with_state` and root style context from surface dimensions.
- `crates/core/ui/render/src/render.rs` — template-to-`WidgetNode` construction and per-node style context propagation.
- `crates/core/ui/render/src/style.rs` — child style context, container query filtering, default/inherited style helpers.
- `crates/core/ui/elements/src/style.rs` — `StyleResolver`, `StyleContext`, selector state matching, and `restyle_subtree`.
- `crates/core/ui/elements/src/events.rs` — `InputState`, hit testing, scroll offsets, and `ElementState` mutation reference behavior.
- `crates/core/ui/elements/src/tree.rs` — `WidgetNode`, `NodeId`, and `ElementState`.
- `crates/core/ui/elements/src/accessibility.rs` — accessibility tree extraction from laid-out widget trees.
- `crates/core/ui/elements/src/layout.rs` — layout computation that must run before hit testing, accessibility metrics, and paint.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `StyleResolver::resolve_node_style` already accepts `StyleContext` and `ElementState`, so pseudo-state and container-query reactivity can reuse the existing resolver path.
- `StyleResolver::restyle_subtree` exists as an in-place state-rule overlay, but it is `NodeId`/tree-instance oriented and only overlays pseudo-state rules. Planner should verify whether it is enough or whether rebuilt-tree state injection is cleaner.
- `FrontendSurfaceComponent` already tracks stable runtime keys such as `hovered_key`, `hovered_path`, `focused_key`, `pointer_down_key`, `active_slider_key`, `input_values`, `scroll_offsets`, and `checked_values`.
- `FrontendPlugin::build_tree_with_state` already receives current width and height and produces a root `StyleContext`, making surface-size query invalidation a natural component paint/build concern.
- `publish_element_metrics` already publishes `elements` and `refs` after tree build/layout; Phase 9 should ensure this happens after every restyle/relayout path.

### Established Patterns
- Frontend trees are rebuilt from component state rather than maintained as a persistent DOM. Any persistent interaction state must be keyed independently of transient `WidgetNode::id`.
- Shell-level event dispatch routes Wayland events into `ComponentInput`, and `FrontendSurfaceComponent::handle_input` currently owns higher-level click, hover, focus, input, slider, checkbox, and scroll behavior.
- Phase 8 locked parser/resolver/render boundaries: parser in `mesh-core-component`, computed style/value resolution in `mesh-core-elements`, rendering in `mesh-core-render`, orchestration in `mesh-core-shell`.
- Runtime state changes should mark a component dirty and let the normal paint path rebuild, layout, publish metrics, and paint.

### Integration Points
- Surface size changes can be detected around `Shell::render_components` / component `paint` calls where actual Wayland or configured surface dimensions are known.
- Container query context is established in `build_tree_with_state` and propagated by `render::build_widget_node`; this is where state/context injection likely belongs.
- Pointer and key input flow through `FrontendSurfaceComponent::handle_input`; pseudo-state updates should integrate there without introducing a parallel event authority.
- Hit testing uses the last painted tree and scroll offsets, so layout and `last_tree` must be refreshed before new hit-test-sensitive interactions rely on changed styles.
- Accessibility and script metrics are generated from the current tree; they are the synchronization proof points for REACT-03.

</code_context>

<specifics>
## Specific Ideas

- Use stable `_mesh_key` or equivalent runtime keys for state persistence across rebuilt trees.
- Favor a single conservative rebuild/restyle path first; optimize visual-only restyles later if necessary.
- Tests should prove that hover/focus/container-query changes update style, layout, hit testing, metrics, and paint without clearing input/scroll/service state.

</specifics>

<deferred>
## Deferred Ideas

- Mouse-driven text selection and clipboard copy remain Phase 10.
- Tab traversal, keyboard shortcut definition, and broader keyboard activation behavior remain Phase 11.
- Theme animation tokens, custom keyframes, and animation scheduling remain Phase 12.
- Full navigation-bar migration/proof remains Phase 13.
- Full browser CSS compatibility, CSS Grid, floats, rich text editing, and GPU transform/filter animation remain out of scope for this milestone.

</deferred>

---

*Phase: 9-Responsive and Interaction Reactivity*
*Context gathered: 2026-05-05*
