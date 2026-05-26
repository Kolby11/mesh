---
phase: 91-shell-proof-docs-and-hardening
plan: 01
title: Shipped Surface Semantic Migration Summary
status: complete
---

# Summary

- Migrated audio popover root to native `popover` semantics.
- Migrated debug inspector root to native `dialog` semantics.
- Migrated backend services view to `empty-state`, `list`, and `list-item`.
- Preserved existing classes, handlers, layout, and shipped behavior.

# Verification

- `nix develop -c cargo test -p mesh-core-shell shipped_audio_popover_content_measured_surface_contains_volume_actions`
- `nix develop -c cargo test -p mesh-core-shell debug_inspector_all_four_views_keep_stable_empty_or_pending_states_on_real_surface`
- `nix develop -c cargo test -p mesh-core-shell navigation_language_button_publishes_locale_request_on_real_surface`
