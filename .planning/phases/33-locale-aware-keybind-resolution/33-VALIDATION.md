---
phase: 33
title: Locale-Aware Keybind Resolution
created: 2026-05-13
---

# Validation Strategy: Phase 33

## Validation Architecture

Phase 33 changes manifest parsing and shell-side keybind resolution. Validation must prove both typed declaration parsing and deterministic resolver precedence.

## Required Automated Checks

- `nix develop -c cargo fmt --check`
- `nix develop -c cargo test -p mesh-core-module localized_keybind`
- `nix develop -c cargo test -p mesh-core-shell keybind_locale`
- `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts`

## Required Behavioral Proof

- User override by stable action id wins over locale and generic defaults.
- Exact active locale trigger wins over parent locale and generic defaults.
- Parent locale trigger wins over generic default.
- Missing or blank localized triggers fall back to generic default.
- Locale-specific shortcut defaults are ignored in Phase 33; access keys are the localized trigger target.
- Existing navigation-bar shortcut behavior remains compatible.

## Non-Goals

- Duplicate/conflict diagnostics.
- Rich script dispatch payloads.
- Accessibility metadata proof.
- Compositor-global shortcuts.
