---
phase: 38-canonical-manifest-normalization
plan: 03
subsystem: shipped-module-fixtures
tags: [manifest, root-graph, shell]
key-files:
  created:
    - config/module.json
    - modules/backend/pipewire-audio/module.json
  modified:
    - modules/backend/pulseaudio-audio/module.json
    - modules/frontend/navigation-bar/module.json
    - crates/core/shell/src/shell/discovery.rs
    - crates/core/shell/src/shell/backend/spawn.rs
    - crates/core/shell/src/shell/tests.rs
requirements-completed: [MAN-01, MAN-02]
completed: 2026-05-17
duration: "inline"
---

# Phase 38 Plan 03: Root Graph And Shipped Manifest Migration Summary

The checked-in root graph and runtime-loaded shipped manifests now use canonical `module.json` paths and schema while preserving active audio provider selection, layout entrypoint resolution, backend provider declarations, dependencies, capabilities, and navigation keybind data.

## Commits

| Task | Commit | Description |
|------|--------|-------------|
| 1-2 | 09895d0 | Migrated `config/module.json`, PipeWire/PulseAudio/navigation manifests, shell root graph references, and fixture tests. |

## Verification

- `nix develop -c cargo test -p mesh-core-shell shell::tests::backend_lifecycle_uses_explicit_active_provider_from_package_graph -- --exact` passed.
- `nix develop -c cargo test -p mesh-core-shell shell::tests::load_frontend_components_keeps_shell_shipped_debug_inspector_even_when_not_in_package_graph -- --exact` passed.
- `cargo test -p mesh-core-module package::tests` passed.
- `cargo test -p mesh-core-module manifest::tests` passed.

## Deviations from Plan

The canonical schema was extended with `accessibility`, `iconRequirements`, and `surfaceLayout` because migrating `navigation-bar` without those fields would have silently dropped supported runtime data.

Total deviations: 1 auto-fixed preservation gap. Impact: positive; it preserves existing manifest behavior.

## Self-Check: PASSED

