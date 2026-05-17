# Phase 39: Contribution and Interface Extension Index - Patterns

## Closest Existing Analogs

| Target Work | Existing Analog | Pattern To Reuse |
|-------------|-----------------|------------------|
| Installed typed graph | `crates/core/extension/module/src/package/installed_graph.rs` | Load root graph, validate module manifests, collect typed runtime registries from enabled modules, expose read-only getters. |
| Manifest validation | `crates/core/extension/module/src/package/module_manifest.rs` | Small `validate()` methods with blocking `ModuleManifestError::Validation` strings for invalid author input. |
| Interface contract/provider catalog | `crates/core/extension/service/src/interface.rs` | Canonical interface names, sorted providers, replacement by provider module/version, resolve by requested version. |
| Backend launch diagnostics | `crates/core/shell/src/shell/backend/candidates.rs` | Return candidates plus `BackendLifecycleStatusRecord` diagnostics instead of panicking or silently ignoring invalid graph state. |
| Shell graph fallback | `crates/core/shell/src/shell/discovery.rs` and `backend/spawn.rs` | Try `config/module.json`; warn and fall back to legacy discovery if graph loading fails. |
| Resource contribution records | existing `ContributedTheme`, `ContributedPathResource`, `ContributedI18n`, `ContributedLibrary`, `ContributedSettingsSchema` | Preserve typed structs and add common source metadata rather than collapsing into an enum-only list. |
| Phase 38 loader diagnostics | `LoadedModuleManifest`, `ModuleManifestDiagnostic`, `ModuleManifestSource` | Carry source path/source kind forward so contribution diagnostics can point at the manifest that produced them. |

## File Roles

- `crates/core/extension/module/src/package/installed_graph.rs`: primary owner
  for installed graph nodes, typed contribution registries, active provider
  validation, interface guidance, resource requirement diagnostics, and graph
  getters.
- `crates/core/extension/module/src/package/module_manifest.rs`: validation
  invariants for interface relationships, provider declarations, dependencies,
  capabilities, and contribution shapes.
- `crates/core/extension/module/src/package/tests.rs`: primary package-level
  contract tests for graph construction, typed registry contents, disabled
  module behavior, and manifest validation errors.
- `crates/core/extension/service/src/interface.rs`: stable interface catalog
  behavior; extend only if graph-derived provider metadata needs a public shape.
- `crates/core/shell/src/shell/discovery.rs`: bridge graph-derived interface
  contracts/providers and frontend entrypoint filtering into shell startup.
- `crates/core/shell/src/shell/backend/candidates.rs`: backend provider status,
  provider/contract/capability diagnostics, and launch candidate construction.
- `crates/core/shell/src/shell/tests.rs`: integration proof that shell startup
  and backend launch use graph metadata without service-specific branches.
- `docs/module-system.md`, `docs/module-vocabulary.md`, `docs/icon-system.md`:
  reference vocabulary only when tests or diagnostics need wording; full author
  docs are Phase 40.

## Data Flow

Installed graph loading:

`config/module.json` -> `RootModuleGraphManifest::from_path` ->
`load_module_manifest` per installed module -> `LoadedModuleManifest` with
manifest path/source -> `InstalledModuleGraph::from_parts`.

Runtime typed indexing:

enabled `InstalledModuleNode` -> `ContributionSource` -> typed registries for
frontend entrypoints, layout, settings, keybinds, resources, interfaces,
providers, and libraries -> read-only graph getters used by shell/tools.

Interface/provider validation:

interface module `mesh.interface` -> `InterfaceDeclarationNode` and guidance ->
backend module `mesh.implements`/legacy `mesh.provides` ->
`BackendProviderNode` -> active provider mapping -> shell/backend candidate
diagnostics -> `InterfaceRegistry` contract/provider cross-check.

Resource/settings inspection:

frontend dependencies and module resource requirements -> graph requirement
records -> selected/contributed icon/font/i18n/theme/settings records ->
non-fatal diagnostics for missing resource names or packs.

## Landmines

- Do not infer host capabilities from provider identity or frontend dependency
  names. Capabilities stay in `mesh.capabilities`.
- Do not make independent interfaces invalid just because a base interface for
  the same domain exists. Emit guidance only.
- Do not index disabled modules into runtime contribution getters. Disabled
  modules may remain visible through installed catalog/node metadata.
- Do not add service-specific shell branches for `audio`, `network`, or any
  other concrete interface. Tests should use generic interface ids where
  possible.
- Do not collapse typed registries into only `serde_json::Value`; tools need
  stable typed fields.
- Do not finish Phase 40 documentation here except for minimal wording needed by
  tests or diagnostics.
