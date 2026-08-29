# Configuration

## Current development configuration

The checked-in shell currently uses these files:

| File | Purpose |
| --- | --- |
| `config/module.json` | Installed module directory, disabled modules, provider choices, and root layout |
| `config/settings.json` | Every user setting, in one namespaced document |
| `config/icons.toml` | Current semantic icon profiles and fallback candidates |

The root graph is also a canonical `module.json`; `mesh.schemaVersion`
distinguishes it from an installable module manifest.

The two split them: the **root graph** decides which modules exist and which
provider implements each interface; the **settings file** holds preference
values for the modules that do. See [Settings](../spec/08-settings.md).

## The settings file

One document, keyed by namespace. `shell` holds core preferences; every other
top-level key is a module id (`@scope/name`) or an interface id (`mesh.audio`):

```json
{
  "schemaVersion": 1,
  "shell": {
    "theme": { "active": "gruvbox-dark" },
    "icons": { "default_pack": "@mesh/icons-material-symbols" }
  },
  "@mesh/navigation-bar": {
    "surface": { "anchor": "bottom" },
    "props": { "global": { "density": "compact" } }
  }
}
```

It is **sparse**: a key exists only where the user changed something. Defaults
come from the module's own `module.json` (`mesh.surface`) and its `<props>`
declarations, never from a copy in this file — so a module that changes its
defaults in an update still reaches a user who never overrode them. Deleting a
key restores the declared default.

Because the file is sparse, a module you have never configured has no entry to
edit. `mesh-shell config eject <module-id>` writes that module's *current,
effective* surface placement in, ready to hand-edit:

```console
$ mesh-shell config eject @mesh/quick-settings
$ mesh-shell config show @mesh/quick-settings
$ mesh-shell config reset @mesh/quick-settings   # back to declared defaults
```

Ejected values are pinned by definition — they are now user overrides, so later
changes to the module's own defaults no longer reach them. Author-only manifest
capabilities such as `mesh.surface.promotable` remain in the module manifest and
are not emitted into user settings.

Settings are watched: editing the file re-applies theme, locale, surface
placement, and module props to the running shell without a restart.

### Editor support

`mesh-tools-lsp` serves `config/settings.json` (and `$MESH_HOME/settings.json`)
as well as module manifests. It completes namespaces, keys, and enum values,
documents them on hover, and reports unknown keys, wrong types, and invalid
enum values as you type — the same rules the store applies at load time, from
the same field tables.

Values that depend on what is installed are discovered from the workspace and
offered as suggestions: themes for `shell.theme.active`, locales with a catalog
for `shell.i18n.locale`, icon packs for `shell.icons.default_pack` and
`<module>.icons.use_packs`, and installed module and interface ids as
top-level namespaces. Suggestions are never enforced — a theme or pack may
live outside the scanned workspace — so an unrecognized one is offered no
warning, while an invalid enum value still is.

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
module's namespace in `config/settings.json` under `props.global`, or under
`props.instances` for one component instance. For example:

```json
{
  "@mesh/navigation-bar": {
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
| `MESH_SETTINGS_PATH` | Overrides the settings file path |
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
