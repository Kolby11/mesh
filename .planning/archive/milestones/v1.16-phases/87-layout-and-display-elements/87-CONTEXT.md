---
phase: 87-layout-and-display-elements
title: Layout And Display Elements Context
status: planned
requirements:
  - ELEMLAYOUT-01
  - ELEMLAYOUT-02
  - ELEMLAYOUT-03
  - ELEMLAYOUT-04
  - ELEMLAYOUT-05
  - ELEMDISPLAY-01
  - ELEMDISPLAY-02
  - ELEMDISPLAY-03
  - ELEMDISPLAY-04
  - ELEMDISPLAY-05
---

# Phase 87 Context

## Goal

Implement the layout, structure, and display primitives needed to compose shell surfaces while preserving current shipped layout behavior.

## Locked Decisions

- Implement concrete behavior for required layout primitives only: `grid`, `stack`, `spacer`, `divider`, and `scroll-area`; preserve existing `box`, `row`, and `column` behavior.
- Keep grid conservative: support fixed/auto track sizing, gaps, and row/column placement metadata; emit diagnostics for unsupported complex grid semantics.
- Implement `stack` as overlay composition using existing absolute positioning and z-order behavior where possible.
- Treat `scroll-area` as the canonical semantic source tag while preserving `scroll` and `scroll-view` compatibility.
- Structure tags (`section`, `header`, `footer`, `group`, `form-row`) lower to existing primitives, but carry accessibility role/label/style metadata and diagnostics.
- Display primitives (`badge`, `avatar`, `shortcut`) build on existing text/icon/image primitives with metadata, accessibility, and style hooks.
- Implement `progress` behavior only in this slice. Keep `meter` in taxonomy/docs as future behavior; do not create duplicate runtime code.
- Reuse existing title/tooltip lookup behavior; make `<tooltip>` and tooltip attributes visible to pointer/keyboard ownership tests.
- Put diagnostics in `mesh-core-elements` and surface them through the frontend compiler.
- Update LSP tag/attribute knowledge for Phase 87 tags and core attributes.
- Prove compatibility with focused package tests and existing shipped surface fixtures; do not build a gallery or migrate shipped surfaces in this phase.

## Current Code Shape

- `crates/core/ui/elements/src/element.rs` already contains Phase 86 taxonomy, contract metadata, attribute/event diagnostics, runtime snapshots, and planned tags.
- `crates/core/ui/component/src/template.rs` and `parser/markup.rs` already parse Phase 87 tags as `SourceTag` values.
- `crates/core/frontend/compiler/src/tags.rs` lowers Phase 87 tags to safe runtime primitives. This is where semantic source tags can be preserved in attributes before lowering.
- `crates/core/frontend/compiler/src/render.rs` builds `WidgetNode` trees, resolves attributes/events, and assigns accessibility defaults.
- `crates/core/ui/elements/src/layout.rs` uses Taffy for layout. Existing tests cover row, column, absolute positioning, stack parity, display none, text measurement, and scroll offsets.
- `crates/core/shell/src/shell/component/input/mod.rs` and shipped surface tests already exercise title-based tooltip ownership and hover timing.
- `crates/tools/lsp/src/knowledge/tags.rs` is the editor tag/attribute vocabulary.
- `docs/frontend/elements.md` is the author-facing element library document created in Phase 86.

## Out Of Scope

- Action control behavior beyond compatibility metadata.
- Text input, numeric input, and choice/menu runtime behavior.
- Container/collection controls.
- Full UI gallery, visual demo migration, or broad shipped surface rewrites.
- Separate `meter` runtime behavior unless a later phase identifies a distinct need.

## Success Proof

- Element metadata and diagnostics cover Phase 87 layout/display/structure attributes and invalid combinations.
- Frontend compiler preserves source element semantics while lowering to compatible runtime primitives.
- Runtime layout/display behavior works through Taffy, existing style hooks, existing scroll offsets, and existing tooltip ownership.
- LSP completions and docs match the shipped source contract.
- Existing shipped modules continue to pass compatibility tests.
