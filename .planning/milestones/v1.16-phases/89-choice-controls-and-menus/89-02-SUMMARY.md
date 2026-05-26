---
phase: 89-choice-controls-and-menus
plan: 02
title: Shell Choice And Menu Behavior Summary
status: complete
---

# Summary

- Added source-tag helpers in `mesh-core-interaction` so lowered choice/menu nodes keep distinct behavior.
- Added shell behavior for option activation, parent select `change`, radio exclusivity, checkable choices, menu `activate`, and simple roving focus among option/menu siblings.
- Annotated select/radio-group values and choice checked/selected state into runtime attributes and accessibility state.
- Fixed resolved accessibility shortcuts so keybind ids are replaced by the active key label instead of duplicated with it.

# Verification

- `nix develop -c cargo test -p mesh-core-shell phase89`
- `nix develop -c cargo test -p mesh-core-shell audio_popover_access_key_toggles_mute_on_real_surface`
- `nix develop -c cargo test -p mesh-core-shell shipped_navigation_volume_icon_inherits_button_click_and_tooltip`
- `nix develop -c cargo test -p mesh-core-shell navigation_buttons_animate_shape_from_squircle_to_circle_with_transform`
- `nix develop -c cargo test -p mesh-core-shell real_navigation_bar_repaints_existing_transition_state_when_theme_changes_back_to_dark`
