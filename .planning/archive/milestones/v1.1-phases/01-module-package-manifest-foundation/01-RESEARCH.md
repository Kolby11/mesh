---
phase: 01
phase_name: plugin-package-manifest-foundation
status: complete
created: 2026-05-03
updated: 2026-05-03
---

# Phase 01 Research: Module Package Manifest Foundation

## Research Question

How should MESH implement `~/.mesh/package.json`, `~/.mesh/modules/`, module-level `package.json`, settings, themes, fonts, icons, i18n, provider choices, Git origin metadata, and a base shell layout entrypoint while keeping one consistent model across module types and avoiding clutter?

## Sources Checked

- npm `package.json` docs: `name` + `version` identify a package; `repository` should be a VCS URL usable by tools; dependencies can use version ranges, local paths, tarballs, and Git URLs.
  - https://docs.npmjs.com/cli/v11/configuring-npm/package-json/
- VS Code extension contribution points: one `contributes` field declares extension additions such as configuration, themes, icon themes, commands, menus, languages, and other typed capabilities.
  - https://code.visualstudio.com/api/references/contribution-points
- XDG Base Directory spec: standard Linux config/data locations are `XDG_CONFIG_HOME` and `XDG_DATA_HOME`, but this phase intentionally chooses the product-specific `~/.mesh` root. Path resolution should still reject invalid relative env override paths if overrides are added.
  - https://specifications.freedesktop.org/basedir-spec/0.8/
- JSON Schema object guidance: `additionalProperties: false` keeps top-level schemas tight; extensibility should happen through explicit extension/contribution maps, not arbitrary top-level keys.
  - https://json-schema.org/understanding-json-schema/reference/object

## Current State

MESH currently discovers plugins by scanning plugin directories and reading `plugin.json` or `mesh.toml` through `mesh-core-plugin`.

Relevant existing code:

- `crates/core/extension/plugin/src/manifest.rs` normalizes plugin manifests into `Manifest`, `PluginType`, `DependenciesSection`, `ProvidedInterface`, `EntrypointsSection`, settings, i18n, theme, assets, icon requirements, and `repository`.
- `crates/core/foundation/config/src/lib.rs` currently loads repo-local `config/shell-settings.json` first, then falls back to `~/.config/mesh/shell-settings.json`.
- `crates/core/foundation/theme/src/lib.rs` currently loads themes from repo-local `config/themes`.
- `crates/core/shell/src/shell/component.rs` currently loads per-plugin `config/settings.json` and `config/i18n/{locale}.json`.
- `Shell::spawn_backend_plugins()` groups backend plugins by service name and picks the highest-priority provider.

The existing manifest model already has most metadata needed for modules. The missing piece is a shell-owned installed-state graph rooted at `~/.mesh/package.json`, plus a compatibility bridge from current "plugin" naming to future "module" naming.

## Key Finding

Use two layers:

1. **Shell installed-state manifest:** `~/.mesh/package.json`
   - Records what the user has installed/enabled, active providers, active layout entrypoint, active theme/mode, and where module packages live.
   - Keeps user intent and shell state in one small file.

2. **Module package manifests:** `~/.mesh/modules/<module-id>/package.json`
   - Record module identity, Git origin, kind, dependencies, provided capabilities, settings schema, resources, and entrypoints.
   - Replace `plugin.json` as the target user-facing name while supporting `plugin.json` as a legacy alias during migration.

Do not create a different schema for each module type. Every module should use the same package envelope:

- common identity fields
- common source/repository fields
- common `mesh.kind`
- common `mesh.entrypoints`
- common `mesh.dependencies`
- common `mesh.provides`
- common `mesh.contributes`

Different module kinds then fill different contribution entries.

## Recommended `~/.mesh` Layout

```text
~/.mesh/
  package.json
  settings.json
  modules/
    @mesh/
      panel/
        package.json
        src/main.mesh
        config/settings.json
        config/i18n/en.json
      pipewire-audio/
        package.json
        src/main.luau
      default-theme/
        package.json
        themes/dark.json
        themes/light.json
  themes/
    custom-dark.json
    custom-light.json
```

Notes:

- `~/.mesh/package.json` is the canonical installed graph.
- `~/.mesh/settings.json` is shell settings only, not module installation state.
- `~/.mesh/themes/` is for user-authored loose theme files. Theme modules can still ship themes inside their module folder.
- `~/.mesh/modules/` contains installed module packages.
- A module's own `config/settings.json` and `config/i18n/` may remain module-scoped overrides/resources, but the package graph should know they exist through manifest contributions.

## Recommended Root Manifest Shape

Keep root-level keys few and stable:

```json
{
  "schemaVersion": 1,
  "modulesDir": "modules",
  "modules": {
    "@mesh/panel": {
      "kind": "frontend",
      "path": "modules/@mesh/panel",
      "enabled": true
    },
    "@mesh/pipewire-audio": {
      "kind": "backend",
      "path": "modules/@mesh/pipewire-audio",
      "enabled": true
    },
    "@mesh/default-theme": {
      "kind": "theme",
      "path": "modules/@mesh/default-theme",
      "enabled": true
    }
  },
  "providers": {
    "mesh.audio": "@mesh/pipewire-audio"
  },
  "layout": {
    "entrypoint": "@mesh/panel:main"
  },
  "theme": {
    "active": "@mesh/default-theme",
    "mode": "dark"
  }
}
```

Why not top-level `frontendDependencies`, `backendDependencies`, `icons`, `fonts`, and `i18n` arrays?

- They will become clutter as module kinds grow.
- They duplicate the module's own manifest.
- They force every graph algorithm to special-case each bucket.

Instead, store all installed modules under one `modules` map with a `kind`. The normalized graph can expose derived helpers:

- `frontend_modules()`
- `backend_modules()`
- `icon_modules()`
- `font_modules()`
- `language_modules()`
- `theme_modules()`

If the user-facing file needs to preserve the phrase "frontend dependencies", use it inside module manifests under `mesh.dependencies.frontend`, not as a root-level installed-state bucket.

## Recommended Module Manifest Shape

Use package.json-compatible identity at the top level, and place MESH-specific extension data under `mesh`:

```json
{
  "name": "@mesh/pipewire-audio",
  "version": "0.1.0",
  "description": "Audio backend using PipeWire",
  "license": "MIT",
  "repository": {
    "type": "git",
    "url": "git+https://github.com/mesh/pipewire-audio.git"
  },
  "mesh": {
    "apiVersion": "0.1",
    "kind": "backend",
    "entrypoints": {
      "main": "src/main.luau"
    },
    "dependencies": {
      "modules": {
        "@mesh/audio-interface": ">=1.0.0 <2.0.0"
      },
      "native": {
        "binaries": [
          {
            "name": "wpctl",
            "reason": "PipeWire volume and mute control"
          }
        ]
      }
    },
    "provides": [
      {
        "interface": "mesh.audio",
        "provider": "pipewire",
        "label": "PipeWire",
        "priority": 100
      }
    ],
    "contributes": {
      "settings": {
        "namespace": "@mesh/pipewire-audio",
        "schema": {}
      }
    }
  }
}
```

Frontend/layout module:

```json
{
  "name": "@mesh/panel",
  "version": "0.1.0",
  "repository": {
    "type": "git",
    "url": "git+https://github.com/mesh/panel.git"
  },
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "entrypoints": {
      "main": "src/main.mesh"
    },
    "dependencies": {
      "backend": {
        "mesh.audio": ">=1.0.0",
        "mesh.network": ">=1.0.0"
      },
      "icons": {
        "@mesh/material-icons": ">=1.0.0"
      },
      "i18n": {
        "@mesh/lang-en": ">=1.0.0"
      }
    },
    "contributes": {
      "layout": [
        {
          "id": "main",
          "entrypoint": "src/main.mesh"
        }
      ],
      "settings": {
        "namespace": "@mesh/panel",
        "schema": {}
      }
    }
  }
}
```

Theme/icon/font/i18n modules should use the same envelope and only differ in `mesh.kind` and `mesh.contributes`.

Example theme module:

```json
{
  "name": "@mesh/default-theme",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "theme",
    "contributes": {
      "themes": [
        {
          "id": "mesh-default",
          "label": "MESH Default",
          "modes": {
            "dark": "themes/dark.json",
            "light": "themes/light.json"
          },
          "defaultMode": "dark"
        }
      ]
    }
  }
}
```

## Why `contributes` Is the Right Pattern

The problem is not just package installation. Modules can add many kinds of things: shell surfaces, backend providers, themes, icons, fonts, language packs, settings schemas, and layout roots.

A single `mesh.contributes` object keeps that extensibility organized:

```json
{
  "mesh": {
    "kind": "frontend",
    "contributes": {
      "surfaces": [],
      "widgets": [],
      "layout": [],
      "settings": {},
      "themes": [],
      "icons": [],
      "fonts": [],
      "i18n": []
    }
  }
}
```

Rules:

- All contribution lists are optional.
- Empty contribution lists should be omitted.
- Unknown contribution keys should be rejected in Phase 1 unless explicitly placed under `mesh.experimental`.
- Contribution entries should use the same `id`, `label`, `path`, and `capabilities` conventions where applicable.

This avoids a cluttered root manifest and keeps new module types from requiring root schema surgery.

## Naming Recommendation

Recommended terminology split:

- Product/docs/config: **module**
- Legacy compatibility layer: **plugin**
- Rust internals during Phase 1: keep `Plugin*` types if renaming would be too broad
- New installed graph types: use `ModulePackage`, `InstalledModule`, `InstalledModuleGraph`, `ModuleKind`

Implementation bridge:

- `package.json` should be preferred when both `package.json` and `plugin.json` exist.
- `plugin.json` should remain loadable as a deprecated legacy alias.
- Convert both file formats into the existing normalized `Manifest` or a new `ModuleManifest` that can be losslessly built from current `Manifest`.
- Emit deprecation diagnostics or tracing warnings for `plugin.json` only after tests and fixtures are updated enough to avoid noise.

## Path Resolution Recommendation

Phase 1 should introduce a single path helper:

- `mesh_home()` defaults to `~/.mesh`
- `MESH_HOME` overrides it for tests
- reject empty or relative `MESH_HOME`
- `package_manifest_path()` -> `mesh_home()/package.json`
- `settings_path()` -> `mesh_home()/settings.json`
- `modules_dir()` -> `mesh_home()/modules`
- `themes_dir()` -> `mesh_home()/themes`

Keep repo-local `config/` fallback for tests and development fixtures, but make it explicit fallback behavior rather than the conceptual user path.

## Graph Normalization Recommendation

Implement parsing in layers:

1. `RootPackageManifest`
   - raw `~/.mesh/package.json`
   - user-authored installed-state data

2. `ModulePackageManifest`
   - raw module-level `package.json` or legacy `plugin.json`
   - package/module metadata

3. `InstalledModuleGraph`
   - normalized runtime graph
   - no JSON-specific field names leak out
   - exposes derived views by kind and provider category

Graph API should answer:

- installed module by ID
- enabled modules only
- modules by kind
- frontend dependencies
- backend providers by interface/category
- selected provider for interface/category
- fallback provider by priority
- unresolved backend requirements
- layout entrypoint module + path
- active theme + mode
- contributed resources by kind

## Validation Recommendation

Use strict validation to avoid clutter becoming ambiguity:

- root manifest allows only `schemaVersion`, `modulesDir`, `modules`, `providers`, `layout`, and `theme`
- module manifest allows common package fields plus `mesh`
- `mesh` allows only known fields plus `experimental`
- module IDs must be unique
- module `kind` must match contributed capability where required
- paths are relative to `~/.mesh` or the module root and cannot escape with `..`
- `repository.type` is `git` when `repository.url` is a Git URL
- provider selections must refer to enabled backend modules
- selected providers must actually provide the selected interface/category
- layout entrypoint must refer to an enabled frontend/layout module
- theme selection must refer to an enabled theme module or a user theme file under `~/.mesh/themes`
- icon/font/i18n dependencies resolve to enabled modules of the right kind

Use JSON Schema for fixture validation if convenient, but Rust typed validation remains the source of truth.

## Recommended Implementation Sequence

### Plan 1: Paths, Raw Schemas, and Compatibility Loader

- Add `MESH_HOME` path helper targeting `~/.mesh`
- Add raw root `package.json` structs
- Add module package loader that prefers `package.json`, falls back to `plugin.json`
- Keep old manifests loadable
- Add strict typed errors

### Plan 2: Normalized Installed Module Graph

- Build `InstalledModuleGraph`
- Normalize module kinds, dependencies, providers, contributions, theme resources, layout entrypoint
- Add derived query APIs instead of exposing raw JSON buckets
- Add fixture tests with frontend/backend/theme/icon/font/i18n examples

### Plan 3: Shell Integration Proof

- Wire shell config/theme path helpers toward `~/.mesh/settings.json` and `~/.mesh/themes`
- Add a sample `~/.mesh/package.json` fixture
- Prove active backend provider and base layout entrypoint can be resolved from graph data
- Do not rewrite lifecycle spawn yet; Phase 2 consumes graph for runtime creation

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Root `package.json` becomes cluttered with type-specific lists | Keep one `modules` map; derive typed views from `kind` and `contributes`. |
| Module kinds drift into separate schema dialects | Require one shared `mesh` envelope and one `contributes` map for all module kinds. |
| Full plugin-to-module rename explodes Phase 1 | Rename product-facing schema first; keep compatibility aliases and defer broad internal renames. |
| Git origin metadata implies downloader behavior | Store and validate repository metadata only; defer clone/fetch/install. |
| `~/.mesh` conflicts with existing docs/code using `~/.config/mesh` | Add one path helper and update docs/tests around it; keep env override for test isolation. |
| Provider selection becomes service-specific | Keep provider graph keyed by interface/category strings; no audio/network branches. |
| Layout entrypoint becomes another special case | Treat layout as a contribution and root selected entrypoint, same as themes/providers. |

## Bottom Line

Implement this as a **module graph**, not as a pile of separate dependency arrays.

The clean model is:

- `~/.mesh/package.json` says what is installed and selected.
- `~/.mesh/settings.json` says how the shell is configured.
- `~/.mesh/modules/*/package.json` says what each module is and contributes.
- `mesh.contributes` is the consistency mechanism across frontend, backend, themes, icons, fonts, i18n, settings, and layout.
- The Rust API exposes a normalized graph with typed query helpers so shell code never has to know whether a module came from a frontend bucket, icon bucket, or theme bucket.
