# Phase 86 Pattern Map

## Closest Analogs

| Target | Existing Analog | Pattern To Reuse |
|--------|-----------------|------------------|
| Element taxonomy metadata | `crates/core/ui/elements/src/element.rs` | Additive typed metadata arrays with static definitions and lookup helpers. |
| Source tag classification | `crates/core/ui/component/src/template.rs` | `SourceTag::from_tag_name` maps source strings to semantic enum values while preserving lowercase primitive / PascalCase component split. |
| Compiler lowering | `crates/core/frontend/compiler/src/tags.rs` | Explicit source-to-runtime primitive lowering with stable runtime strings. |
| Attribute/event extraction | `crates/core/frontend/compiler/src/render.rs` | Convert parsed attributes to resolved attributes and `event_handlers` maps while keeping Luau handler names. |
| Accessibility defaults | `crates/core/frontend/compiler/src/render.rs` and `crates/core/ui/elements/src/accessibility.rs` | Assign role/focusability from element tag; extend defaults from metadata. |
| Unsupported feature diagnostics | `crates/core/ui/elements/src/style.rs` and `crates/core/ui/elements/src/layout.rs` | Structured diagnostics with exact property/tag/reason strings and focused tests. |
| Author docs | `docs/frontend/mesh-syntax.md` | Add focused frontend docs and cross-link from syntax overview. |

## Data Flow

1. `.mesh` parser reads source tag and attributes into `ElementNode`.
2. `SourceTag` captures source-level semantics.
3. Frontend compiler lowers source semantics to runtime primitives and resolves attributes/events.
4. `WidgetNode` carries runtime tag, attributes, event handlers, state, and accessibility metadata.
5. Renderer/layout/style consume runtime primitive tags and state.
6. Author docs explain source-level element behavior and MESH-native boundaries.

## Plan Guidance

- Keep Phase 86 additive and compatibility-preserving.
- Use `mesh-core-elements` as the source of truth for full taxonomy metadata.
- Keep `SourceTag` and `UiTag` as compatibility adapters until later phases can retire duplication safely.
- Tests should assert exact tag names, requirement-family coverage, event names, state flags, and diagnostic messages.
