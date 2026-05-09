---
phase: 08-practical-css-coverage
status: complete
created: 2026-05-05
---

# Phase 8 Pattern Map

## Files and Closest Analogs

| Target File | Role | Closest Existing Analog | Pattern to Preserve |
|-------------|------|-------------------------|---------------------|
| `crates/core/ui/component/src/parser/styles.rs` | CSS parse/lower layer | Existing `lower_css_rules`, `lower_property`, `classify_style_value`, container query lowering | Keep parser runtime-independent; return `ParseError::InvalidStyle` for unsupported syntax that changes semantics. |
| `crates/core/ui/component/src/style.rs` | Portable style AST | Existing `StyleValue`, `Declaration`, `ContainerQuery`, `Selector` | Add only AST data that is safe before runtime theme/layout context. |
| `crates/core/ui/elements/src/style.rs` | Computed style and resolver | Existing `ComputedStyle`, `StyleResolver::resolve_node_style`, `apply_declaration`, `parse_transition_shorthand` | Resolve tokens/variables before writing concrete fields; keep unsupported values deterministic and tested. |
| `crates/core/ui/elements/src/layout.rs` | Layout consumer | Existing flex, absolute positioning, overflow-aware layout tests | Consume concrete `ComputedStyle` fields only; no parser or theme dependency. |
| `crates/core/ui/render/src/surface/painter.rs` | Paint consumer | Existing background, border, clipping, z-index, text render paths | Paint only supported concrete fields; sort z-index and clip overflow in one place. |
| `crates/tools/lsp/src/knowledge/css.rs` | Authoring support table | Existing `CSS_PROPERTIES` static list | Keep completions limited to supported properties; update comments when diagnostics replace silent ignore. |
| `docs/css-coverage.md` | Author contract | Existing coverage tables | Make support statuses match code and explicitly separate accepted metadata from active behavior. |
| `docs/frontend/mesh-syntax.md` | Authoring guide | Existing syntax guide sections | Link to CSS coverage and show practical examples without duplicating the full table. |

## Reusable Code Excerpts

### Parser Lowering Boundary

`parse_style` parses with `lightningcss` and returns a `StyleBlock`:

```rust
let stylesheet = StyleSheet::parse(source, CssParserOptions { error_recovery: false, ..Default::default() })?;
lower_css_rules(&stylesheet.rules.0, None, &mut rules)?;
```

Use this as the boundary for at-rule and selector support. Do not move style resolution into the parser.

### Resolver Boundary

`StyleResolver::resolve_node_style(...)` currently builds `ComputedStyle` by applying matching rules in order:

```rust
for rule in rules {
    if rule_matches(rule, tag, classes, id, context, state) {
        for decl in &rule.declarations {
            apply_declaration(&mut style, &decl.property, &decl.value, self);
        }
    }
}
```

Custom property collection and unsupported-property diagnostics should attach near this loop so rule order remains deterministic.

### Existing Diagnostics Gap

Unknown properties currently fall through:

```rust
_ => {
    tracing::debug!("unknown style property: {property}");
}
```

Phase 8 must replace or supplement this with visible diagnostics that tests can assert.

### Layout Consumer Pattern

`layout.rs` excludes absolute children from flex flow and then positions them using inset fields. Any shorthand changes should resolve before layout receives the tree.

### Paint Consumer Pattern

`painter.rs` sorts children by `computed_style.z_index` and clips descendants when overflow is not visible. New visual fields need a corresponding paint consumer or should not be added.

## Landmines

- `lightningcss` may serialize shorthands into normalized values; parser tests must lock the actual strings seen by the resolver.
- Full custom-property cascade is browser-engine scope. Phase 8 should implement local component/rule variables only.
- `@keyframes` must remain unsupported until Phase 12 implements scheduling. Accepting animation declarations must not imply animation playback.
- Do not add CSS Grid/floats/media queries as hidden extras; they are explicitly out of scope in `08-CONTEXT.md`.
- LSP completions must not advertise properties the resolver still ignores.

## PATTERN MAPPING COMPLETE
