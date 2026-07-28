# Phase 41: Shipped Module Proof and Documentation - Patterns

## Closest Existing Analogs

| Target Work | Existing Analog | Pattern To Reuse |
|-------------|-----------------|------------------|
| Real installed graph proof | `installed_module_graph_loads_repo_module_fixture` in `crates/core/extension/module/src/package/tests.rs` | Load `config/module.json` from the workspace root and assert typed graph state from shipped manifests. |
| Contribution proof | `contribution_index_exposes_frontend_keybind_resource_interface_and_provider_records` | Assert typed getters for frontend entrypoints, settings schemas, keybind actions, icon requirements, icon packs, declared interfaces, and backend providers. |
| Diagnostics proof | `contribution_index_reports_resource_and_settings_compatibility_diagnostics` | Assert diagnostic `status`, `module_id`, and `contribution_id` without making compatibility gaps fatal. |
| Shell graph proof | `installed_module_graph_exposes_shell_package_choices` and `backend_lifecycle_uses_explicit_active_provider_from_package_graph` in `crates/core/shell/src/shell/tests.rs` | Prove runtime selection through installed graph records and generic backend candidates. |
| Keybind surface proof | Phase 40 navigation interaction tests | Focus on manifest-declared keybind ids and metadata, not a new keybind runtime feature. |
| Author workflow docs | `docs/module-system.md` and Phase 40 docs updates | Use canonical `module.json` and `mesh.*` vocabulary with grep-verifiable strings. |

## File Roles

- `config/module.json`: root installed-module graph for the shipped proof path.
- `modules/frontend/navigation-bar/module.json`: canonical frontend manifest
  with layout entrypoint, settings contribution, keybind action, icon
  requirements, and frontend dependencies.
- `modules/interfaces/audio.toml`: existing interface contract source that
  Phase 41 should preserve while creating a canonical `@mesh/audio-interface`
  module directory for installed-graph loading.
- `modules/interfaces/audio/module.json`: canonical interface module manifest
  to add for the proof path.
- `modules/interfaces/audio/interface.toml`: graph-loadable copy of the
  `mesh.audio` contract to add inside the interface module directory.
- `modules/backend/pipewire-audio/module.json`: active `mesh.audio` backend
  provider.
- `modules/backend/pulseaudio-audio/module.json`: alternate `mesh.audio`
  provider proving provider multiplicity.
- `modules/icon-packs/default/module.json`: default icon pack to convert to
  canonical shape and install in `config/module.json` for navigation icon
  requirement proof.
- `crates/core/extension/module/src/package/installed_graph.rs`: typed graph,
  contribution indexes, provider records, diagnostics, and getters.
- `crates/core/extension/module/src/package/tests.rs`: primary package proof
  target.
- `crates/core/shell/src/shell/discovery.rs`: graph-to-shell startup bridge.
- `crates/core/shell/src/shell/tests.rs`: shell runtime proof target.
- `docs/module-system.md`: canonical author workflow.
- `docs/modules/frontend/core/navigation-bar/README.md`: shipped frontend
  proof page.
- `docs/modules/backend/core/pipewire-audio/README.md` and
  `docs/modules/backend/core/pulseaudio-audio/README.md`: shipped backend
  provider proof pages.
- `docs/settings/README.md`: settings/keybind override boundary.
- `docs/llm-context.md`: AI-facing module workflow summary.

## Data Flow

Real proof path:

`config/module.json` -> `load_installed_module_graph` -> shipped
`module.json` manifests -> `InstalledModuleGraph` -> typed frontend
requirements, provider records, contribution records, diagnostics, layout
entrypoint -> shell discovery/runtime tests -> author docs.

Author workflow:

frontend `module.json` requires interface and contributes layout/settings/keybind
metadata -> interface contract defines `mesh.audio` -> backend providers
declare `mesh.implements` -> root graph chooses active provider -> graph
diagnostics explain missing resources or incompatible declarations.

## Landmines

- Do not introduce a toy module as the primary proof; it can only support an
  isolated diagnostic edge case.
- Do not treat `service.audio.*` capabilities as a replacement for interface
  dependencies or backend provider declarations.
- Do not use old public names such as package/plugin/contract in the happy path
  except where a document explicitly discusses migration diagnostics.
- Do not update unrelated dirty UI/icon/theme files for this phase.
- Do not rely on a plain host `cargo test` for shell tests when Nix native deps
  are required.
