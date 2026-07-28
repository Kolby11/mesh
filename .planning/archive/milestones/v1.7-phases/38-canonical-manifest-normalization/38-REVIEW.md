---
phase: 38
status: clean
reviewed: 2026-05-17
scope:
  - crates/core/extension/module/src/package
  - crates/core/extension/module/src/manifest
  - crates/core/shell/src/shell root graph references
  - config/module.json
  - modules/backend and modules/frontend shipped manifests
---

# Phase 38 Code Review

## Findings

No blocking or warning-level issues found in the Phase 38 manifest normalization changes.

## Notes

- The old Rust manifest type names were removed rather than kept as public compatibility aliases.
- Canonical `module.json` and legacy manifest inputs are distinguishable through source variants and diagnostics.
- Duplicate manifest files and `plugin.json` now produce blocking diagnostics.
- Navigation-bar migration required extending the canonical module schema with `accessibility`, `iconRequirements`, and `surfaceLayout` so existing runtime data is not dropped.
- Full `mesh-core-shell shell::tests` still has two pointer-focus failures unrelated to Phase 38 manifest normalization; focused shell root graph tests passed.

## Verification Reviewed

- `cargo test -p mesh-core-module package::tests`
- `cargo test -p mesh-core-module manifest::tests`
- `nix develop -c cargo test -p mesh-core-shell shell::tests::backend_lifecycle_uses_explicit_active_provider_from_package_graph -- --exact`
- `nix develop -c cargo test -p mesh-core-shell shell::tests::load_frontend_components_keeps_shell_shipped_debug_inspector_even_when_not_in_package_graph -- --exact`

