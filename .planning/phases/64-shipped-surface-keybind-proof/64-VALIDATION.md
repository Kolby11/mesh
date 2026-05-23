# Phase 64 Validation

## Must Pass

- Real navigation keybind dispatch test.
- Real audio-popover access-key dispatch test.
- Focused locale/override/no-binding resolver tests.
- Focused debug metadata tests.
- Full navigation interaction suite.

## Commands

```bash
nix develop -c cargo test -p mesh-core-shell navigation_bar_keyboard_shortcut_and_theme_activation_work_on_real_surface -- --nocapture
nix develop -c cargo test -p mesh-core-shell audio_popover_access_key -- --nocapture
nix develop -c cargo test -p mesh-core-shell keybind_locale -- --nocapture
nix develop -c cargo test -p mesh-core-shell keybind_diagnostic -- --nocapture
nix develop -c cargo test -p mesh-core-shell keybind_debug -- --nocapture
nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation -- --nocapture
```
