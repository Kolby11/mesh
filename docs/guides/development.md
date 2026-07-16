<!-- generated-by: gsd-doc-writer -->
# Development

## Local setup

```bash
git clone git@github.com:Kolby11/mesh.git
cd mesh
nix develop
cargo check --workspace
```

The Nix shell provides Rust, `rustfmt`, Clippy, native Wayland/font libraries,
and Linux profiling tools.

## Build and run commands

| Command | Purpose |
| --- | --- |
| `cargo check --workspace` | Type-check the complete Cargo workspace |
| `cargo build --workspace` | Build all workspace crates |
| `cargo run -p mesh-tools-cli --bin mesh-shell -- start` | Run the development shell |
| `cargo run -p mesh-tools-cli --bin mesh-shell -- help` | Show current CLI commands |
| `cargo run -p mesh-tools-lsp --bin mesh-tools-lsp` | Run the language server over stdio |
| `cargo fmt --all --check` | Check Rust formatting |
| `cargo clippy --workspace --all-targets` | Run Clippy across workspace targets |
| `cargo test --workspace` | Run the workspace test suite |
| `./tools/check-performance` | Compare configured performance workloads with stored tolerances |

Prefix commands with `nix develop -c` when not already inside the development
shell.

## Code style

Rust code follows `rustfmt`. The workspace uses Rust edition 2024 and declares
Rust `1.85` as its minimum version. Luau formatting in the LSP is backed by
StyLua. Preserve established crate boundaries and avoid introducing
service-specific behavior into generic core crates.

## Working with modules

- Keep the canonical manifest at the module root as `module.json`.
- Put MESH declarations under the top-level `mesh` key.
- Frontends depend on interface contracts, not backend provider IDs.
- Service-specific state derivation belongs in modules, not Rust core branches.
- Public module text should use localization records where the schema requires
  them.
- Add focused tests for manifest normalization, interface validation, component
  compilation, and real shipped-surface behavior as appropriate.

## Documentation rules

- `docs/spec/` defines public shipped and target contracts.
- `docs/` describes current implementation and author/maintainer workflows.
- `.planning/` contains historical plans, evidence, and design records.
- `docs/BACKLOG.md` is the single unfinished-work backlog.

When a specification includes unshipped behavior, mark it `Target`. Do not put
future behavior in current guides without the same qualification.

## Branches and pull requests

No repository-specific branch naming or pull-request template is currently
defined. Keep changes scoped, include verification evidence, and avoid combining
unrelated dirty-tree changes in one commit.
