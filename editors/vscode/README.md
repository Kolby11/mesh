# MESH Language (VS Code)

Client extension that runs [`mesh-tools-lsp`](../../crates/tools/lsp) and wires
it to:

- `.mesh` single-file components (template / script / style highlighting plus
  completion, hover, and diagnostics from the server).
- `module.json` and `package.json` manifests (schema completion, hover, and
  diagnostics).

## Setup

1. Build the language server:

   ```sh
   cargo build --release -p mesh-tools-lsp
   ```

2. Build the extension bundle:

   ```sh
   cd editors/vscode
   npm install
   npm run build
   ```

3. Install it into VS Code (symlink keeps it live across rebuilds):

   ```sh
   ln -sfn "$PWD" ~/.vscode/extensions/mesh-language
   ```

   Then reload VS Code (`Developer: Reload Window`).

## Configuration

- `mesh.lsp.serverPath` — absolute path to the `mesh-tools-lsp` binary. When
  empty (default), the extension looks in the workspace's `target/release` and
  `target/debug` directories, then on `PATH`.
- `mesh.lsp.trace.server` — set to `verbose` to log LSP traffic in the
  "MESH Language Server" output channel.

## Rebuilding after a server change

Rebuild the binary (`cargo build --release -p mesh-tools-lsp`) and reload the
window. After changing the extension's TypeScript, re-run `npm run build` (or
`npm run watch`) and reload.
