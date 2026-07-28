---
phase: 87-layout-and-display-elements
title: Layout And Display Elements Research
status: complete
---

# Phase 87 Research

## Findings

### Element Contract

Phase 86 made `mesh-core-elements` the correct home for native element metadata. `ElementContractDef` already carries tag, family, attributes, events, accessibility defaults, style hooks, and compatibility references. `validate_element_attribute` and `validate_element_event` are the existing diagnostics entry points, and frontend compilation already calls them through `collect_element_diagnostics`.

Research implication: Phase 87 should extend the metadata arrays and validation helpers rather than adding compiler-only rule tables.

### Source Tags And Lowering

`SourceTag` already recognizes Phase 87 names including `grid`, `stack`, `scroll-area`, structure tags, display tags, `progress`, `meter`, and `tooltip`. `lower_source_tag` currently maps planned tags to safe primitives:

- layout/structure mostly to `box`, `row`, `column`, or `scroll`
- display to `text`, `icon`, or `image`
- `meter` and `progress` both to `text`

Research implication: preserve semantic source tag data as runtime attributes before lowering so diagnostics, accessibility, docs, and style hooks can distinguish source semantics without destabilizing layout/painter primitives.

### Layout Runtime

The layout engine already delegates to Taffy and has coverage for flex direction, gap, padding, absolute positioning, display none, text measurement, direction, and stack-like overlay parity. CSS grid properties are currently classified as unsupported/out-of-scope.

Research implication: implement grid behavior through conservative source attributes and existing layout fields rather than opening broad CSS grid support. Keep unsupported browser grid CSS diagnostic behavior intact.

### Scroll

Runtime scroll support already uses `scroll` nodes, `_mesh_scroll_x`, `_mesh_scroll_y`, scroll limits, input wheel handling, and overflow annotation. `scroll-area` can safely lower to `scroll`.

Research implication: make `scroll-area` canonical at the source/metadata/docs/LSP layer while preserving runtime `scroll` and `scroll-view` compatibility.

### Tooltip

Shell interaction tests already prove title-based tooltip lookup, inherited tooltip ownership, hover timing, repaint scheduling, and locale updates. There is no separate `<tooltip>` behavior yet.

Research implication: Phase 87 should bind `<tooltip>` to the existing tooltip/title model and add tests for semantic ownership rather than inventing a new overlay subsystem.

### LSP

`crates/tools/lsp/src/knowledge/tags.rs` has hand-maintained tag definitions and attribute bases. It already imports `mesh-core-elements` for script element fields, but template tag completion uses its local table.

Research implication: update the local LSP table for Phase 87 source tags and attributes. A later phase can deduplicate LSP knowledge against `ElementContractDef`.

## Risks

| Risk | Mitigation |
|------|------------|
| Source semantics disappear after lowering, making progress/structure/display indistinguishable at runtime | Preserve source tag and role metadata in `WidgetNode.attributes` before lowering. |
| Grid scope accidentally becomes browser CSS grid | Keep CSS grid properties out of supported CSS and implement only source attributes with diagnostics for unsupported combinations. |
| `meter` duplicates `progress` behavior | Keep `meter` taxonomy/docs-only for now and add docs/tests that only `progress` has behavior in this phase. |
| LSP diverges from parser | Add LSP completion tests for new Phase 87 tags/attributes. |
| Existing shipped layouts regress | Run package-level tests plus shipped tooltip/layout fixture tests. |

## Recommended Plan Shape

1. Extend metadata and diagnostics for Phase 87 layout/structure/display primitives.
2. Preserve semantic source tag/accessibility/value metadata through compiler lowering.
3. Update LSP/docs and prove runtime compatibility with focused tests.
