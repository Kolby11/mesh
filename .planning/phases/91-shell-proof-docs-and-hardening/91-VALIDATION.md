---
phase: 91-shell-proof-docs-and-hardening
title: Validation
status: planned
---

# Validation Plan

- `nix develop -c cargo test -p mesh-core-shell shipped_audio_popover_content_measured_surface_contains_volume_actions`
- `nix develop -c cargo test -p mesh-core-shell debug_inspector_all_four_views_keep_stable_empty_or_pending_states_on_real_surface`
- `nix develop -c cargo test -p mesh-core-shell navigation_language_button_publishes_locale_request_on_real_surface`
- `nix develop -c cargo test -p mesh-core-elements -p mesh-core-frontend -p mesh-core-interaction -p mesh-tools-lsp`
- `nix develop -c cargo fmt --check`
