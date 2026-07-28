# Phase 86: Element Contract And Infrastructure - Context

**Gathered:** 2026-05-26
**Status:** Ready for planning

<domain>
## Phase Boundary

Define the MESH-native element contract that later v1.16 phases build against: a broad element taxonomy, parser/runtime metadata, shared control state, common value/change event plumbing, diagnostics for unsupported or invalid attributes, and author documentation explaining the MESH element model. This phase establishes names, metadata, state, events, diagnostics, and docs; family-specific rendering, keyboard behavior, popups, layout algorithms, collection interaction, and shipped surface migrations belong to Phases 87-91.

</domain>

<decisions>
## Implementation Decisions

### Element Taxonomy And Boundaries
- Use a registry-backed `ElementKind` taxonomy with family metadata, common attribute metadata, state/event capabilities, accessibility defaults, and diagnostics hooks.
- Treat HTML, Qt Widgets/layouts, and Flutter as coverage references only; each documented element should explain the closest inspiration and MESH-native behavior without promising one-for-one compatibility.
- Include all planned families in Phase 86 metadata: layout, display, action, text/numeric input, choice/menu, container, collection, and shell-specific controls. Later phases fill behavior behind the same metadata.
- Represent shell-specific controls as first-class MESH element family entries, not hidden variants of generic HTML-like controls.

### Attributes, State, And Events
- Add shared metadata for common attributes such as `disabled`, `readonly`, `required`, `value`, labels, ids/classes, style hooks, and handler names so parser/runtime diagnostics can be generic.
- Introduce a shared control state record covering disabled, read-only, required, focused, selected, checked, expanded, pressed, invalid, active, and value. Metadata determines which fields apply to each element.
- Route typed value/change events through shared Luau handler plumbing using element metadata for event names and payload shape.
- Implement generic unsupported/invalid attribute diagnostics with concrete author actions; family-specific nesting/value diagnostics can be added in later behavior phases.

### Parser And Runtime Integration
- Put canonical runtime element metadata in `mesh-core-elements`, with parser/compiler consuming it rather than duplicating tag knowledge.
- Preserve current shipped tags and map them into the new taxonomy without changing existing module behavior in Phase 86.
- Define stable classes and pseudo-state hooks from shared state metadata, but implement only generic metadata and diagnostics now; later phases prove family-specific visuals.
- Focus tests on taxonomy completeness, parser/AST metadata representation, shared state/event plumbing, and non-fatal attribute diagnostics. Full interaction tests belong to later control phases.

### Docs, Author Contract, And Deferrals
- Author docs should provide the contract/reference foundation: taxonomy, common attributes, state, events, diagnostics, accessibility expectations, and explicit non-compatibility with HTML/Qt/Flutter parity.
- Add the first element model docs under `docs/frontend/`, cross-linked from existing syntax/module docs as needed.
- Defer family-specific rendering, keyboard behavior, popups, layout algorithms, collection interaction, and shipped surface migration to Phases 87-91.
- Treat Phase 86 metadata and docs as the source of truth; later phases add behavior against the taxonomy instead of redefining element names, state, or event semantics.

### the agent's Discretion
Implementation details not decided above are at the agent's discretion, constrained by existing MESH architecture, typed Rust domain models, non-fatal diagnostics, retained rendering ownership, and Luau authoring conventions.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/ui/elements` owns runtime UI tree, element definitions, layout, style, events, and accessibility primitives.
- `crates/core/ui/component` owns `.mesh` parser and AST/template structures.
- `crates/core/frontend/compiler` owns tag lowering, accessibility defaults, and widget tree construction.
- `docs/frontend/mesh-syntax.md` and related frontend docs are the right author-facing documentation neighborhood.

### Established Patterns
- Rust core stays generic; service- or surface-specific behavior belongs in modules, metadata, and typed runtime lanes rather than bespoke shell branches.
- Parser/load failures that affect authors should become typed, actionable diagnostics rather than crashes where possible.
- Tests are colocated in Rust source under `#[cfg(test)]`, with behavior-oriented names.
- Existing `.mesh` tags such as `row`, `box`, `button`, `icon`, and `text` should remain compatible for shipped modules.

### Integration Points
- Element metadata should integrate with `mesh-core-elements` and be consumed by parser/compiler paths in `mesh-core-component` and `mesh-core-frontend`.
- Shared state and event metadata should align with existing Luau handler and event/channel conventions from v1.14-v1.15.
- Documentation should connect the native element model to the frontend syntax docs and v1.16 milestone requirements.

</code_context>

<specifics>
## Specific Ideas

Use HTML, Qt Widgets/layouts, and Flutter as coverage references while keeping MESH behavior shell-native, deterministic, retained-renderer-friendly, diagnostic-rich, and Luau-oriented.

</specifics>

<deferred>
## Deferred Ideas

- Family-specific rendering and layout behavior.
- Keyboard navigation, focus trapping, popups, dropdowns, menus, and collection interaction.
- Shipped navigation/audio/quick-settings/debug surface migrations.
- Full browser form submission semantics, native platform widget embedding, virtualized mega-tables, and rich text editing.

</deferred>
