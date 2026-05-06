---
phase: 11-keyboard-navigation-and-shortcuts
verified: 2026-05-06T16:39:00Z
status: human_needed
score: "10/10 must-haves verified"
overrides_applied: 0
human_verification:
  - test: "Wayland navigation-bar keyboard focus acquisition"
    expected: "With the navigation bar focused in a live compositor, Tab and Shift+Tab traverse controls, Enter and Space activate focused controls, arrows step the audio slider, m triggers the mute shortcut, shell-global shortcuts still win, and unfocused surfaces do not react."
    why_human: "The shell tests prove routing and shortcut precedence, but they cannot fully validate compositor-specific layer-shell keyboard_mode behavior."
  - test: "Pointer-to-keyboard focus-visible coherence on a real input"
    expected: "Clicking a text input keeps :focus-visible shown, then clicking a non-text control clears the strong visible-focus ring while logical focus remains correct."
    why_human: "Automated tests verify the heuristic state transitions, but not the final live-surface focus-ring feel."
---

# Phase 11: Keyboard Navigation and Shortcuts Verification Report

**Phase Goal:** Make shell UI usable without a mouse through deterministic focus traversal, keyboard activation, and plugin-defined shortcuts.
**Verified:** 2026-05-06T16:39:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | Tab and Shift+Tab move focus through focusable components in deterministic visual order, wrap, skip hidden/disabled targets, and honor `tabindex` overrides. | ✓ VERIFIED | `crates/core/shell/src/shell/layout.rs` implements `collect_focus_traversal`, `next_focus_target`, `node_is_tabbable`, and `tabindex=-1` filtering; `crates/core/shell/src/shell/component/tests.rs` covers visual-order traversal, wraparound, hidden/disabled skipping, and positive/negative `tabindex`. |
| 2 | `:focus` and `:focus-visible` are distinct, shell-owned states rather than aliases. | ✓ VERIFIED | `crates/core/ui/elements/src/tree.rs` adds `ElementState.focus_visible`; `crates/core/ui/elements/src/style.rs` matches `focus-visible` against that field; `crates/core/shell/src/shell/component/runtime_tree.rs` rehydrates both `focused` and `focus_visible` from stable `_mesh_key` state. |
| 3 | Pointer-focused text inputs keep visible focus, while pointer-focused non-text controls clear the stronger keyboard-style visible focus. | ✓ VERIFIED | `crates/core/shell/src/shell/component/input.rs` uses `pointer_focus_visible_for_key` and `set_focus_target`; regression test `keyboard_navigation_pointer_focus_visible_tracks_input_modality` proves the input/non-text split. |
| 4 | Focused controls expose visible focus style and respond through shell-owned keyboard defaults for buttons, toggles, sliders, and inputs. | ✓ VERIFIED | `crates/core/shell/src/shell/component/input.rs` dispatches focused `keydown`/`keyup`, input Backspace editing, slider arrow stepping, button activation on release, and toggle activation on release; tests cover button/toggle release, slider arrows, and focused input Backspace. |
| 5 | Focused `keydown` and `keyup` stay focused-element scoped and use the existing request/event pipeline. | ✓ VERIFIED | `crates/core/shell/src/shell/component/input.rs` builds keyboard events and calls `dispatch_focused_keyboard_handler`; `crates/core/shell/src/shell/types.rs` carries key/modifier payloads in `ComponentInput`; test `keyboard_handlers_keydown_and_keyup_payloads_route_to_focused_node` verifies payload shape and focused-target routing. |
| 6 | Phase 10 copy ownership still wins, and stale focused keys are pruned before keyboard dispatch. | ✓ VERIFIED | `crates/core/shell/src/shell/component/input.rs` checks `Ctrl+C` selection copy before shortcut/handler/default activation and clears missing focused keys via `normalized_focused_key`; tests `keyboard_handlers_ctrl_c_selection_still_wins_over_focused_button` and `keyboard_handlers_stale_focus_is_pruned_before_dispatch` verify both behaviors. |
| 7 | Plugin-defined key handlers and focused-surface shortcuts route through the existing shell event/capability model without bypassing shell-global precedence. | ✓ VERIFIED | `crates/core/shell/src/shell/mod.rs` intercepts shell-global shortcuts before component routing and then forwards `ComponentInput` into component handling; `crates/core/shell/src/shell/component/input.rs` resolves surface shortcuts, targets refs/focused nodes, and invokes namespaced handlers through the existing handler pipeline; tests verify shell-global precedence and focused-surface shortcut routing/metadata. |
| 8 | Keyboard defaults and focused-surface shortcuts are remappable through settings rather than fixed engine constants. | ✓ VERIFIED | `crates/core/foundation/config/src/lib.rs` defines `KeyboardSettings` and `surface_shortcuts`; `config/settings-default.json` provides concrete defaults; `crates/core/shell/src/shell/component/input.rs` reads those settings and matches runtime keys against configured bindings; config tests verify deterministic defaults/override merge behavior. |
| 9 | Shipped shell surfaces provide real proof coverage: the navigation bar exercises traversal, focused shortcut routing, and keyboard button activation, while the audio popover proves real slider stepping. | ✓ VERIFIED | `modules/frontend/navigation-bar/src/main.mesh` and `modules/frontend/navigation-bar/config/settings.json` wire the mute shortcut and keyboard-capable surface mode; `modules/frontend/audio-popover/src/main.mesh` uses shell-owned slider defaults; tests `navigation_bar_keyboard_shortcut_and_theme_activation_work_on_real_surface` and `navigation_bar_keyboard_audio_popover_slider_responds_to_arrow_keys` execute the shipped modules directly. |
| 10 | Author docs describe the shipped keyboard contract and real `:focus-visible` behavior. | ✓ VERIFIED | `docs/frontend/mesh-syntax.md` documents `tabindex`, focused-only `onkeydown` / `onkeyup`, and focused-surface shortcut scope; `docs/css-coverage.md` documents modality-aware `:focus-visible`; `docs/modules/frontend/core/navigation-bar/README.md` documents traversal, default button activation, and the mute shortcut proof. |

**Score:** 10/10 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/core/shell/src/shell/layout.rs` | Visual-order traversal collector with `tabindex` support | ✓ VERIFIED | Implements traversal collection, geometry sorting, wrapping, native focusability, and `tabindex` parsing/filtering. |
| `crates/core/ui/elements/src/tree.rs` | Runtime state carries visible-focus state | ✓ VERIFIED | `ElementState` includes `focus_visible`. |
| `crates/core/ui/elements/src/style.rs` | Real `:focus-visible` selector semantics | ✓ VERIFIED | `focus-visible` matches `state.focus_visible`; unit test proves it no longer aliases `focused`. |
| `crates/core/shell/src/shell/component/runtime_tree.rs` | Rebuilt trees rehydrate shell-owned focus state | ✓ VERIFIED | Writes `focused` and `focus_visible` into live node state using stable `_mesh_key` paths. |
| `crates/core/ui/render/src/render.rs` | Accessibility focusability aligns with keyboard target set | ✓ VERIFIED | `checkbox` and `switch` are marked focusable; targeted test passes. |
| `crates/core/shell/src/shell/component/input.rs` | Single shell-owned keyboard path for traversal, handlers, defaults, and shortcuts | ✓ VERIFIED | Owns Tab traversal, focused dispatch, copy precedence, slider/button/toggle/input defaults, and surface shortcuts. |
| `crates/core/shell/src/shell/types.rs` | Keyboard inputs preserve key/modifier payloads | ✓ VERIFIED | `ComponentInput::KeyPressed` and `KeyReleased` carry `KeyModifiers`. |
| `crates/core/foundation/config/src/lib.rs` | Keyboard settings schema and merge behavior | ✓ VERIFIED | Defines configurable activation keys and per-surface shortcut overrides with tests. |
| `config/settings-default.json` | Concrete shell keyboard defaults | ✓ VERIFIED | Ships button/toggle/slider bindings plus empty `surface_shortcuts` override map. |
| `modules/frontend/navigation-bar/config/settings.json` | Concrete focused-surface shortcut defaults | ✓ VERIFIED | Enables `keyboard_mode: "on_demand"` and defines the `m` mute shortcut targeting `volume-button`. |
| `modules/frontend/navigation-bar/src/main.mesh` | Real shipped proof surface | ✓ VERIFIED | Wires settings, volume, theme controls and `onMuteShortcut` through shell-owned handlers. |
| `modules/frontend/audio-popover/src/main.mesh` | Real shipped slider/control proof | ✓ VERIFIED | Uses shell-owned slider/button defaults and emits service commands from shipped handlers. |
| `docs/frontend/mesh-syntax.md` | Author-facing keyboard contract docs | ✓ VERIFIED | Documents traversal, `tabindex`, focused handlers, and focused-surface shortcut scope. |
| `docs/css-coverage.md` | Author-facing `:focus-visible` guidance | ✓ VERIFIED | Describes modality-aware visible focus and the `:focus`/`:focus-visible` split. |
| `crates/core/shell/src/shell/component/tests.rs` | Regression coverage for KEY-01..KEY-04 | ✓ VERIFIED | Contains focused traversal, activation, shortcut, real-surface, and mixed-regression tests. |

### Key Link Verification

| From | To | Via | Status | Details |
| --- | --- | --- | --- | --- |
| `crates/core/shell/src/shell/component/runtime_tree.rs` | `crates/core/ui/elements/src/tree.rs` | `ElementState { focused, focus_visible }` | ✓ VERIFIED | Runtime-tree annotation writes the new `focus_visible` field onto live node state keyed by `_mesh_key`. |
| `crates/core/shell/src/shell/layout.rs` | `crates/core/shell/src/shell/component/input.rs` | `next_focus_target` / `advance_keyboard_focus` | ✓ VERIFIED | Keyboard Tab handling calls the post-layout traversal helper instead of maintaining a separate focus list. |
| `crates/core/shell/src/shell/mod.rs` | `crates/core/shell/src/shell/types.rs` | `component_key_pressed_input` / `component_key_released_input` | ✓ VERIFIED | Shell routing preserves key names and modifier state into `ComponentInput`. |
| `crates/core/shell/src/shell/mod.rs` | `crates/core/shell/src/shell/component/input.rs` | shell-global shortcut gate before `handle_input` | ✓ VERIFIED | `dispatch_wayland` processes shell-global shortcuts first, then routes remaining keyboard events to the focused surface component. |
| `crates/core/shell/src/shell/component/input.rs` | `crates/core/shell/src/shell/component/tests.rs` | `keyboard_*` and navigation-bar tests | ✓ VERIFIED | The tests exercise traversal, activation, stale-key pruning, surface shortcuts, and real proof surfaces against the same input path. |
| `modules/frontend/navigation-bar/module.json` | `config/settings-default.json` | shared keyboard settings contract | ✓ VERIFIED | The module schema declares `keyboard.shortcuts`; shell defaults define the matching shell `keyboard` section and surface override map used by runtime shortcut resolution. |
| `modules/frontend/navigation-bar/src/main.mesh` | `crates/core/shell/src/shell/component/tests.rs` | `real_frontend_module_component("@mesh/navigation-bar", ...)` | ✓ VERIFIED | The shipped navigation-bar module is compiled and exercised directly by shell tests, not replaced with a synthetic fixture. |
| `docs/css-coverage.md` | `crates/core/ui/elements/src/style.rs` | `:focus-visible` semantics | ✓ VERIFIED | Documentation says `:focus-visible` is modality-aware, which matches selector evaluation against `state.focus_visible`. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| --- | --- | --- | --- | --- |
| `crates/core/shell/src/shell/component/runtime_tree.rs` | `node.state.focus_visible` | `FrontendSurfaceComponent.focus_visible_key` set by pointer/keyboard handling in `component/input.rs` | Yes | ✓ FLOWING |
| `crates/core/shell/src/shell/component/input.rs` | `keyboard_settings` and resolved surface shortcuts | `mesh_core_config::load_shell_settings()` + module `settings_json.keyboard.shortcuts` | Yes | ✓ FLOWING |
| `modules/frontend/navigation-bar/src/main.mesh` | `onMuteShortcut` / `audio_surface_hidden` | surface settings + runtime service binding / handler pipeline | Yes | ✓ FLOWING |
| `modules/frontend/audio-popover/src/main.mesh` | `slider_value` and audio commands | `audio.percent` service state + shell keyboard slider stepping | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| --- | --- | --- | --- |
| `:focus-visible` is distinct from plain focus | `nix develop -c cargo test -p mesh-core-elements focus_visible_requires_focus_visible_state` | 1 test passed | ✓ PASS |
| Shell keyboard settings defaults and overrides merge | `cargo test -p mesh-core-config keyboard_settings -- --nocapture` | 2 tests passed | ✓ PASS |
| Accessibility focusability includes checkbox and switch | `nix develop -c cargo test -p mesh-core-render accessibility_for_tag_marks_switch_and_checkbox_focusable` | 1 test passed | ✓ PASS |
| Focused-surface shortcut routing and metadata annotation | `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts_surface_handler_runs_and_metadata_matches_binding` | 1 test passed | ✓ PASS |
| Real navigation-bar keyboard shortcut and button activation | `nix develop -c cargo test -p mesh-core-shell navigation_bar_keyboard_shortcut_and_theme_activation_work_on_real_surface` | 1 test passed | ✓ PASS |
| Real audio-popover slider arrow stepping | `nix develop -c cargo test -p mesh-core-shell navigation_bar_keyboard_audio_popover_slider_responds_to_arrow_keys` | 1 test passed | ✓ PASS |
| Mixed buttons/sliders/inputs regression path | `nix develop -c cargo test -p mesh-core-shell keyboard_regression_buttons_sliders_inputs_and_pointer_modality` | 1 test passed | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| --- | --- | --- | --- | --- |
| `KEY-01` | `11-01` | Users can move focus through focusable components using Tab and Shift+Tab in deterministic visual order. | ✓ SATISFIED | `layout.rs` traversal helpers plus navigation tests for visual order, wrap, hidden/disabled skip, and `tabindex`. |
| `KEY-02` | `11-01`, `11-02`, `11-04` | Focused controls expose a visible focus style and activate through keyboard actions such as Enter, Space, arrows, or component-specific configured keys. | ✓ SATISFIED | Distinct `focus_visible` state, pointer-modality handling, release-based button/toggle activation, slider arrows, input Backspace, and real-surface navigation/audio tests. |
| `KEY-03` | `11-02`, `11-03`, `11-04` | Plugin authors can define keyboard shortcuts or key handlers for components without bypassing focus and capability rules. | ✓ SATISFIED | Focused `keydown` / `keyup` handler dispatch, surface shortcut resolution through module settings plus shell overrides, shell-global precedence, and real navigation-bar mute shortcut proof. |
| `KEY-04` | `11-02`, `11-03`, `11-04` | Keyboard navigation and shortcuts are covered by tests for buttons, sliders, inputs, and navigation-bar controls. | ✓ SATISFIED | `component/tests.rs` includes button, slider, input, shortcut, regression, and real navigation-bar/audio-popover keyboard tests. |

Phase 11 requirement IDs declared in plan frontmatter are fully accounted for in `.planning/REQUIREMENTS.md`. No orphaned Phase 11 requirements were found.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| --- | --- | --- | --- | --- |
| `—` | `—` | No TODO/FIXME/placeholder/empty user-visible stub patterns found in the phase-touched artifacts. | ℹ️ Info | No anti-pattern blocker was found during phase-scoped scanning. |

### Human Verification Required

### 1. Wayland Navigation Focus

**Test:** Start MESH in a Wayland session, focus the navigation-bar surface, then press `Tab`, `Shift+Tab`, `Enter`, `Space`, arrow keys, and `m`.
**Expected:** Traversal, activation, slider stepping, and the mute shortcut work only while that surface owns keyboard focus, and shell-global shortcuts still win.
**Why human:** Automated tests prove shell routing and precedence, but not compositor-specific `keyboard_mode: "on_demand"` behavior.

### 2. Live Focus-Visible Feel

**Test:** Open a real surface with an input, click into the input, then click a non-text control.
**Expected:** The input keeps `:focus-visible`; the non-text control keeps logical focus but clears the stronger visible-focus ring.
**Why human:** The shell tests verify state transitions, but not the final live-surface focus-ring behavior.

### Gaps Summary

No code-level blocker gap was found in the phase artifacts or targeted spot-checks. Automated verification supports all Phase 11 must-haves. Remaining work is the manual compositor/live-surface verification already identified in the phase validation plan, so the phase status is `human_needed` rather than `passed`.

---

_Verified: 2026-05-06T16:39:00Z_
_Verifier: the agent (gsd-verifier)_
