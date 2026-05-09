# Phase 10: Selectable Text and Clipboard Copy - Context

**Gathered:** 2026-05-06
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase adds mouse-driven text selection, visible selection highlighting, and standard copy-shortcut behavior for rendered frontend text in MESH.

The Phase 10 implementation is intentionally narrow for the first shipped version:
- Selection is explicit opt-in on text content rather than a shell-wide default.
- Selection applies only to selectable text nodes, not to interactive controls or general shell chrome.
- Selection stays within a single selectable text node, while still supporting natural wrapped-line selection inside that node.
- Clipped or ellipsized text is not selectable in Phase 10.

This is narrower than the current `TEXT-04` wording in `REQUIREMENTS.md` / `ROADMAP.md`, which still says selection should work with clipped text and nested component trees. Planning should preserve the user's narrowed product decision and reconcile the planning docs rather than silently broadening implementation scope.

</domain>

<decisions>
## Implementation Decisions

### Selectable Scope
- **D-01:** Phase 10 selection is explicit opt-in only. Text must be marked selectable to participate.
- **D-02:** Text inside interactive controls such as buttons, switches, and sliders is not selectable in Phase 10, even if those controls contain visible copy.
- **D-03:** Non-text elements do not participate in the selection path. Phase 10 selection walks selectable text nodes only.
- **D-04:** The first shipped proof should use a small non-interactive fixture or read-only text block, not navigation-bar chrome or other clickable shell surfaces.

### Selection Boundaries
- **D-05:** A drag-selection may span only a single selectable text node in Phase 10.
- **D-06:** Wrapped text inside that node should select naturally across visual lines.
- **D-07:** Clipped or ellipsized text is not selectable in Phase 10. Do not copy hidden/truncated underlying text.
- **D-08:** If the drag leaves selectable text and enters non-selectable space, clamp the selection to the last valid selectable character instead of canceling the selection.

### Highlight Styling
- **D-09:** Phase 10 should add dedicated theme tokens for selection foreground and background colors.
- **D-10:** Selected text fully overrides the node's normal foreground and background styling while the selection is active.
- **D-11:** Selection styling remains shell/theme-owned in Phase 10. Component authors do not get per-component selection color overrides yet.
- **D-12:** Selection visibility should follow theme contrast rules rather than hardcoded “always strong” or “always subtle” engine defaults.

### Copy Ownership
- **D-13:** The standard copy shortcut copies selected Phase 10 text only when a Phase 10 text selection exists; otherwise normal focused-control behavior wins.
- **D-14:** After a successful copy, keep the selection visible.
- **D-15:** Clear selection on explicit click elsewhere, keyboard input aimed at another control, or any surface hide/rebuild path that removes the selected node.
- **D-16:** Phase 10 does not include copy-on-select, X11 primary-selection, or middle-click paste behavior.

### Requirement Reconciliation
- **D-17:** The user's accepted Phase 10 scope is narrower than current `TEXT-04` planning language. Downstream planning should either update the planning docs to match this narrowed first release or explicitly split the broader clipped/nested selection behavior into later work.

### the agent's Discretion
- Planner/researcher may choose the exact runtime representation for selection anchors/ranges, as long as it preserves the single-text-node boundary.
- Planner/researcher may choose the exact hit-testing and glyph-indexing strategy for wrapped text selection.
- Planner/researcher may choose the exact clipboard plumbing path, as long as it is triggered only by the standard copy shortcut and respects normal focused-control behavior when no text selection exists.
- Planner/researcher may choose the minimal proof surface and test fixture for the first shipped selectable text behavior.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Phase Scope
- `.planning/PROJECT.md` — v1.2 rendering goals and the “practical shell UI” boundary.
- `.planning/REQUIREMENTS.md` — `TEXT-01` through `TEXT-04`, plus the current wording that needs reconciliation with the narrowed Phase 10 decision.
- `.planning/ROADMAP.md` — Phase 10 goal, success criteria, and dependency on Phase 9.
- `.planning/STATE.md` — current milestone position and carried-forward project decisions.
- `.planning/phases/08-practical-css-coverage/08-CONTEXT.md` — practical CSS boundary and explicit deferral of selection work to Phase 10.
- `.planning/phases/09-responsive-and-interaction-reactivity/09-CONTEXT.md` — stable runtime-key, restyle, and interaction-state decisions that selection must respect.

### Frontend Runtime and Input Flow
- `crates/core/shell/src/shell/component.rs` — `FrontendSurfaceComponent` state ownership and the main surface runtime.
- `crates/core/shell/src/shell/component/input.rs` — pointer, key, focus, slider, and control-input routing that selection must coexist with.
- `crates/core/shell/src/shell/component/rendering.rs` — tree build, post-restyle layout, and paint flow used by selectable text rendering.
- `crates/core/shell/src/shell/component/runtime_tree.rs` — stable `_mesh_key` annotation and runtime-state injection across rebuilt trees.
- `crates/core/shell/src/shell/layout.rs` — hit-testing, node-path lookup, and overflow-aware bounds traversal used by pointer interactions.
- `crates/core/shell/src/shell/types.rs` — `ComponentInput` and shell component event contract.

### Text Tree, Layout, and Paint
- `crates/core/ui/render/src/render.rs` — how template text/expression nodes become `WidgetNode` text nodes with `content`.
- `crates/core/ui/elements/src/element.rs` — existing `text.selectable` attribute definition.
- `crates/core/ui/elements/src/tree.rs` — `WidgetNode` structure and per-node runtime state.
- `crates/core/ui/elements/src/layout.rs` — text measurement and wrapped layout behavior.
- `crates/core/ui/render/src/surface/text.rs` — `cosmic-text` measurement/rendering path that planning will likely need for selection range/glyph mapping.
- `crates/core/ui/render/src/surface/painter.rs` — current text painting path where selection highlight rendering will integrate.

### Theme and Clipboard Boundaries
- `config/themes/mesh-default-dark.json` — current theme tokens; no selection-specific tokens exist yet.
- `crates/core/foundation/capability/src/lib.rs` — existing `shell.clipboard.write` capability classification, relevant if clipboard access is exposed through shell-owned plumbing.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `text.selectable` already exists as an element field in `crates/core/ui/elements/src/element.rs`, so Phase 10 can reuse an existing author-facing attribute instead of inventing a new selection flag.
- `FrontendSurfaceComponent` already owns stable runtime state such as focus, pointer-down, hover path, input values, checked values, and scroll offsets. Selection state can live alongside these shell-owned interaction states.
- `runtime_tree::annotate_runtime_tree` already stamps stable `_mesh_key` paths across rebuilt trees, which gives planning a stable identity mechanism when selection must survive repaints or be cleared deterministically if a node disappears.
- `TextRenderer` in `crates/core/ui/render/src/surface/text.rs` already uses `cosmic-text`, which should give planning a practical path for wrapped-line measurement and selection range math without inventing a second text stack.

### Established Patterns
- Frontend trees are rebuilt from reactive/script state rather than maintained as a persistent DOM. Selection logic cannot depend on transient `NodeId` stability alone.
- Pointer input currently prioritizes focus, click, slider, checkbox, scroll, and hover behaviors in `component/input.rs`. Phase 10 must integrate without regressing existing control interactions.
- Layout is recomputed after restyle and before paint/hit testing. Selection highlight rendering and hit-testing should follow that same authoritative post-layout tree.
- Theme-owned visual contracts are already the norm for shell-level styling. Selection visuals should follow that model rather than introducing per-component author styling immediately.

### Integration Points
- Add selection state ownership to `FrontendSurfaceComponent` near the existing focus/hover/pointer state.
- Extend pointer handling in `crates/core/shell/src/shell/component/input.rs` to support drag-selection only for selectable text and only when not interacting with controls.
- Use the built tree plus post-layout geometry to map pointer drag positions into character ranges for a single text node.
- Integrate highlight rendering into `crates/core/ui/render/src/surface/painter.rs` / `text.rs` so selected ranges paint with selection theme tokens.
- Route standard copy-shortcut handling through the shell component input flow and whichever shell-owned clipboard path planning chooses.

</code_context>

<specifics>
## Specific Ideas

- Follow the precedent of mainstream desktop shell chrome: panel/taskbar/top-bar surfaces are mostly action/status UI, not broad copyable text surfaces.
- Start with a small non-interactive proof fixture or passive read-only text block rather than trying to prove Phase 10 on navigation-bar buttons or other interactive shell chrome.
- Preserve the user's narrower first-release preference even though the current planning docs still describe broader clipped/nested selection behavior.

</specifics>

<deferred>
## Deferred Ideas

- Cross-node selection across nested component trees is deferred beyond this narrowed Phase 10 scope.
- Selection on clipped or ellipsized text is deferred beyond this narrowed Phase 10 scope.
- Copy-on-select, X11 primary-selection, and middle-click paste behaviors are deferred.
- Navigation-bar proof remains a later milestone concern; Phase 13 owns the comprehensive navigation-bar proof surface.
- Keyboard-driven selection ranges and broader keyboard navigation remain Phase 11 work, not Phase 10.

</deferred>

---

*Phase: 10-Selectable Text and Clipboard Copy*
*Context gathered: 2026-05-06*
