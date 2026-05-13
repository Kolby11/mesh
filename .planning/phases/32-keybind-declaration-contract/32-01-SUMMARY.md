# Phase 32-01 Summary: Keybind Declaration Contract and Compatibility Bridge

**Completed:** 2026-05-13
**Commits:**
- `1978d11 feat(32): add keybind manifest declarations`
- `8790f5a feat(32): bridge surface shortcuts through keybind declarations`
- `140c257 feat(32): declare navigation bar keybind metadata`
- `288a8b5 chore(32): align shortcut bridge helper contract`

## Delivered

- Added normalized `Manifest.keybinds` support with typed `KeybindsSection`, `KeybindAction`, `KeybindTrigger`, `KeybindScope`, and `KeybindTriggerKind`.
- Wired keybind parsing through module JSON, legacy TOML, and package-style manifests.
- Added validation for stable action ids, non-empty handlers, required trigger keys, and non-empty optional metadata fields.
- Bridged shell surface shortcut dispatch through typed declarations while preserving legacy `settings.keyboard.shortcuts` compatibility.
- Preserved user override identity by stable action id through `KeyboardSettings.surface_shortcuts[surface_id][action_id]`.
- Added navigation-bar mute keybind metadata with `scope`, `label_i18n_key`, and manifest-level typed declaration.

## Verification

Ran through `nix develop` because the host shell environment lacked the native `xkbcommon.pc` dependency needed by Wayland test crates.

- `cargo fmt --check`
- `cargo test -p mesh-core-module keybind`
- `cargo test -p mesh-core-shell keyboard_shortcuts`
- `cargo test -p mesh-core-shell navigation_bar_keyboard_shortcut`

All focused checks passed.

## Notes

- The planned package name `mesh-core-extension-module` is stale; the actual crate is `mesh-core-module`.
- Phase 32 intentionally did not implement locale fallback, duplicate/conflict diagnostics, global shortcuts, or expanded dispatch payloads. Those remain assigned to later v1.6 phases.
