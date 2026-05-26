# Phase 87 Pattern Map

## Closest Analogs

| Target | Existing Analog | Pattern To Reuse |
|--------|-----------------|------------------|
| Element contract metadata | `crates/core/ui/elements/src/element.rs` | Extend static metadata and validation helpers from Phase 86. |
| Source tag compatibility | `crates/core/ui/component/src/template.rs` | Preserve source-level semantics in `SourceTag` while lowering runtime tags conservatively. |
| Runtime primitive lowering | `crates/core/frontend/compiler/src/tags.rs` | Keep `UiTag` small and safe; add metadata/attributes before lowered tags reach layout/painter. |
| Accessibility defaults | `crates/core/frontend/compiler/src/render.rs` | Assign role/focusability during `WidgetNode` construction. |
| Taffy layout proof | `crates/core/ui/elements/src/layout.rs` | Add narrow tests around grid/stack/spacer/divider/scroll behavior using existing layout helpers. |
| Tooltip ownership | `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs` | Reuse existing title/tooltip inheritance and hover behavior tests. |
| LSP completions | `crates/tools/lsp/src/knowledge/tags.rs` | Add source tags and attribute groups to the existing hand-maintained table. |
| Author docs | `docs/frontend/elements.md` | Update the existing element library page instead of creating a separate document. |

## Data Flow

1. Parser converts lowercase native tags into `SourceTag` variants.
2. Frontend compiler records source semantics and resolves attributes/events.
3. Compiler lowers to runtime primitives consumed by style/layout/painter.
4. Runtime primitives render through existing Taffy, scroll, text/icon/image, and tooltip systems.
5. Metadata, diagnostics, LSP, and docs describe source-level behavior.

## Plan Guidance

- Prefer source attributes and metadata over new painter primitives.
- Keep CSS grid properties unsupported unless a value maps cleanly to current layout primitives.
- Keep `meter` taxonomy/docs-only for this phase.
- Add tests close to each contract surface rather than one broad end-to-end fixture.
