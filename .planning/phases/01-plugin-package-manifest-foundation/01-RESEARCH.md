---
phase: 01
phase_name: plugin-package-manifest-foundation
status: complete
created: 2026-05-03
---

# Phase 01 Research: Plugin Package Manifest Foundation

## Research Question

What does the planner need to know to implement a local, package.json-like installed-plugin manifest and normalized plugin graph for MESH?

## Current State

MESH currently discovers plugins by scanning plugin directories and reading each plugin's `plugin.json` or `mesh.toml` through `mesh-core-plugin`. Backend provider selection happens inside `Shell::spawn_backend_plugins()`: providers are grouped by service name, sorted by manifest priority, and the highest-priority candidate is spawned.

Existing manifest capabilities already provide much of the metadata needed for a package graph:

- `PluginType` distinguishes `surface`, `backend`, `interface`, `icon-pack`, and related plugin types.
- `Manifest::declared_provides()` normalizes legacy `service` declarations and new-style `provides` declarations.
- `DependenciesSection.plugins` already maps plugin IDs to version specs.
- Frontend core surfaces already declare interface dependencies such as `@mesh/audio-interface`.
- Backend audio providers already demonstrate same-category alternatives: `@mesh/pipewire-audio` and `@mesh/pulseaudio-audio` both provide `mesh.audio`.

## Recommended Shape

Implement the package manifest inside `mesh-core-plugin` as installed plugin metadata, not inside `mesh-core-config`. The package graph needs to depend on plugin concepts like `PluginType`, `ProvidedInterface`, and dependency specs; keeping it in the extension/plugin crate avoids making the foundation config crate depend upward on extension types.

The active user-facing file can live under `config/plugins.json` for repo defaults/tests and later resolve to an XDG config path. The planner should keep naming isolated behind a path helper so future packaging work can rename or relocate it without changing graph semantics.

Suggested JSON shape:

```json
{
  "version": 1,
  "plugins": {
    "frontend": [
      {
        "id": "@mesh/panel",
        "source": "packages/plugins/frontend/core/panel",
        "requires": {
          "backend_categories": ["audio", "network", "power"],
          "plugins": ["@mesh/audio-interface"]
        }
      }
    ],
    "backend": [
      {
        "id": "@mesh/pipewire-audio",
        "source": "packages/plugins/backend/core/pipewire-audio",
        "category": "audio",
        "provides": "mesh.audio",
        "default": true
      }
    ]
  },
  "providers": {
    "audio": {
      "active": "@mesh/pipewire-audio"
    }
  }
}
```

The exact field names can change during implementation, but the graph must represent:

- installed frontend plugin IDs and source paths
- installed backend plugin IDs and source paths
- frontend dependencies on backend categories and/or plugin IDs
- backend category/service metadata
- active provider choice per category

## Implementation Notes

### Parser and Validation

Use `serde::Deserialize` and `serde_json`. Return typed errors with enough context to identify duplicate plugin IDs, empty IDs, unknown active provider choices, and mismatched active provider category.

Validation should be local and deterministic:

- plugin IDs must be non-empty
- plugin IDs should be unique across frontend and backend sections
- backend categories must be non-empty
- active provider IDs must reference installed backend plugins
- active provider category must match the provider section key
- frontend dependency categories are allowed even when no installed provider exists yet, but graph resolution should expose them as unresolved requirements

### Normalized Graph

The graph should be a separate runtime structure from the raw manifest parse result. Raw manifest keeps user-authored data; graph exposes normalized answers:

- `frontend_plugins`
- `backend_plugins`
- `backend_by_category`
- `active_provider(category)`
- `requirements_for_frontend(plugin_id)`
- `unresolved_backend_categories()`

### Shell Integration Boundary

Phase 1 should not rewrite backend task spawning. It should provide shell-facing parsing and graph APIs plus a small shell/config integration proof. Phase 2 can consume the graph in lifecycle spawning.

## Validation Architecture

Validation should be Rust unit tests plus a small fixture parse test. No live Wayland or system binaries are needed.

Recommended automated checks:

- `nix develop -c cargo test -p mesh-core-plugin package`
- `nix develop -c cargo test -p mesh-core-shell plugin_package`
- grep checks for `InstalledPluginPackage`, `InstalledPluginGraph`, `active_provider`, and `config/plugins.json`

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Schema turns into remote package manager too early | Keep Phase 1 local-only; defer download/signing/marketplace. |
| Duplicates or mismatched provider choices silently select wrong backend | Typed validation errors for duplicates, unknown provider IDs, and category mismatch. |
| Shell continues to bypass the graph | Add a shell-facing loader/proof test even if lifecycle consumption waits for Phase 2. |
| Frontend dependencies are confused with current plugin manifest dependencies | Keep package-level installed graph separate from individual `plugin.json` manifests. |

## Planning Recommendation

Use three plans:

1. Raw package manifest schema, parser, validation, and default path helper.
2. Normalized installed plugin graph and dependency/provider resolution.
3. Shell-facing loader, sample `config/plugins.json`, and proof tests that active providers can be derived from package data.
