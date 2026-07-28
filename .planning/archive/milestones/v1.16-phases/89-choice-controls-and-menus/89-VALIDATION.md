---
phase: 89-choice-controls-and-menus
title: Validation
status: planned
---

# Validation Plan

- `nix develop -c cargo test -p mesh-core-elements phase89`
- `nix develop -c cargo test -p mesh-core-frontend phase89`
- `nix develop -c cargo test -p mesh-core-shell phase89`
- `nix develop -c cargo test -p mesh-core-shell navigation_language_button_publishes_locale_request_on_real_surface`
- `nix develop -c cargo test -p mesh-tools-lsp phase89`
- `nix develop -c cargo test -p mesh-core-elements -p mesh-core-frontend -p mesh-core-shell -p mesh-tools-lsp`
- `nix develop -c cargo fmt --check`
