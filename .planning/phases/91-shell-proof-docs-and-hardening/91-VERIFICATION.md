---
phase: 91-shell-proof-docs-and-hardening
title: Verification
status: complete
---

# Verification

## Passing

- `nix develop -c cargo test -p mesh-core-shell shipped_audio_popover_content_measured_surface_contains_volume_actions`
- `nix develop -c cargo test -p mesh-core-shell debug_inspector_all_four_views_keep_stable_empty_or_pending_states_on_real_surface`
- `nix develop -c cargo test -p mesh-core-shell navigation_language_button_publishes_locale_request_on_real_surface`
- `nix develop -c cargo test -p mesh-core-elements -p mesh-core-frontend -p mesh-core-interaction -p mesh-tools-lsp --no-fail-fast`
- `nix develop -c cargo fmt --check`

## Residual

- The full shell suite still has the pre-existing `container_size_restyle_preserves_runtime_and_local_state` failure around container-query restyle after surface-size changes. It predates Phase 91 scope and is unrelated to the native element semantic migrations.
