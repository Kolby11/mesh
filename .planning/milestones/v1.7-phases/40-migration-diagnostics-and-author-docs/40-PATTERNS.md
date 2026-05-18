# Phase 40: Migration Diagnostics and Author Docs - Patterns

## Closest Existing Analogs

| Target Work | Existing Analog | Pattern To Reuse |
|-------------|-----------------|------------------|
| Manifest diagnostic contract | `crates/core/extension/module/src/package/error.rs` | Structured diagnostic record with severity, path, optional module id, optional field path, message, and suggested action. |
| Manifest migration loader | `crates/core/extension/module/src/package/installed_graph.rs` | Detect unsupported/legacy manifest files before graph indexing and return diagnostics with replacement wording. |
| Loader diagnostics tests | `crates/core/extension/module/src/package/tests.rs` | Temp-directory manifest fixtures with string assertions on diagnostic messages and suggested actions. |
| Keybind runtime declaration | `crates/core/extension/module/src/manifest/model.rs` | `KeybindAction` stores scope, label, description, category, trigger, and localized triggers. |
| Shell keybind fallback | `crates/core/shell/src/shell/component/input/keyboard.rs` | Manifest declarations are primary; legacy settings-derived declarations are appended only for missing ids. |
| Author docs model | `docs/module-system.md` and `docs/module-vocabulary.md` | Canonical `module.json` plus `mesh` sections, old manifest names as replacement/internal migration only. |
| Docs migration sweep | Phase 37 and 38 docs plans | Grep-verifiable string replacements in bounded docs files, preserving resource aliases and OS package names. |

## File Roles

- `crates/core/extension/module/src/package/error.rs`: severity and diagnostic
  record shape for blocking errors and migration warnings.
- `crates/core/extension/module/src/package/installed_graph.rs`: module
  manifest source detection, loaded-manifest diagnostics, graph diagnostics,
  and typed contribution records.
- `crates/core/extension/module/src/package/tests.rs`: primary regression
  suite for manifest diagnostics and installed-graph keybind contribution data.
- `crates/core/extension/module/src/manifest/model.rs`: source of truth for
  keybind action fields to preserve in graph records.
- `crates/core/shell/src/shell/component/input/keyboard.rs`: shortcut
  resolution behavior that must remain compatible with v1.6.
- `docs/module-system.md`: canonical author contract and migration diagnostics
  table.
- `docs/module-vocabulary.md`: inventory and handoff source for old terms.
- `docs/installation.md`, `docs/font-system.md`, `docs/theming/themes.md`,
  `docs/theming/locales.md`, `docs/settings/README.md`, `docs/llm-context.md`:
  stale author-doc surfaces that still teach old manifest names or shapes.

## Data Flow

Manifest migration diagnostics:

legacy file on disk -> `load_module_manifest` source detection ->
`ModuleManifestDiagnostic` warning or error -> `LoadedModuleManifest.diagnostics`
or `ModuleManifestError::Diagnostic` -> installed graph diagnostics/tests ->
author docs.

Keybind migration continuity:

canonical `module.json` -> `ModuleManifest.mesh.keybinds.actions` ->
`ModuleContributionIndex.keybinds` -> typed graph getter -> future dispatch,
conflict, accessibility, and settings UI phases.

Shell shortcut fallback:

manifest keybind declarations -> resolved shortcut with optional user override
-> legacy settings declarations only for action ids absent from the manifest.

## Landmines

- Do not use `alias`, `synonym`, or `compatible name` to describe old MESH
  manifest names. Use `replace`, `remove`, or `internal migration input`.
- Do not rewrite operating-system package names under binary dependency
  suggestions; those are not MESH module names.
- Do not rename resource resolver aliases when they are icon/font lookup
  mechanics rather than old module vocabulary.
- Do not make `settings.keyboard.surface_shortcuts` the declaration format.
  It is user override data for manifest-declared actions.
- Do not start Phase 41 shipped proof work in Phase 40. Phase 40 prepares docs,
  diagnostics, and keybind continuity for that proof.

