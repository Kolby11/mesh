---
phase: 11
slug: keyboard-navigation-and-shortcuts
status: complete
created: 2026-05-06
---

# Phase 11 Research - Keyboard Navigation and Shortcuts

## Goal

Make MESH shell surfaces usable without a mouse by adding deterministic keyboard traversal, modality-aware focus styling, default keyboard activation for core controls, and focused-surface shortcut routing that preserves shell-global precedence.

## Findings

### Current focus and interaction state is already shell-owned

- `crates/core/shell/src/shell/component.rs` stores stable interaction state on `FrontendSurfaceComponent`, including `focused_key`, `pointer_down_key`, `active_slider_key`, selection state, value maps, and scroll offsets.
- `crates/core/shell/src/shell/component/runtime_tree.rs` already rehydrates runtime element state from stable `_mesh_key` paths, so Phase 11 should keep keyboard state shell-owned instead of inventing a transient render-only focus model.
- `crates/core/shell/src/shell/component/interaction_state.rs` already prunes stale `focused_key`, hover, slider, and selection targets after rebuilds. Keyboard traversal should reuse that pruning pattern so focus never points at removed nodes.

### The current key-event path stops short of real keyboard navigation

- `crates/core/shell/src/shell/mod.rs` already intercepts shell-global shortcuts before component routing, which is the correct precedence boundary for Phase 11.
- `ComponentInput::KeyPressed` preserves modifier state via `KeyModifiers`, but `ComponentInput::KeyReleased` still carries only the key name.
- `crates/core/shell/src/shell/component/input.rs` currently handles only a narrow set of keyboard behaviors:
  - `Ctrl+C` copies Phase 10 text selection when present.
  - `Backspace` edits focused inputs.
  - `Enter` / `Space` toggle focused switches and checkboxes.
- There is no general Tab / Shift+Tab traversal path, no default button activation path, no slider arrow-key path, and no general focused-element `keydown` / `keyup` dispatch.

### Focusability and traversal are still tag-based and pointer-oriented

- `crates/core/shell/src/shell/layout.rs` exposes `find_focusable_at`, but it only hit-tests the deepest focusable node under a pointer position.
- That helper considers `input`, `button`, `slider`, `switch`, and `checkbox` focusable for pointer hit testing, but there is no collector that produces a deterministic traversal order across the whole post-layout tree.
- There is no existing `tabindex`-style override path in the runtime, element metadata, or docs.
- Because the tree is rebuilt and laid out before input handling, the correct place to derive traversal order is the final post-layout widget tree, not template/source order.

### `:focus-visible` is still a temporary alias

- `crates/core/ui/elements/src/tree.rs` currently stores only `hovered`, `active`, `focused`, `disabled`, and `checked` in `ElementState`.
- `crates/core/ui/elements/src/style.rs` treats `:focus-visible` as a direct alias of `state.focused`.
- `docs/css-coverage.md` documents `:focus-visible` as just another runtime element state rather than a modality-aware heuristic.
- Phase 9 intentionally deferred real keyboard modality until Phase 11, so replacing the alias with explicit `focus_visible` state is now the expected next step.

### Accessibility metadata is present but only partially aligned

- `crates/core/ui/render/src/render.rs` currently marks only `button`, `input`, and `slider` as focusable in `AccessibilityInfo`, even though the shell already treats `switch` and `checkbox` as keyboard-relevant controls.
- `crates/core/ui/elements/src/accessibility.rs` already includes `keyboard_shortcut: Option<String>`, which gives Phase 11 a natural place to expose surfaced shortcut hints once the routing contract exists.
- This means Phase 11 does not need a brand-new accessibility shape, but it does need to align keyboard focusability and shortcut metadata with the shell runtime.

### Settings and shortcut plumbing are not in place yet

- `crates/core/foundation/config/src/lib.rs` defines `ShellSettings` with only `theme`, `i18n`, and `sounds`. There is no shell-wide keyboard section yet.
- Module-owned surface settings already flow through `module.json` plus `config/settings.json`, then into `SurfaceLayoutSettings` in `crates/core/shell/src/shell/surface_layout.rs`.
- `modules/frontend/navigation-bar/config/settings.json` currently sets `"keyboard_mode": "none"`, which blocks real keyboard proof behavior on the milestone’s main shell chrome surface.
- `modules/frontend/audio-popover/module.json` and the navigation-bar manifest already expose user-editable settings schemas, so Phase 11 can add module-owned shortcut defaults without inventing a new configuration channel.

### Proof surfaces already exist for most requirement coverage

- Navigation-bar already provides three real button-like controls: settings, volume, and theme.
- Audio popover already provides a real slider and three action buttons.
- Input coverage is best handled through existing shell component tests or a small focused fixture rather than forcing navigation-bar to grow an unrelated input field.
- Because Phase 10 already claimed `Ctrl+C` when a text selection exists, Phase 11 tests must prove that keyboard activation and shortcut routing do not steal that copy behavior.

## Recommended Implementation Shape

1. Add explicit keyboard-modality and `focus_visible` state to the shell-owned focus model, then feed it into `ElementState` so `:focus-visible` stops aliasing `:focus`.
2. Extend layout helpers with a full traversal collector that operates on the final laid-out tree, sorts by visual order, skips hidden/disabled targets, wraps at the ends, and supports `tabindex` plus `tabindex="-1"` semantics.
3. Route Tab / Shift+Tab focus movement through `FrontendSurfaceComponent`, including blur/focus handler dispatch and pointer-to-keyboard coherence rules.
4. Add focused-element `keydown` / `keyup` dispatch plus shell-owned default activation behavior for buttons, switches, checkboxes, sliders, and inputs.
5. Add shell settings for remappable default keyboard actions and module-owned surface shortcut defaults, then merge shell overrides on top without bypassing shell-global shortcuts.
6. Prove the behavior on real shell surfaces: navigation-bar for button traversal/shortcuts, audio popover for sliders and control buttons, and shell tests for focused input behavior.

## Validation Architecture

Use targeted Rust tests while implementing, then run a broader keyboard suite before phase verification.

- Quick command: `nix develop -c cargo test -p mesh-core-shell keyboard_`
- Full command: `nix develop -c cargo test -p mesh-core-config keyboard_ && nix develop -c cargo test -p mesh-core-elements focus_visible && nix develop -c cargo test -p mesh-core-render accessibility_for_tag && nix develop -c cargo test -p mesh-core-shell keyboard_`
- Primary coverage:
  - `:focus-visible` follows keyboard modality rather than plain logical focus.
  - Tab and Shift+Tab traverse deterministic visual order, skip hidden/disabled nodes, honor `tabindex`, and wrap.
  - Focus changes emit blur/focus handlers and stay keyed by stable `_mesh_key`.
  - Buttons, switches, checkboxes, sliders, and inputs respond to the agreed keyboard defaults.
  - Focused-element `keydown` / `keyup` handlers and focused-surface shortcuts route through the existing request/capability model.
  - Shell-global shortcuts still win over focused-surface shortcuts.
  - Navigation-bar and audio popover demonstrate real keyboard behavior without regressing Phase 10 copy ownership.
- Sampling rule: every task that changes runtime keyboard behavior must add or update at least one automated regression test in the same task.

## Open Risks

- Visual-order traversal based on geometry needs deterministic tie-breaking for equal rows and partially overlapping layouts; tests should cover same-row and wrap behavior explicitly.
- `WindowKeyEvent::Released` currently lacks modifier payload, so any keyup behavior that depends on modifiers must be intentionally constrained or backed by shell-side pressed-key state.
- Changing navigation-bar keyboard interactivity from `"none"` to a focused keyboard mode needs live compositor confirmation so it does not create surprising global key capture behavior.
- Shortcut metadata should not drift away from actual routing. If a node advertises a keyboard shortcut in accessibility data, the runtime behavior must stay in sync.
