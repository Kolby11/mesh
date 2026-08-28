# mesh-tools-lsp

Language server for the MESH authoring formats. It speaks LSP over stdio and
serves two kinds of files:

## `.mesh` single-file components

Inside `<template>`, `<script lang="luau">`, and `<style>` blocks:

- **Completion** for core element tags and attributes, CSS properties/values,
  the `mesh.*` host API, `refs.<name>` element references, and interface proxies
  bound via `require("mesh.<service>")`.
- **Hover** documentation for elements, attributes, and service fields/commands.
- **Diagnostics** for parse errors, unknown `refs`, and invalid element fields.

Element knowledge comes from the shared `mesh-core-elements` model, and service
shapes are inferred by scanning backend `main.luau` scripts in the workspace.

## `module.json` manifests

Two manifest flavors are recognized automatically:

- **Per-module manifests** — the `name` / `version` / `mesh` envelope with
  `mesh.kind`, `mesh.apiVersion`, `mesh.uses`, `mesh.provides`, and
  `mesh.implements`.
- **The workspace root config** (`config/module.json`) — `mesh.schemaVersion`,
  `mesh.modulesDir`, `mesh.providers`, `mesh.layout`, `mesh.theme`.

For both:

- **Completion** of object keys at the cursor's path and enum values
  (`mesh.kind`, surface `anchor` / `layer` / `keyboard_mode`, ...). Capabilities
  are offered as non-binding suggestions.
- **Hover** documentation for every known key, including allowed enum values.
- **Diagnostics**: JSON syntax errors, editor-schema unknown properties and
  enum/missing-property hints, structural type mismatches, and canonical
  runtime validation for both per-module manifests and the root graph config.

`src/manifest/schema.rs` provides editor metadata for completion and hover;
parse and semantic validation comes from the runtime contracts in
`mesh-core-module` (`ModuleManifest` / `MeshModuleSection` and
`RootModuleGraphManifest`). The `tests/real_manifests.rs` guard fails if any
shipped `module.json` stops validating cleanly against either layer.

## Editor setup

The server is a generic stdio LSP. Point your editor's language client at the
`mesh-tools-lsp` binary for `*.mesh` files and canonical `module.json` files in
a MESH workspace. Legacy manifest filenames may receive migration diagnostics
but are not public alternatives. The client-supplied workspace root URI is
used to discover modules and infer interface shapes. Clients may omit
`rootUri` and provide `workspaceFolders` instead; the first file-backed folder
is used. Open-document versions and refresh generations keep rapid `didChange`
notifications ordered, so an older snapshot or diagnostic cannot replace a
newer one.
