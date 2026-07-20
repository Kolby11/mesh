<!-- generated-by: gsd-doc-writer -->
# Configuration

## Current development configuration

The checked-in shell currently uses these files:

| File | Purpose |
| --- | --- |
| `config/module.json` | Installed module directory, disabled modules, provider choices, and root layout |
| `config/shell-settings.json` | Development user overrides for theme, locale, and icons |
| `config/settings-default.json` | Bundled shell setting defaults |
| `config/icons.toml` | Current semantic icon profiles and fallback candidates |

The root graph is also a canonical `module.json`; `mesh.schemaVersion`
distinguishes it from an installable module manifest.

## Current user paths

Module-path helpers currently default `MESH_HOME` to `~/.mesh`. Under that
directory, the module package layer expects:

```text
~/.mesh/
├── module.json
├── modules/
├── settings.json
└── themes/
```

The running development shell currently resolves its installed graph from the
repository `config/module.json`, so the complete dotfiles/profile design is not
yet wired through the shell entrypoint.

## Frontend visual-effect settings

Frontend modules declare editable visual knobs in their root `<props>` block.
Author defaults may reference theme tokens; user overrides are stored in that
module's `config/settings.json` under `props.global`, or under
`props.instances` for one component instance. For example:

```json
{
  "props": {
    "global": {
      "blur_enabled": true,
      "blur_radius": "18px",
      "blur_background": "rgba(24, 26, 34, 0.28)"
    },
    "instances": {
      "@mesh/navigation-bar/import:audio": {
        "blur_radius": "24px"
      }
    }
  }
}
```

The shipped themes provide `--effect-backdrop-blur-*-radius` and
`--effect-backdrop-blur-*-background` defaults. Component CSS decides which
elements emit compositor blur regions. Compositor-wide kernel quality (for
example Hyprland's blur size and pass count) remains compositor configuration;
the Wayland blur protocol carries regions but no per-surface kernel settings.

## Environment variables

| Variable | Current use |
| --- | --- |
| `MESH_HOME` | Overrides the module/configuration home; the module loader requires an absolute path |
| `MESH_SETTINGS_PATH` | Overrides the user shell settings JSON |
| `MESH_SETTINGS_DEFAULTS_PATH` | Overrides bundled defaults JSON |
| `MESH_THEME_DIR` | Overrides the theme directory |
| `MESH_IPC_SOCKET` | Overrides the Unix IPC socket path |
| `MESH_BACKEND` | Forces a presentation backend where supported |
| `RUST_LOG` | Controls tracing filters through `tracing-subscriber` |

## Module manifests

Every installable module uses `module.json`:

```json
{
  "name": "@alice/example",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "entry": "src/main.mesh"
  }
}
```

See [Module System](../spec/01-module-system.md) for the complete shipped and
target schema.

## Target profile configuration

The accepted target introduces named shell profiles stored with editable module
source. Profiles will define root component instances, surface placement,
ambiguous provider bindings, resources, root services, and profile-specific
overrides. Component dependencies will infer required services.

Configuration will use layered scope:

1. module-declared default;
2. shared user default;
3. profile override;
4. component-instance override.

Durable service data will remain service-owned and shared unless the service
declares another scope. The exact profile file schema and migration from the
current repository graph remain backlog work.
