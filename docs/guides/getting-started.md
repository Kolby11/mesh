<!-- generated-by: gsd-doc-writer -->
# Getting Started

## Prerequisites

- Linux with a compatible Wayland compositor/session.
- Rust `1.85` or newer.
- Wayland, `libxkbcommon`, Fontconfig, FreeType, and `pkg-config` development
  libraries.
- Nix with flakes enabled is the supported repository setup path and provides
  the required development dependencies.

## Install the source tree

```bash
git clone git@github.com:Kolby11/mesh.git
cd mesh
nix develop
```

MESH does not currently provide the planned end-user module installer. This
workflow runs the development shell from source.

## First run

Inside a compatible Wayland session:

```bash
nix develop -c cargo run -p mesh-tools-cli --bin mesh-shell -- start
```

The current shell reads the repository module graph from `config/module.json`
and module source from `modules/`.

## Inspect the module graph

```bash
nix develop -c cargo run -p mesh-tools-cli --bin mesh-shell -- list
nix develop -c cargo run -p mesh-tools-cli --bin mesh-shell -- services
```

## Edit a shipped component

Frontend modules live under `modules/frontend/`. Each contains a `module.json`
and a `.mesh` entrypoint. For example, the navigation bar entry is
`modules/frontend/navigation-bar/src/main.mesh`.

The shell has file-watch and reload infrastructure, but reload behavior depends
on the changed resource and current runtime state. Restart the development shell
when a change is not reflected.

## Common setup issues

### Native library linking fails

Use `nix develop` so `pkg-config` and the Wayland/font libraries are available
through the flake environment.

### No surface appears

Confirm the process is running inside a Wayland session and inspect the selected
root entrypoint and disabled modules in `config/module.json`.

### A service is unavailable

Run the `services` command and check that the selected provider's required
binaries and capabilities are available. Provider requirements are declared in
its `module.json`.

## Next steps

- [Development](development.md)
- [Testing](../testing/overview.md)
- [Configuration](../configuration/overview.md)
- [`.mesh` syntax](../frontend/mesh-syntax.md)
- [Module-system specification](../spec/01-module-system.md)
