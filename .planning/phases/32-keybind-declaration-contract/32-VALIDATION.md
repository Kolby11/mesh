# Phase 32 Validation Strategy: Keybind Declaration Contract

**Phase:** 32 - Keybind Declaration Contract
**Date:** 2026-05-13

## Validation Architecture

Phase 32 is a declaration-contract phase, so validation should prove parsing, compatibility, and dispatch preservation.

## Required Test Coverage

### Manifest Parsing

- `module.json` with top-level `keybinds` parses into normalized manifest keybind actions.
- Optional metadata parses correctly: `label`, `label_i18n_key`, `scope`, `target_ref`, trigger kind, key, and modifiers.
- Existing manifests without keybinds still parse.

### Runtime Compatibility

- Existing `settings_json.keyboard.shortcuts` declarations still dispatch.
- Existing `KeyboardSettings.surface_shortcuts` overrides still win over defaults.
- Existing accessibility shortcut annotation still uses the effective key.

### Navigation-Bar Proof

- `@mesh/navigation-bar` mute shortcut remains active.
- The shortcut still targets `volume-button`.
- The shortcut still calls `onMuteShortcut`.

## Suggested Commands

- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-extension-module manifest`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts`
- `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell navigation_bar_keyboard_shortcut`

## Non-Goals

- No live shell UAT required for Phase 32 unless automated real-surface tests show a regression.
- No XDG portal/global shortcut validation.
- No localized Slovak/English access-key behavior until Phase 33.
