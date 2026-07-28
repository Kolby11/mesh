# Phase 88 Pattern Map

## Closest Analogs

| Target | Existing Analog | Pattern To Reuse |
|--------|-----------------|------------------|
| Single action behavior | `crates/core/frontend/compiler/src/tags.rs` | Keep action source tags lowering to runtime `button`. |
| Source semantics | `crates/core/frontend/compiler/src/render.rs` | Preserve source tag metadata in `data-mesh-element` while using runtime primitives. |
| Button activation | `crates/core/shell/src/shell/component/input/widgets.rs` and `keyboard.rs` | Pointer and keyboard activation both dispatch click-like events. |
| Text input editing | `crates/core/shell/src/shell/component/input/mod.rs` | Reuse existing value editing, Enter commit, and keyboard paths. |
| Focus traversal | `crates/core/ui/interaction/src/focus.rs` | Native focusable tags and tabindex sorting remain the source of traversal truth. |
| Diagnostics | `crates/core/ui/elements/src/element.rs` | Extend metadata-backed validation helpers. |
| LSP completions | `crates/tools/lsp/src/knowledge/tags.rs` | Extend local tag/attribute tables and tests. |
| Author docs | `docs/frontend/elements.md` | Update the existing element library page. |

## Data Flow

1. Parser captures source tags such as `button`, `search`, `number-input`, and compatibility button aliases.
2. Frontend compiler records source semantics and lowers to `button` or `input`.
3. Runtime focus/interaction paths operate on `button` and `input`.
4. Accessibility metadata and event payloads expose source semantics where authors need them.
5. LSP/docs guide authors toward configured primitives and explicit child content.

## Plan Guidance

- Do not create separate native action elements in Phase 88.
- Do not add button-level icon shortcut attributes.
- Numeric input behavior should be validation/coercion around `input`, not a new widget.
- Keep tests focused on contracts and existing runtime paths.
