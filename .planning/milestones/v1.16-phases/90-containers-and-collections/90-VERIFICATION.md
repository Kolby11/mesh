---
phase: 90-containers-and-collections
title: Verification
status: complete
---

# Verification

- `nix develop -c cargo test -p mesh-core-elements phase90`
- `nix develop -c cargo test -p mesh-core-frontend phase90`
- `nix develop -c cargo test -p mesh-core-shell phase90`
- `nix develop -c cargo test -p mesh-core-shell debug_inspector_all_four_views_keep_stable_empty_or_pending_states_on_real_surface`
- `nix develop -c cargo test -p mesh-tools-lsp phase90`
- `nix develop -c cargo fmt --check`

# Notes

- Shell target still emits pre-existing dead code warnings during tests.
