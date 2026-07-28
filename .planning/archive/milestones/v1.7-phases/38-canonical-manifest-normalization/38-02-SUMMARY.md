---
phase: 38-canonical-manifest-normalization
plan: 02
subsystem: module-manifest-loader
tags: [manifest, loader, migration]
key-files:
  created: []
  modified:
    - crates/core/extension/module/src/package/installed_graph.rs
    - crates/core/extension/module/src/manifest/load.rs
    - crates/core/extension/module/src/manifest/model.rs
    - crates/core/extension/module/src/package/tests.rs
    - crates/core/extension/module/src/manifest/tests.rs
requirements-completed: [MAN-01, MAN-02, MAN-03]
completed: 2026-05-17
duration: "inline"
---

# Phase 38 Plan 02: Canonical Module Loader And Migration Paths Summary

The manifest loader now distinguishes canonical `module.json` from legacy `package.json`, old `module.json`, and `mesh.toml` migration inputs, with blocking errors for ambiguous manifest files and unsupported `plugin.json`.

## Commits

| Task | Commit | Description |
|------|--------|-------------|
| 1-2 | 09895d0 | Added canonical/legacy source variants, shape detection, replacement diagnostics, ambiguity errors, and regression tests. |

## Verification

- `cargo test -p mesh-core-module package::tests` passed.
- `cargo test -p mesh-core-module manifest::tests` passed.
- Tests cover `CanonicalModuleJson`, `LegacyPackageJson`, `LegacyModuleJson`, `LegacyMeshToml`, `plugin.json`, ambiguous manifests, and replacement diagnostics.

## Deviations from Plan

None beyond the shared implementation commit noted in Plan 01.

Total deviations: 0 auto-fixed. Impact: none.

## Self-Check: PASSED

