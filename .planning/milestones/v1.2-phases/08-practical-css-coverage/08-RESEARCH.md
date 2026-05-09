---
phase: 08-practical-css-coverage
status: complete
created: 2026-05-05
research_scope: parser-resolver-renderer-docs
requirements: [CSS-01, CSS-02, CSS-03, CSS-04]
---

# Phase 8: Practical CSS Coverage - Research

## Research Question

What needs to be understood to plan practical CSS coverage for MESH without turning the renderer into a browser engine?

## Current Implementation

### Parser Layer

`crates/core/ui/component/src/parser/styles.rs` already uses `lightningcss` to parse `<style>` blocks, lower grouped selectors into separate `StyleRule` entries, lower supported `@container` rules, and reject unsupported at-rules with `ParseError::InvalidStyle`.

Useful existing behavior:
- Property names are serialized from `lightningcss::properties::Property::property_id().name()`.
- Values are serialized with `value_to_css_string(...)`.
- `token(...)` and `var(...)` are classified into `StyleValue::Token` and `StyleValue::Var`.
- Unsupported at-rules already fail with messages like `unsupported at-rule '@media'`.
- Container queries already support width/height range features and nested `and` intersection.

Gaps:
- Unknown CSS properties that `lightningcss` accepts can still lower into `Declaration` and later disappear in `StyleResolver::apply_declaration` with only `tracing::debug!("unknown style property: {property}")`.
- The parser does not preserve source selector/property context beyond the lowered selector and property name.
- Custom property declarations such as `--surface: token(color.surface)` are not treated as a local variable definition; only `var(...)` use-sites are represented.
- `@keyframes` is rejected, but Phase 8 needs animation declarations for later Phase 12. The safe Phase 8 boundary is declaration parsing/storage, not custom keyframe scheduling.

### Style AST and Computed Style

`crates/core/ui/component/src/style.rs` contains the portable style AST. `StyleValue::Var` exists, and `ContainerQuery` plus selector types are runtime-independent.

`crates/core/ui/elements/src/style.rs` owns `ComputedStyle`, `StyleResolver`, property enums, token resolution, style rule matching, and most current declaration application.

Supported concepts already present:
- Box model: width/height/min/max, padding, margin, border width/color/radius.
- Visual: background/color/opacity/overflow.
- Text: font family/size/weight/style, line height, letter spacing, text alignment, direction, overflow.
- Flex: direction, grow/shrink/basis, wrap, align/justify/gap.
- Positioning: static/relative/absolute, z-index, inset sides.
- Transitions: duration/delay/easing/properties for a small set of visual properties.
- State selectors: hover/focus/active/disabled/checked/focus-visible are matched against `ElementState`.

Gaps:
- Edge and corner shorthands currently use one value for all sides/corners. Common CSS 2/3/4-value forms need practical support.
- `border` shorthand is not applied; authors must set `border-width` and `border-color` separately.
- `font` shorthand is absent.
- `flex` shorthand handles `none`, `auto`, and a bare number only; common `1 0 auto` / `0 1 12px` forms are absent.
- `inset` currently uses one value for all sides only.
- `var(...)` resolves to an empty string.
- Unsupported properties are not collected into a visible author-facing diagnostic structure.
- Animation declarations are not represented in `ComputedStyle`.

### Layout and Paint Consumers

`crates/core/ui/elements/src/layout.rs` already consumes display, dimensions, min/max, padding, margin, flex grow/shrink/basis, wrap, align-self, align-content, position absolute, inset sides, and overflow-aware sizing.

`crates/core/ui/render/src/surface/painter.rs` already consumes display, background color, border width/color, border radius, text style, overflow clipping/scrollbar behavior, and z-index child ordering.

Useful implication: Phase 8 should avoid adding `ComputedStyle` fields that no layout or paint consumer can use, unless they are declaration-only data reserved for Phase 12 (`animation-*` and expanded transition metadata).

### LSP and Docs

`crates/tools/lsp/src/knowledge/css.rs` is the existing author-facing property table for completions. Its comment currently says unsupported properties are silently ignored, which should become false after diagnostics improve.

`docs/css-coverage.md` already lists CSS selector/property coverage but is stale:
- It says pseudo state selectors are parsed only, while `StyleResolver` now evaluates state selectors.
- It says positioning and transitions are not implemented, while current code already has position/inset/z-index and transition fields.
- It says custom properties are parsed only, which is still true.

`docs/frontend/mesh-syntax.md` is the broad authoring guide and should reference the supported CSS subset instead of duplicating every property.

## Recommended Implementation Shape

### Property Contract

Create one shared source of truth for practical property support in `mesh-core-elements`, then mirror it in LSP/docs as needed:
- Supported properties: current `apply_declaration` properties plus Phase 8 additions.
- Declaration-only properties: `animation`, `animation-name`, `animation-duration`, `animation-delay`, `animation-timing-function`, `animation-iteration-count`, `animation-direction`, `animation-fill-mode`, `animation-play-state`.
- Explicitly unsupported properties: grid, floats, multicolumn, arbitrary media/supports/layer rules, transforms/filters, box-shadow, background-image/gradients, rich web text APIs.

The runtime should not silently ignore unknown properties. The minimum viable diagnostic is a structured warning emitted from resolver/parser tests with property and selector context. If a full diagnostics collector path is too invasive, store warnings on the style block or expose a deterministic warning helper consumed by compile/LSP later.

### Shorthand Parsing

Implement practical CSS shorthand parsing with small local parsers instead of preserving browser CSS object model:
- `margin`, `padding`, `border-width`, `border-color`, `border-radius`, `inset`: support 1/2/3/4-value CSS expansion.
- `border`: support width + color forms such as `1px solid #fff`, ignore style keyword except to recognize `none`.
- `overflow`: support one or two values, mapping to x/y.
- `flex`: support `none`, `auto`, bare grow number, and grow/shrink/basis triples.
- `font`: support enough to extract italic/normal, numeric or keyword weight, px size, optional line-height, and family.
- `transition`: keep current scheduler-independent metadata, but parse comma-separated transition items predictably for the supported visual property set.

Do not accept syntax that MESH cannot explain. Unsupported shorthand pieces should warn or no-op explicitly.

### Variables and Tokens

Keep `token(...)` first-class. Add component-local custom property support:
- Treat declarations whose property starts with `--` as variable assignments.
- Resolve `var(--name)` by looking up the nearest/local rule-defined custom property before consuming a supported declaration.
- Support `var(--name, fallback)` only if it is cheap to parse in `StyleValue::Var`; otherwise document that fallback is unsupported in Phase 8.
- Resolve token values inside custom property assignments before final property consumption.

Full browser custom property inheritance is not required. A node-local rule-order variable map is enough for Phase 8 if documented.

### Animation Boundary

Phase 8 should parse and store animation declarations for later Phase 12, but must not schedule frames or interpolate keyframes. A practical boundary:
- Add `AnimationStyle` / `AnimationDeclaration` metadata to `ComputedStyle`.
- Support `animation-*` longhands and a simple `animation` shorthand.
- Reject or diagnose `@keyframes` as unsupported until Phase 12.
- Document that animation declarations are accepted as metadata and become active when Phase 12 implements scheduling.

## Validation Architecture

Phase 8 should be test-heavy because the blast radius is a shared authoring contract.

### Test Targets

- `nix develop -c cargo test -p mesh-core-component style` for parser lowering, custom property classification, unsupported at-rules, and selector/container coverage.
- `nix develop -c cargo test -p mesh-core-elements style` for shorthand expansion, variables, token resolution, unsupported-property diagnostics, and computed style fields.
- `nix develop -c cargo test -p mesh-core-elements layout` for position/inset/flex/overflow consumer behavior when new shorthand results flow into layout.
- `nix develop -c cargo test -p mesh-core-render style` or focused render tests for border/overflow/z-index/text style consumption where tests already exist or can be added cheaply.
- `nix develop -c cargo test -p mesh-tools-lsp css` for CSS knowledge table and completion support.

### Proof Fixtures

Add representative `.mesh` snippets in tests rather than relying only on unit-level parser strings:
- A shell card using padding/margin/border/font/flex/inset variables.
- A scrollable shell list using overflow and flex shorthand.
- A button-like state rule using hover/focus styles and transition declarations.
- Unsupported property and unsupported at-rule examples that produce deterministic messages.

### Risk Areas

- `lightningcss` may normalize shorthands unexpectedly, so tests should assert the exact lowered property/value strings before resolver tests consume them.
- `var(...)` resolution can become browser-CSS scope creep. Keep Phase 8 local and documented.
- Diagnostics can become invasive if routed through every render path. Prefer a small deterministic style diagnostic type first, then integrate with LSP/docs.
- Animation declarations must not imply Phase 12 scheduling is complete.

## Planning Recommendation

Plan Phase 8 in five waves/slices:

1. Define the property contract and visible diagnostics path.
2. Expand shorthand and value resolution, including local custom properties.
3. Harden layout/render consumers for supported computed fields and representative fixtures.
4. Add transition and animation declaration metadata without scheduling.
5. Update LSP/docs and add end-to-end authoring proof examples.

This keeps architectural boundaries intact: parser lowering in `mesh-core-component`, runtime style resolution in `mesh-core-elements`, paint/layout consumption in `mesh-core-render`, and authoring knowledge in `docs/` plus `mesh-tools-lsp`.

## RESEARCH COMPLETE
