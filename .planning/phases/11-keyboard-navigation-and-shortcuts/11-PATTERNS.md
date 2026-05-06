---
phase: 11
slug: keyboard-navigation-and-shortcuts
status: complete
created: 2026-05-06
---

# Phase 11 Pattern Map

## Existing Patterns To Follow

### Shell-owned stable interaction state

- Primary files:
  - `crates/core/shell/src/shell/component.rs`
  - `crates/core/shell/src/shell/component/runtime_tree.rs`
  - `crates/core/shell/src/shell/component/interaction_state.rs`
- Focus, hover, active-slider, selection, and value state already live on `FrontendSurfaceComponent` and are re-applied to rebuilt trees via stable `_mesh_key` values.
- Phase 11 keyboard traversal, keyboard modality, and focused-surface shortcut state should follow this pattern instead of storing focus in transient `WidgetNode::id` values.

### Global shortcut precedence is already centralized

- Primary files:
  - `crates/core/shell/src/shell/mod.rs`
  - `crates/core/shell/src/shell/types.rs`
- Shell-global shortcuts are intercepted before component routing, and key modifiers already arrive with `ComponentInput::KeyPressed`.
- Phase 11 shortcut routing should extend this existing gate rather than introducing a parallel shortcut system inside frontend scripts.

### Post-layout tree is the authoritative interaction surface

- Primary files:
  - `crates/core/shell/src/shell/layout.rs`
  - `crates/core/shell/src/shell/component/input.rs`
  - `crates/core/shell/src/shell/component/rendering.rs`
- Pointer hit testing, hover updates, slider dragging, scroll targeting, and selection behavior already consult the final laid-out tree.
- Visual-order traversal and `tabindex` handling should be derived from that same final tree so focus order, bounds, accessibility, and paint stay consistent.

### Pseudo-state styling flows through `ElementState`

- Primary files:
  - `crates/core/ui/elements/src/tree.rs`
  - `crates/core/ui/elements/src/style.rs`
  - `crates/core/shell/src/shell/component/runtime_tree.rs`
- State-based CSS already runs through `ElementState` plus selector matching.
- Real `:focus-visible` support should therefore extend `ElementState` and runtime annotation rather than creating a special-case styling path in paint code.

### Settings layering already exists for shell-wide and module-owned values

- Primary files:
  - `crates/core/foundation/config/src/lib.rs`
  - `config/settings-default.json`
  - `crates/core/shell/src/shell/surface_layout.rs`
  - `modules/frontend/navigation-bar/module.json`
  - `modules/frontend/navigation-bar/config/settings.json`
- Shell settings already merge defaults and user overrides, while module surface settings already flow from manifest defaults plus `config/settings.json`.
- Phase 11 should use this split directly:
  - module-owned defaults for surface shortcuts
  - shell-owned overrides for remapping and global policy

### Proof surfaces already exist as real modules

- Primary files:
  - `modules/frontend/navigation-bar/src/main.mesh`
  - `modules/frontend/navigation-bar/src/components/*.mesh`
  - `modules/frontend/audio-popover/src/main.mesh`
  - `config/package.json`
- Navigation-bar already proves shell button-like controls, and audio popover already proves a real slider plus action buttons.
- Phase 11 should reuse those shipped modules instead of inventing a synthetic keyboard-only demo surface for the milestone proof.

## Phase 11 File Ownership

| Area | Files | Plan |
|------|-------|------|
| Focus-visible runtime state, focusable accessibility parity, traversal helpers, Tab/Shift+Tab lifecycle | `crates/core/ui/elements/src/tree.rs`, `crates/core/ui/elements/src/style.rs`, `crates/core/ui/render/src/render.rs`, `crates/core/shell/src/shell/layout.rs`, `crates/core/shell/src/shell/component.rs`, `crates/core/shell/src/shell/component/runtime_tree.rs`, `crates/core/shell/src/shell/component/interaction_state.rs`, `crates/core/shell/src/shell/component/input.rs`, `crates/core/shell/src/shell/component/tests.rs` | 11-01 |
| Focused keydown/keyup dispatch and default control activation | `crates/core/shell/src/shell/types.rs`, `crates/core/shell/src/shell/component/input.rs`, `crates/core/shell/src/shell/component/tests.rs`, supporting shell/runtime files touched by keyboard event payload shaping | 11-02 |
| Shell keyboard settings, surface shortcut merge rules, shortcut routing, accessibility shortcut metadata | `crates/core/foundation/config/src/lib.rs`, `config/settings-default.json`, `crates/core/shell/src/shell/mod.rs`, `crates/core/shell/src/shell/component.rs`, `crates/core/shell/src/shell/component/input.rs`, `crates/core/shell/src/shell/component/tests.rs`, `modules/frontend/navigation-bar/module.json`, `modules/frontend/navigation-bar/config/settings.json`, `modules/frontend/navigation-bar/src/components/volume-button.mesh` | 11-03 |
| Navigation-bar/audio-popover proof behavior, docs, and end-to-end regression coverage | `modules/frontend/navigation-bar/config/settings.json`, `modules/frontend/navigation-bar/src/main.mesh`, `modules/frontend/navigation-bar/src/components/settings-button.mesh`, `modules/frontend/navigation-bar/src/components/theme-button.mesh`, `modules/frontend/navigation-bar/src/components/volume-button.mesh`, `modules/frontend/audio-popover/src/main.mesh`, `docs/frontend/mesh-syntax.md`, `docs/css-coverage.md`, `docs/modules/frontend/core/navigation-bar/README.md`, `crates/core/shell/src/shell/component/tests.rs` | 11-04 |

## Tests To Mirror

- `crates/core/shell/src/shell/component/tests.rs` integration-style tests that build real frontend components and assert request routing, runtime state, and rebuilt-tree behavior.
- `crates/core/shell/src/shell/mod.rs` tests that prove shell-global shortcut precedence and shell-owned request handling.
- `crates/core/ui/render/src/render.rs` tests for accessibility-tag defaults and compiled frontend trees.
- `crates/core/ui/elements/src/style.rs` tests for selector-state matching semantics.

## Implementation Guidance

- Keep traversal and focus keyed by `_mesh_key`. Do not make `WidgetNode::id` the persisted focus identity.
- Derive traversal order from layout geometry, then use `tabindex` only as an override tool. Do not silently fall back to template/source order unless geometry is truly tied.
- Keep shell-global shortcuts authoritative. Focused-surface shortcuts are an additional layer, not a replacement.
- When adding docs or manifest metadata for shortcuts, keep the runtime route and the advertised string synchronized so accessibility data stays truthful.
- Use existing shipped surfaces for proof coverage whenever possible, and keep input-specific regression tests in the shell test suite rather than bloating navigation-bar with unrelated controls.
