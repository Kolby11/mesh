# Phase 63 Validation

## Must Pass

- Accessibility annotation tests for resolved keybind metadata.
- Debug snapshot/payload tests for structured resolved keybind entries.
- Author docs contain declaration, localized trigger, override, diagnostics, accessibility, and focused-surface scope guidance.

## Commands

```bash
nix develop -c cargo test -p mesh-core-shell keybind_debug -- --nocapture
nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts_surface_handler_runs_and_metadata_matches_binding -- --nocapture
nix develop -c cargo test -p mesh-core-shell debug_snapshot -- --nocapture
```
