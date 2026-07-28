---
phase: 01-plugin-package-manifest-foundation
status: passed
verified: 2026-05-03
requirements: [PINST-01, PINST-02, PINST-03, PINST-04, PINST-05, PINST-06]
---

# Verification: Phase 01

## Goal

Create the central package.json-like installed module manifest and normalized module graph that frontend/backend lifecycle and provider selection can consume in later phases.

## Result

Passed. Phase 01 now provides:

- `~/.mesh` path helpers and typed root `package.json` installed-state schema.
- Module-level `package.json` schema with Git origin metadata but no downloader behavior.
- `package.json`-first module loading with legacy `plugin.json` compatibility.
- `InstalledModuleGraph` with frontend requirements, backend providers, active/fallback provider selection, and contribution indexes.
- Repo fixtures mirroring the target `~/.mesh/package.json` and `~/.mesh/modules/*/package.json` shape.
- Shell-facing proof test for active provider and base layout entrypoint resolution.

## Requirement Coverage

- **PINST-01:** Covered by `RootPackageManifest`, `root_package_manifest_path()`, and `config/package.json`.
- **PINST-02:** Covered by module `mesh.dependencies.backend` and `requirements_for_frontend`.
- **PINST-03:** Covered by `ModuleKind::Backend` and `mesh.provides` provider declarations.
- **PINST-04:** Covered by two `mesh.audio` providers plus explicit root provider selection.
- **PINST-05:** Covered by `InstalledModuleGraph` and `load_installed_module_graph`.
- **PINST-06:** Covered by metadata-only repository fields and static no-download checks.

## Checks Run

- `nix develop -c cargo test -p mesh-core-plugin module_package` - passed.
- `nix develop -c cargo test -p mesh-core-plugin -p mesh-core-shell installed_module_graph` - passed.
- `nix develop -c cargo test -p mesh-core-config -p mesh-core-theme` - passed.
- `grep -R "RootPackageManifest\|ModulePackageManifest\|ModuleKind\|load_module_manifest\|mesh_home" -n crates/core/extension/plugin/src` - expected symbols found.
- `grep -R "\"@mesh/pipewire-audio\"\|\"@mesh/panel:main\"\|\"mesh.audio\"\|\"mesh.network\"\|\"mesh.power\"" -n config/package.json config/modules` - expected fixture values found.
- `grep -R "~/.mesh/settings.json\|~/.mesh/themes\|mesh.contributes.themes" -n docs/settings/README.md docs/theming/themes.md crates/core/foundation/config/src/lib.rs crates/core/foundation/theme/src/lib.rs` - expected doc/path direction found in docs; code uses helper-based `~/.mesh` fallbacks.
- `grep -R "git clone\|git fetch\|marketplace\|signature\|download" -n crates/core/extension/plugin/src/package.rs crates/core/shell/src/shell/mod.rs` - no matches.

## Review Gate

`01-REVIEW.md` status is clean. The review gate caught and fixed one fixture acceptance gap before final verification: quick settings now declares `mesh.audio`, `mesh.network`, and `mesh.power`.

## Residual Risk

Runtime backend lifecycle still uses existing plugin discovery. That is intentional: Phase 2 consumes this graph for deterministic backend lifecycle and provider selection.
