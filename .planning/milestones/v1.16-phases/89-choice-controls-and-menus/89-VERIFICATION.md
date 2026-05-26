---
phase: 89-choice-controls-and-menus
title: Verification
status: complete
---

# Verification

## Passing

- `nix develop -c cargo test -p mesh-core-elements phase89`
- `nix develop -c cargo test -p mesh-core-frontend phase89`
- `nix develop -c cargo test -p mesh-core-shell phase89`
- `nix develop -c cargo test -p mesh-core-shell navigation_language_button_publishes_locale_request_on_real_surface`
- `nix develop -c cargo test -p mesh-core-shell icon_reliability_core_surfaces_proof`
- `nix develop -c cargo test -p mesh-core-shell audio_popover_access_key_toggles_mute_on_real_surface`
- `nix develop -c cargo test -p mesh-core-shell shipped_navigation_volume_icon_inherits_button_click_and_tooltip`
- `nix develop -c cargo test -p mesh-core-shell navigation_buttons_animate_shape_from_squircle_to_circle_with_transform`
- `nix develop -c cargo test -p mesh-core-shell real_navigation_bar_repaints_existing_transition_state_when_theme_changes_back_to_dark`
- `nix develop -c cargo test -p mesh-tools-lsp phase89`
- `nix develop -c cargo fmt --check`

## Residual

- The broad command `nix develop -c cargo test -p mesh-core-elements -p mesh-core-frontend -p mesh-core-interaction -p mesh-core-shell -p mesh-tools-lsp --no-fail-fast` still exposes `container_size_restyle_preserves_runtime_and_local_state` failing in isolation with a stale container-query background after surface-size restyle. This was not introduced by the Phase 89 choice/menu code path and remains a separate shell invalidation issue.
