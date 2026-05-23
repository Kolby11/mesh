---
phase: 60
slug: surface-keybind-dispatch-runtime
status: complete
researched: 2026-05-23
---

# Phase 60 Research: Surface Keybind Dispatch Runtime

## Question

What needs to be known to plan Phase 60 well?

## Current Runtime Shape

Surface keybind dispatch already has a partial runtime path:

- `crates/core/shell/src/shell/component/input/mod.rs` handles `ComponentInput::KeyPressed`.
- Tab and Escape cross-surface focus handling run before surface shortcuts.
- `Ctrl+C` selection copy runs before surface shortcuts when a text selection exists.
- `FrontendSurfaceComponent::dispatch_surface_shortcut` in `input/keyboard.rs` resolves a matching shortcut, finds runtime tree subscribers with `keybind` plus `onkeybind`, builds a keyboard event, and calls the existing component handler path.
- `ResolvedSurfaceShortcut` already carries keybind id, key, modifiers, trigger kind, and resolution source.
- `annotate_surface_shortcuts` already writes resolved shortcut strings into accessibility metadata, but Phase 63 owns broader metadata/observability.

## Known Gap

The current `KeyPressed` order dispatches surface shortcuts before focused input/default handling. This is correct for general focused-surface semantic actions, but a bare printable keybind can steal normal text entry when an input owns focus. Phase 60 context explicitly locks that bare printable keybinds must not steal focused text input.

The safest Phase 60 implementation is narrow:

1. Add characterization tests around the existing manifest-owned subscriber dispatch path.
2. Add a focused-input guard for bare printable surface keybinds.
3. Preserve modified shortcuts such as `Ctrl+M`, shell-global shortcut precedence, Tab/Escape, and `Ctrl+C` selection copy.
4. Strengthen shipped navigation proof that real manifest-owned actions dispatch without relying only on legacy settings shortcuts.

## Files To Plan Around

| Area | File | Notes |
|------|------|-------|
| Key event order | `crates/core/shell/src/shell/component/input/mod.rs` | Dispatch order and focused input/default behavior. |
| Shortcut resolution/dispatch | `crates/core/shell/src/shell/component/input/keyboard.rs` | Resolved shortcuts, subscribers, event payload. |
| Runtime tests | `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` | Existing keybind and navigation tests. |
| Manifest model | `crates/core/extension/module/src/manifest/model.rs` | Canonical `KeybindAction` and `KeybindTrigger` source. |
| Shipped manifest | `modules/frontend/navigation-bar/module.json` | Declares `mesh.keybinds.mute`. |
| Shipped subscriber | `modules/frontend/navigation-bar/src/components/volume-button.mesh` | Uses `keybind` and `onkeybind`. |

## Validation Architecture

Phase 60 can be validated with focused Rust tests:

- `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts`
- `nix develop -c cargo test -p mesh-core-shell navigation_bar_keyboard_shortcut_and_theme_activation_work_on_real_surface`
- `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation`

No manual-only verification is required for Phase 60.

## Planning Recommendation

Use one plan with three tasks:

1. Add/strengthen tests for manifest subscriber dispatch, no-subscriber no-op, and focused input protection.
2. Implement the focused input guard in `input/mod.rs` or a small helper nearby, keeping `dispatch_surface_shortcut` unchanged unless tests force a narrow helper extraction.
3. Strengthen shipped navigation proof and run the focused shell test commands.

Avoid moving locale-resolution, duplicate-binding diagnostics, missing-target diagnostics, or settings UI work into Phase 60.
