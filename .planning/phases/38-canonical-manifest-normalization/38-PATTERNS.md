# Phase 38: Canonical Manifest Normalization - Patterns

## Closest Existing Analogs

| Target Work | Existing Analog | Pattern To Reuse |
|-------------|-----------------|------------------|
| Canonical module manifest parsing | `crates/core/extension/module/src/package/module_manifest.rs` | serde structs with `from_json_str`, `from_path`, `validate`, and `into_runtime_manifest` methods. |
| Legacy manifest migration | `ModulePackageManifest::from_legacy_manifest()` | Convert old runtime `Manifest` into canonical module-shaped data before graph indexing. |
| Root graph manifest parsing | `crates/core/extension/module/src/package/root.rs` | Parse wrapper and direct shapes into one normalized root graph type, then validate. |
| Installed graph loading | `crates/core/extension/module/src/package/installed_graph.rs` | Load root graph, load each module manifest, then call `InstalledModuleGraph::from_parts`. |
| Runtime fallback loader | `crates/core/extension/module/src/manifest/load.rs` | Source-specific parsing helpers returning `LoadedManifest`. |
| Path defaults | `crates/core/extension/module/src/package/paths.rs` | Small helper functions returning `Result<PathBuf, ModuleManifestError>`. |
| Shell root graph integration | `crates/core/shell/src/shell/discovery.rs`, `crates/core/shell/src/shell/backend/spawn.rs` | Build `workspace_root`, join config path, call `load_installed_module_graph`, fall back to legacy discovery on error. |
| Validation tests | `crates/core/extension/module/src/package/tests.rs`, `crates/core/extension/module/src/manifest/tests.rs` | Temporary directories and fixture-based assertions. |

## File Roles

- `module_manifest.rs`: canonical module schema and legacy-to-canonical
  conversion. Rename types here first.
- `root.rs`: root installed-module graph schema. Rename root types and keep the
  direct root graph shape canonical.
- `error.rs`: diagnostic/error vocabulary. Add field-path and migration action
  support here.
- `installed_graph.rs`: canonical/legacy source detection, ambiguity handling,
  diagnostics threading, and installed graph loading.
- `manifest/load.rs`: compatibility runtime loader for canonical module JSON,
  legacy module JSON, legacy package JSON, and mesh.toml.
- `manifest/model.rs`: normalized runtime model. Rename `PackageSection` to
  `ModuleSection` and source variants.
- `package/tests.rs` and `manifest/tests.rs`: primary behavioral contract for
  Phase 38.
- `config/module.json` and `modules/**/module.json`: shipped canonical fixtures.
- shell tests and runtime files: update hard-coded `config/package.json`.

## Data Flow

Canonical module path:

`module.json` -> `ModuleManifest::from_path` -> `ModuleManifest::validate` ->
`ModuleManifest::into_runtime_manifest` -> `InstalledModuleGraph::from_parts`.

Legacy module path:

old `module.json` / `package.json` / `mesh.toml` -> legacy parser ->
`Manifest` -> `ModuleManifest::from_legacy_manifest` -> migration diagnostic ->
`InstalledModuleGraph::from_parts`.

Root graph path:

`config/module.json` -> `RootModuleGraphManifest::from_path` -> root
validation -> module manifest loads -> installed graph validation.

## Landmines

- Do not add `pub type ModulePackageManifest = ModuleManifest`; Phase 38 locked
  no public compatibility aliases.
- Do not silently pick between `module.json` and `package.json`.
- Do not mistake legacy `module.json` for canonical `module.json`; inspect shape
  or parse canonical first with a clear legacy fallback only for known old keys.
- Do not rename OS package names under binary dependency `packages`.
- Do not make Phase 38 responsible for full provider conflict resolution or
  resource cascade policy; preserve the data for Phase 39.

