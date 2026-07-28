---
phase: 89-choice-controls-and-menus
plan: 03
title: Navigation Migration Docs And Tooling Summary
status: complete
---

# Summary

- Migrated the shipped navigation language selector to native `select` with static `option` children.
- Kept the navigation service behavior: selecting Slovak still publishes `SetLocale { locale: "sk" }`.
- Added the missing `language` icon mapping used by the shipped navigation manifest.
- Updated LSP tag knowledge for choice/menu tags and updated frontend element/syntax docs.

# Verification

- `nix develop -c cargo test -p mesh-core-shell navigation_language_button_publishes_locale_request_on_real_surface`
- `nix develop -c cargo test -p mesh-tools-lsp phase89`
- `nix develop -c cargo fmt --check`
