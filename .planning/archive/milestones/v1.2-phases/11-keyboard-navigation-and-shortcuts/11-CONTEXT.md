# Phase 11: Keyboard Navigation and Shortcuts - Context

**Gathered:** 2026-05-06
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase makes MESH shell surfaces usable without a mouse through deterministic keyboard focus traversal, visible keyboard-focus indication, keyboard activation for common controls, and explicit shortcut routing that stays within shell-owned capability and settings rules.

This phase covers:
- Tab and Shift+Tab traversal through focusable components in a deterministic rendered order.
- Visible focus behavior that stays close to modern web-engine `:focus-visible` semantics.
- Keyboard activation defaults for buttons, toggles, sliders, inputs, and navigation-bar controls.
- Plugin-defined focused key handlers plus explicit surface-level shortcuts that remain subordinate to shell-global shortcuts.

This phase does not add plugin-global shortcuts beyond the focused surface, broad accessibility-tree export work, richer slider navigation contracts beyond the agreed defaults, or the full navigation-bar migration proof from Phase 13.

</domain>

<decisions>
## Implementation Decisions

### Traversal Order
- **D-01:** Normal Tab traversal follows rendered visual order, not raw template/source order.
- **D-02:** When multiple focusable controls share a visual row, ties break left-to-right and then continue row by row.
- **D-03:** Hidden and disabled controls are skipped during normal Tab traversal.
- **D-04:** Traversal wraps at the ends of the surface rather than stopping on the final control.
- **D-05:** Phase 11 should add a `tabindex`-style focus attribute as an override path, but the default traversal model remains visual-order based.
- **D-06:** The override is exception-oriented rather than the primary ordering model.
- **D-07:** `tabindex="-1"`-style behavior is supported: a control may remain focusable by script or pointer while being skipped by normal Tab traversal.

### Focus Visibility
- **D-08:** MESH should stay as close as practical to current web-engine behavior for `:focus-visible`.
- **D-09:** `:focus` remains the logical focused state, while `:focus-visible` becomes the heuristic "show a visible focus indicator" state rather than a plain alias of `:focus`.
- **D-10:** Script-driven focus that continues a keyboard navigation flow should remain `:focus-visible`.
- **D-11:** Pointer-focused text-entry controls should still become `:focus-visible`, because users need a visible insertion target.
- **D-12:** Plugin authors should use `:focus-visible` for the strong visual ring and `:focus` for general logical focus state.
- **D-13:** A pointer action on a non-text control clears keyboard-style `:focus-visible` for that interaction unless heuristics say it should still show.

### Activation Behavior
- **D-14:** Focused buttons activate with both `Enter` and `Space` by default.
- **D-15:** Default button activation fires on key release.
- **D-16:** Focused switches and checkboxes toggle with `Space`, and `Enter` may also toggle them by default.
- **D-17:** Focused sliders use arrow keys for step adjustment in the default Phase 11 contract.
- **D-18:** The agreed key bindings are default shell behaviors only, not fixed engine constants.
- **D-19:** Shell settings must be able to remap the default activation keys.

### Shortcut Scope
- **D-20:** `onkeydown` and `onkeyup` default to the focused element only.
- **D-21:** Phase 11 should also allow explicit surface-level shortcuts in addition to focused-element key handlers.
- **D-22:** Frontend modules define the default surface-level shortcut bindings in module settings, and core shell settings can override or remap them.
- **D-23:** Shell-global shortcuts always win when they conflict with a surface-level shortcut.
- **D-24:** Surface-level shortcuts are active only when that surface has keyboard focus.

### the agent's Discretion
- Planner/researcher may choose the exact visual-order traversal algorithm, as long as it preserves the locked visual-order default, left-to-right row ordering, and `tabindex` override semantics.
- Planner/researcher may choose the exact runtime representation for keyboard modality / `:focus-visible` state, as long as it stays close to current web-engine behavior.
- Planner/researcher may choose the exact config schema and file plumbing for remappable activation keys and surface-level shortcuts, as long as frontend modules own defaults and shell settings own overrides.
- Planner/researcher may choose the exact proof fixtures and test surfaces, but navigation-bar controls must be covered because `KEY-04` explicitly names them.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Prior Phase Context
- `.planning/PROJECT.md` — v1.2 milestone boundary and the "practical shell UI" renderer goal.
- `.planning/REQUIREMENTS.md` — `KEY-01` through `KEY-04`, plus later-phase boundaries that should not be pulled into Phase 11.
- `.planning/ROADMAP.md` — Phase 11 goal, success criteria, and dependency on Phase 9.
- `.planning/STATE.md` — current milestone position and carried-forward project decisions.
- `.planning/phases/08-practical-css-coverage/08-CONTEXT.md` — Phase 8 styling contract, including `:focus` / `:focus-visible` as supported selector hooks.
- `.planning/phases/09-responsive-and-interaction-reactivity/09-CONTEXT.md` — stable `_mesh_key` runtime state and the Phase 9 decision that `:focus-visible` is only a temporary alias until keyboard modality exists.
- `.planning/phases/10-selectable-text-and-clipboard-copy/10-CONTEXT.md` — `Ctrl+C` selection ownership and the rule that text-selection keyboard behavior must coexist with later keyboard-navigation work.

### Authoring and Settings Contracts
- `docs/frontend/mesh-syntax.md` — author-facing `onkeydown`, `onkeyup`, `onfocus`, and `onblur` event hooks.
- `docs/css-coverage.md` — current documented pseudo-class support, including the temporary `:focus-visible` wording that Phase 11 should reconcile with runtime behavior.
- `modules/frontend/navigation-bar/config/settings.json` — current surface settings, including `keyboard_mode: "none"` that planning must revisit for keyboard-capable proof behavior.
- `modules/frontend/navigation-bar/module.json` — navigation-bar surface configuration schema and keyboard interactivity options exposed to module authors.

### Shell Input and Surface Runtime
- `crates/core/shell/src/shell/component.rs` — `FrontendSurfaceComponent` ownership of `focused_key`, selection state, slider state, and other shell-owned interaction maps.
- `crates/core/shell/src/shell/component/input.rs` — current pointer/key routing, selection copy interception, and the main Phase 11 integration point for traversal, activation, modality, and shortcut dispatch.
- `crates/core/shell/src/shell/layout.rs` — current focusable-node hit-testing and the natural place to add visual-order traversal collection plus `tabindex` handling.
- `crates/core/shell/src/shell/mod.rs` — shell-global shortcut interception and top-level event routing precedence.
- `crates/core/shell/src/shell/types.rs` — `ComponentInput`, `KeyModifiers`, and `CoreRequest` contract used by surface keyboard handling.
- `crates/core/shell/src/shell/surface_layout.rs` — shell-side parsing of per-surface keyboard interactivity settings.
- `crates/core/shell/src/shell/component/rendering.rs` — where parsed surface keyboard settings are applied to the live surface.

### Element, Style, and Accessibility Runtime
- `crates/core/ui/elements/src/style.rs` — pseudo-class matching, including the current `focus-visible => focused` alias that Phase 11 should replace with real modality-aware behavior.
- `crates/core/ui/elements/src/events.rs` — existing focus state transitions and a useful reference for explicit focus transfer semantics.
- `crates/core/ui/elements/src/element.rs` — runtime element field definitions, including exposed `focused` state and any future author-facing focus metadata.
- `crates/core/ui/elements/src/accessibility.rs` — accessibility node model, including `focusable`, `focused`, and `keyboard_shortcut` fields relevant to later proof/testing.
- `crates/core/ui/render/src/render.rs` — current accessibility/focusability tagging for `button`, `input`, and `slider`, which Phase 11 may need to broaden or align with traversal logic.

### Proof Surfaces and Controls
- `modules/frontend/navigation-bar/src/main.mesh` — current top-level proof surface and shortcut candidate host.
- `modules/frontend/navigation-bar/src/components/volume-button.mesh` — concrete example for a focused control plus a future surface shortcut such as `m` for mute.
- `modules/frontend/navigation-bar/src/components/settings-button.mesh` — existing clickable control that should participate in Tab traversal and keyboard activation coverage.
- `modules/frontend/navigation-bar/src/components/theme-button.mesh` — existing clickable control that should participate in Tab traversal and keyboard activation coverage.

### Platform Keyboard Mode
- `crates/core/platform/wayland/src/lib.rs` — `KeyboardMode` contract for shell surfaces and the boundary between shell-level keyboard interactivity and component-level keyboard behavior.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `FrontendSurfaceComponent` already owns stable interaction state such as `focused_key`, selection state, slider state, checked values, and scroll offsets, so Phase 11 can stay shell-owned rather than pushing focus logic into transient widget nodes.
- `ComponentInput::KeyPressed`, `KeyReleased`, and `Char` already exist in the shell input contract, so Phase 11 is primarily about better routing and interpretation rather than introducing a new event channel.
- `layout.rs` already has focusable hit-testing and `_mesh_key`-based lookup helpers that can be expanded into traversal-order collection and focus movement.
- `mesh_core_elements::AccessibilityInfo` already includes a `keyboard_shortcut` field, which gives planning an obvious place to keep future shortcut metadata aligned with accessibility state.
- Navigation-bar already contains several compact button-like controls and is a natural proof surface for traversal, activation, and shortcut behavior.

### Established Patterns
- Shell-owned stable `_mesh_key` state is already the authority for interaction state across rebuilt trees; Phase 11 should not make transient `NodeId` values the canonical focus source.
- Shell-global shortcuts are intercepted before component-level input handling, so any plugin shortcut design must preserve that precedence rather than trying to bypass it.
- Module and shell settings already act as layered configuration surfaces elsewhere in the project, so remappable keyboard bindings should follow that same default-plus-override pattern.
- Current focusability is mostly tag-based (`button`, `input`, `slider`, `switch`, `checkbox`), which gives a simple starting point but means Phase 11 must deliberately define how `tabindex`-style overrides extend or constrain that model.
- Phase 10 already gave `Ctrl+C` to visible text selection when a selection exists, so keyboard activation and shortcut work must not accidentally steal that behavior.

### Integration Points
- Add traversal collection, focus movement, activation dispatch, and shortcut routing in `crates/core/shell/src/shell/component/input.rs`.
- Extend `crates/core/shell/src/shell/layout.rs` with focusable-node ordering logic based on rendered geometry plus `tabindex` override semantics.
- Replace the temporary `focus-visible` alias in `crates/core/ui/elements/src/style.rs` with explicit modality-aware runtime state.
- Use `crates/core/shell/src/shell/surface_layout.rs`, module settings, and shell settings plumbing to define default/remapped keyboard bindings and surface shortcut configuration.
- Update navigation-bar module settings and/or component fixtures so Phase 11 tests can prove real surface behavior, not only isolated engine behavior.

</code_context>

<specifics>
## Specific Ideas

- Stay as close as practical to contemporary web-engine behavior for `:focus-visible` and `tabindex` semantics rather than inventing a shell-specific mental model.
- The `tabindex`-style attribute is an override tool, not the primary traversal model.
- Keyboard bindings should be treated as defaults that users can remap through shell settings rather than fixed engine behavior.
- A concrete Phase 11 shortcut example is the volume widget defining `m` as a mute shortcut at the surface level, with module-owned defaults and shell-owned overrides.

</specifics>

<deferred>
## Deferred Ideas

- Plugin-global shortcuts beyond the focused surface are deferred; Phase 11 only locks focused-element handlers plus explicit focused-surface shortcuts.
- Richer slider keyboard behavior such as `Home`, `End`, `PageUp`, and `PageDown` is deferred beyond the first locked Phase 11 default contract.
- More complex shortcut precedence policies beyond "shell-global wins" are deferred.

</deferred>

---

*Phase: 11-Keyboard Navigation and Shortcuts*
*Context gathered: 2026-05-06*
