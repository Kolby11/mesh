<!-- generated-by: gsd-doc-writer -->
# MESH

MESH is a Wayland-native shell-building platform written in Rust: its core
provides rendering, module execution, service contracts, and compositor
integration, while editable modules define the desktop experience.

MESH runs above an existing Wayland compositor. It is not a compositor, window
manager, process supervisor, or fixed desktop environment.

## Project status

MESH is under active development. The repository contains a working shell,
module graph, `.mesh` component compiler, Luau runtimes, service interfaces,
software renderer, Wayland presentation layer, language server, and shipped
example modules. Some parts of the public specification are explicitly marked
as targets and are not implemented yet.

The canonical status sources are:

- [Specification](docs/spec/README.md) for shipped and target contracts.
- [Architecture](docs/architecture/overview.md) for current code ownership and
  the agreed platform direction.
- [Backlog](docs/BACKLOG.md) for unfinished implementation work.

Historical plans and milestone evidence live under `.planning/` and are not
current product documentation.

## Core model

MESH separates four public concepts:

- A **module** is one installable unit with a canonical `module.json` manifest.
- A **component** is reusable UI authored in a `.mesh` file with Luau behavior.
- A **service** owns domain state and behavior behind a typed interface.
- A **profile** is the target composition document for root components,
  surfaces, provider choices, and profile-scoped configuration.

The Rust core supplies mechanisms: loading, validation, capability enforcement,
service transport, component execution, rendering, input, accessibility,
Wayland integration, and diagnostics. Panels, settings, developer tools,
package tooling, themes, and system integrations are replaceable modules.

```text
shell profile (target)
        │
        ▼
root component instances ──uses──► service interfaces
        │                              │
        ▼                              ▼
 .mesh + Luau                    service providers
        │                              │
        └──────────► MESH core ◄───────┘
                         │
                         ▼
                 Wayland surfaces
```

Today, the repository starts from `config/module.json`. The profile model and
live distribution switching are accepted target architecture, not shipped
behavior.

## Installation

The repository provides a Nix development shell with Rust and the required
Wayland/font libraries:

```bash
git clone git@github.com:Kolby11/mesh.git
cd mesh
nix develop
```

Without Nix, use Rust `1.85` or newer and install development packages for
Wayland, `libxkbcommon`, Fontconfig, FreeType, and `pkg-config` through the host
distribution.

MESH does not yet ship the planned module/package installer.

## Quick start

From the repository root:

```bash
nix develop -c cargo run -p mesh-tools-cli --bin mesh-shell -- start
```

Useful inspection commands are:

```bash
nix develop -c cargo run -p mesh-tools-cli --bin mesh-shell -- list
nix develop -c cargo run -p mesh-tools-cli --bin mesh-shell -- services
nix develop -c cargo run -p mesh-tools-cli --bin mesh-shell -- help
```

The shell expects a compatible Wayland session. Its current development module
graph is [config/module.json](config/module.json).

## Creating a module

Every module has one `module.json` at its root. All MESH-specific declarations
live under its `mesh` key:

```json
{
  "name": "@alice/example",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "entry": "src/main.mesh",
    "uses": {
      "interfaces": {
        "mesh.audio": ">=1.0"
      }
    }
  }
}
```

`package.json`, `mesh.toml`, and `plugin.json` are legacy inputs and are not
public manifest alternatives.

See [Getting started](docs/guides/getting-started.md), the
[module-system specification](docs/spec/01-module-system.md), and the
[`.mesh` syntax reference](docs/frontend/mesh-syntax.md).

## Workspace

The Cargo workspace is organized under `crates/`:

- `crates/core/foundation/` contains capabilities, configuration, events,
  localization, theming, diagnostics, and debug data.
- `crates/core/extension/` contains module and service contracts.
- `crates/core/ui/` contains components, elements, animation, icons, and input.
- `crates/core/frontend/` contains compilation, hosting, and rendering.
- `crates/core/runtime/` contains sandbox, scripting, and backend execution.
- `crates/core/platform/wayland/` owns Wayland integration.
- `crates/core/shell/` assembles the running development shell.
- `crates/tools/` contains the CLI and language server.

## Development

```bash
nix develop -c cargo check --workspace
nix develop -c cargo test --workspace
nix develop -c cargo fmt --all --check
```

See [Development](docs/guides/development.md) and
[Testing](docs/testing/overview.md) for focused commands and repository
conventions.

## License

Workspace crates currently declare the MIT license in their Cargo metadata.
The repository does not currently contain a root license file.
