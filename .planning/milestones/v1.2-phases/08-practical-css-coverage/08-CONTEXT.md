# Phase 8: Practical CSS Coverage - Context

**Gathered:** 2026-05-05
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase expands MESH's CSS-like styling contract so plugin authors can use a practical, documented subset of common CSS properties for shell UI styling. It covers parsing/lowering, computed style representation, token and variable resolution, unsupported-property diagnostics, and proof examples for common shell styling.

This phase does not implement container-size reactivity beyond preserving the existing container-query contract, text selection, keyboard navigation, custom keyframe animation scheduling, or navigation-bar migration. Those are Phase 9 through Phase 13 work.

</domain>

<decisions>
## Implementation Decisions

### CSS Support Boundary
- **D-01:** Phase 8 should define "common CSS" as a practical shell-ui subset, not browser compatibility. Supported areas are box model, sizing, flex layout, typography, borders/radius, overflow, display/visibility-like behavior, opacity/color/background, positioning/inset, selector state hooks, transition declarations, and animation declarations needed by later phases.
- **D-02:** CSS Grid, floats, multicolumn layout, full media queries, cascade layers, arbitrary web at-rules, transforms/filters, rich web text APIs, and browser-compatible layout edge cases remain out of scope for Phase 8.
- **D-03:** The supported subset should be documented as an author-facing contract, with examples that look like normal CSS where possible and explicit notes where MESH semantics differ.

### Shorthands and Computed Style Model
- **D-04:** Shorthand support should lower into MESH's existing `ComputedStyle` fields rather than preserving a browser-style CSS object model. Planner/researcher should extend `ComputedStyle` only when a property has a real renderer/layout consumer or is required by a later v1.2 phase.
- **D-05:** Phase 8 should prioritize common shorthand coverage for `margin`, `padding`, `border`, `border-width`, `border-color`, `border-radius`, `overflow`, `flex`, `inset`, `font`, `transition`, and animation declarations.
- **D-06:** Where browser shorthand behavior is complex, MESH should implement the useful shell subset first and document unsupported forms instead of accepting syntax that produces surprising output.

### Tokens and Variables
- **D-07:** Existing `token(...)` authoring remains a first-class path and should keep working across all supported properties.
- **D-08:** CSS custom properties should be supported enough for local component variables and `var(...)` resolution in supported declarations. Full browser cascade inheritance for custom properties is not required in Phase 8 unless planning finds it is already cheap and well-contained.
- **D-09:** Theme-token and variable resolution should happen before computed style is consumed by layout or paint, so downstream phases can rely on concrete values.

### Unsupported CSS Diagnostics
- **D-10:** Unsupported properties should produce clear diagnostics or structured warnings with property name and enough selector/source context to help authors fix styles.
- **D-11:** Unsupported at-rules should fail or warn consistently according to risk: harmless unsupported at-rules may become diagnostics/no-ops, but malformed supported CSS and unsupported constructs that would change cascade semantics should remain errors rather than silently changing output.
- **D-12:** Unknown properties should not crash the shell at runtime. They should be visible to plugin authors through diagnostics, logs, or LSP-facing metadata.

### Parser, Resolver, LSP, and Docs
- **D-13:** Phase 8 should keep parsing/lowering in `mesh-core-component`, computed style and value resolution in `mesh-core-elements`, and paint/layout consumption in `mesh-core-render`.
- **D-14:** Any supported CSS expansion should update tests near the parser/resolver and update author-facing documentation or LSP knowledge where the project already exposes styling help.
- **D-15:** Navigation-bar should not be migrated in this phase. Phase 8 may add focused fixtures or examples, but the full navigation-bar proof belongs to Phase 13.

### the agent's Discretion
- Planner/researcher may choose the exact initial property table, as long as it covers the requirement categories and avoids full browser compatibility scope.
- Planner/researcher may decide whether diagnostics are emitted through existing diagnostics collectors, parser errors, tracing, LSP metadata, or a combination, as long as unsupported CSS is visible to authors.
- Planner/researcher may choose whether CSS variables are implemented as a small `VariableStore` extension, parser-level lowering, or resolver-level lookup, as long as supported declarations can resolve `var(...)` predictably.
- Planner/researcher may decide exact test fixture names and documentation locations.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Phase Scope
- `.planning/PROJECT.md` — v1.2 rendering system goals and the locked decision that MESH supports practical shell CSS rather than full web compatibility.
- `.planning/REQUIREMENTS.md` — CSS-01 through CSS-04 requirements and out-of-scope boundaries.
- `.planning/ROADMAP.md` — Phase 8 goal, dependencies, and success criteria.
- `.planning/STATE.md` — Current milestone state and prior architectural decisions.

### Codebase Maps
- `.planning/codebase/STRUCTURE.md` — UI/render/component crate locations and where to add CSS-related code.
- `.planning/codebase/STACK.md` — Rust, `lightningcss`, `cssparser`, `cosmic-text`, Wayland, and rendering dependencies.
- `.planning/codebase/ARCHITECTURE.md` — Render, element, component parser, and shell orchestration boundaries.

### CSS Parser and Style Model
- `crates/core/ui/component/src/parser/styles.rs` — Existing `lightningcss` lowering, selector parsing, container query handling, and unsupported at-rule behavior.
- `crates/core/ui/component/src/style.rs` — Style AST types, `StyleRule`, `Declaration`, `StyleValue`, `Selector`, and `ContainerQuery`.
- `crates/core/ui/elements/src/style.rs` — `ComputedStyle`, style property enums, `StyleResolver`, token resolution, shorthands, transition parsing, and current unsupported-property handling.
- `crates/core/ui/render/src/style.rs` — Render-tree default styles, inheritance helpers, child container style context, and style defaults.

### Layout, Paint, and Tooling Consumers
- `crates/core/ui/elements/src/layout.rs` — Layout engine consuming `ComputedStyle` sizing, flex, spacing, positioning, and overflow fields.
- `crates/core/ui/render/src/surface/painter.rs` — Painter consuming visual, border, overflow, and text style fields.
- `crates/core/ui/render/src/surface/text.rs` — `cosmic-text` measurement/rendering path for typography-related properties.
- `crates/tools/lsp/src/knowledge/tags.rs` — Existing LSP-facing element and attribute knowledge that may need matching style support docs later.
- `packages/plugins/frontend/core/navigation-bar/src/main.mesh` — Existing core surface style examples and future Phase 13 proof target; do not migrate fully in Phase 8.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `lightningcss` already parses style blocks and serializes property names/values in `crates/core/ui/component/src/parser/styles.rs`.
- `StyleValue::Token` and `StyleValue::Var` already exist in `crates/core/ui/component/src/style.rs`, but `StyleResolver::resolve_value` currently treats `var(...)` as unresolved.
- `ComputedStyle` already covers many common shell CSS concepts: dimensions, min/max constraints, padding, margin, borders, radius, opacity, transition, overflow, typography, flex, wrap, align, position, z-index, and insets.
- `StyleResolver::apply_declaration` already handles many longhands and some shorthands, making Phase 8 mostly an expansion/hardening pass instead of a new styling engine.
- `ContainerQuery` lowering already exists for width/height, but Phase 9 owns reactive re-evaluation when container sizes change.

### Established Patterns
- Parser types live in `mesh-core-component`; runtime-independent style and layout types live in `mesh-core-elements`; rendering-specific behavior lives in `mesh-core-render`.
- The renderer builds fresh `WidgetNode` trees from component state, then layout and paint consume resolved `ComputedStyle`.
- Theme tokens are expected in `.mesh` styles through `token(group.name)`.
- Unsupported or unknown style properties currently trend toward debug logging; Phase 8 should raise author visibility without making normal shell startup fragile.

### Integration Points
- Add or normalize CSS property lowering in `crates/core/ui/component/src/parser/styles.rs` when `lightningcss` emits values that need MESH-specific handling.
- Add supported style fields and parsers in `crates/core/ui/elements/src/style.rs` only when a field has a planned consumer.
- Wire new visual/layout fields into `crates/core/ui/elements/src/layout.rs` or `crates/core/ui/render/src/surface/painter.rs` as needed.
- Update author-facing style support docs and LSP metadata where supported properties become part of the contract.

</code_context>

<specifics>
## Specific Ideas

- User wants "most of the common CSS attributes" but explicitly does not want unnecessary CSS attributes.
- User wants enough styling power to create a unique shell styling language and distinctive shell UI.
- Phase 8 should make later phases easier by giving them a stable style-property contract for reactivity, selection, keyboard focus styling, and animations.

</specifics>

<deferred>
## Deferred Ideas

- Container-size restyle behavior and state-driven live restyling are Phase 9.
- Selectable text and copy behavior are Phase 10.
- Keyboard navigation and shortcuts are Phase 11.
- Theme animation tokens and custom CSS keyframe scheduling are Phase 12.
- Navigation-bar migration/proof is Phase 13.
- Full browser CSS compatibility, CSS Grid, floats, rich text editing, and GPU transform/filter animation remain future/out-of-scope work.

</deferred>

---

*Phase: 8-Practical CSS Coverage*
*Context gathered: 2026-05-05*
