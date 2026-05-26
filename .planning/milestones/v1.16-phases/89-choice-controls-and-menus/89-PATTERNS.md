---
phase: 89-choice-controls-and-menus
title: Patterns
status: complete
---

# Patterns

## Existing Patterns To Follow

- Phase 88 used configured source variants over a single runtime path. Phase 89 should follow that pattern for `segmented-control`, `menu-item`, `command-item`, and `preference-row`.
- Attribute diagnostics live in `crates/core/ui/elements/src/element.rs` and use `invalid_attr(...)` with actionable messages.
- Compiler source defaults live in `apply_source_tag_defaults(...)`.
- Runtime interaction state is keyed by `_mesh_key`; new choice state should use the same key-based model.
- Navigation tests prefer real shipped component proof for user-visible migrations.

## File Ownership

- Element metadata/diagnostics: `crates/core/ui/elements/src/element.rs`
- Compiler lowering/accessibility/defaults: `crates/core/frontend/compiler/src/render.rs`, `crates/core/frontend/compiler/src/tags.rs`
- Shell behavior: `crates/core/shell/src/shell/component/input/*`, `runtime_tree.rs`
- Tooling/docs: `crates/tools/lsp/src/knowledge/tags.rs`, `docs/frontend/elements.md`, `docs/frontend/mesh-syntax.md`
- Shipped migration: `modules/frontend/navigation-bar/src/components/language-button.mesh`
