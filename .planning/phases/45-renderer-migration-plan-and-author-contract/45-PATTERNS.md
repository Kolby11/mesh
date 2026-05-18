# Phase 45 Pattern Map

## Documentation Patterns

### Existing Author-Facing Syntax Docs

Closest analog: `docs/frontend/mesh-syntax.md`

Use this pattern for `docs/frontend/renderer-contract.md`:

- Start with the user-facing contract in direct language.
- Use concrete examples and tables.
- Explicitly separate current behavior from future/deferred behavior.
- Name invalid syntax or unsupported expectations directly.

Relevant existing sections:

- `# .mesh Component Syntax`
- `## File structure`
- `### Tags`
- `### Element metrics`
- `### Keyboard focus and traversal`

### Existing Architecture Sketch Docs

Closest analog: `docs/frontend/html-css-transition.md`

Use this pattern for `docs/renderer-migration.md`:

- Explain current implementation status before target architecture.
- Separate "Functional now", "Partially functional", and "Not functional yet" concepts when helpful.
- Use pipeline diagrams for source-to-runtime transitions.
- Keep browser compatibility non-goals explicit.

Relevant existing sections:

- `## Current implementation status`
- `## Current pipeline in this codebase`
- `## Recommended target architecture`

### Existing Crate Boundary Docs

Closest analog: `crates/core/frontend/render/README.md`

Use this pattern for `docs/renderer-ownership.md`:

- Name crate ownership boundaries precisely.
- Keep rendering, frontend compiling, runtime, and presentation responsibilities separate.
- Describe where new renderer-specific code should live.

Relevant existing statements:

- `mesh-core-render` owns software rendering, pixel buffers, text, icons, glyphs, widgets, and debug overlays.
- `mesh-core-frontend` compiles and lowers source into widget trees.
- `mesh-core-presentation` presents `PixelBuffer`s.

## Planned Files And Closest Analogs

| Planned file | Role | Closest analog | Pattern to copy |
|--------------|------|----------------|-----------------|
| `docs/renderer-migration.md` | Maintainer-facing phased migration roadmap | `docs/frontend/html-css-transition.md` | Current state, target architecture, explicit non-goals, phased plan |
| `docs/renderer-ownership.md` | Maintainer-facing classification table | `crates/core/frontend/render/README.md` | Crate-boundary responsibility language and exact paths |
| `docs/frontend/renderer-contract.md` | Plugin-author-facing renderer contract | `docs/frontend/mesh-syntax.md` | Direct authoring contract with tables and invalid expectations |
| `docs/frontend/mesh-syntax.md` | Existing authoring doc to link contract | Existing file | Add one short link section, do not rewrite syntax |
| `docs/module-system.md` | Existing module authoring doc to link contract | Existing file | Add one short reference under frontend module guidance |

## Verification Patterns

Docs in this repo are usually verified with `rg` checks rather than a docs build. Phase 45 plans should use exact-string checks for:

- `MIGR-01`, `MIGR-02`, `MIGR-03`
- `authoritative`, `adapter-owned`, `replacement candidate`
- `Feature flag`, `Linux/Nix`, `Binary`, `Rollback`
- `NodeId`, `typed invalidation`, `damage`, `profiling`, `diagnostics`, `AccessKit`, `theme-owned selection`
- `renderer contract` links from existing docs
