# Architecture Research: v1.7 Modularity and Extensibility

## Current Integration Points

- `crates/core/extension/module/src/package/module_manifest.rs` parses the target `package.json` shape and converts it into the runtime manifest model.
- `crates/core/extension/module/src/manifest/model.rs` is the normalized runtime manifest model, but it still exposes several organically grown sections and legacy compatibility concepts.
- `crates/core/extension/module/src/package/installed_graph.rs` builds the installed module graph, active providers, interface declarations, contribution index, and layout entrypoint.
- `crates/core/extension/service/src/lib.rs` and its submodules own interface contracts, interface/provider resolution, and registry behavior.
- `docs/module-system.md`, `docs/extensibility.md`, `docs/modules/README.md`, backend/frontend module docs, and codebase maps already describe most of the desired model, but the docs and structs are not yet fully aligned.
- v1.6 added keybind declaration and locale-resolution concepts that need to fit the broader contribution/capability vocabulary before dispatch and accessibility proof continue.

## Suggested Build Order

1. Inventory and freeze vocabulary before changing schemas.
2. Normalize manifest/package models around a canonical `mesh` schema while keeping compatibility loaders.
3. Unify typed contribution indexing and validation for declarations that already exist in separate sections.
4. Add migration and compatibility diagnostics so older docs/examples keep loading visibly.
5. Prove the model with real bundled module paths and author documentation.

## New vs Modified Components

| Area | Work Type | Notes |
|------|-----------|-------|
| Manifest schema | Modified | Consolidate existing fields, add canonical docs/schema, preserve load compatibility. |
| Installed graph | Modified | Ensure contribution index covers the normalized extension points and exposes useful diagnostics. |
| Interface registry | Modified | Align interface/provider declarations with dependency/capability vocabulary. |
| Diagnostics | Modified/New | Add author-facing migration and validation diagnostics. |
| Docs/examples | Modified | Update module-system, extensibility, backend/frontend module docs, and examples to one model. |
| LSP metadata | Optional/New | Generate or expose contract metadata only if bounded by the plan. |

## Architectural Guardrails

- Rust core remains a generic validator, router, loader, and diagnostics layer.
- Services remain interface contracts plus providers, not hardcoded Rust service APIs.
- Module identity should be stable package identity. Localized labels, display names, and resources must not become identity.
- Capabilities are host powers. Interface implementation and interface consumption are separate declarations.
- Contributions should be inspectable from the installed graph so tools, diagnostics, and docs do not need separate discovery paths.
