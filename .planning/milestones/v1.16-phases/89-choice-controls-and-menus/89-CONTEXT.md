---
phase: 89-choice-controls-and-menus
title: Choice Controls And Menus Context
status: planned
requirements:
  - ELEMCHOICE-01
  - ELEMCHOICE-02
  - ELEMCHOICE-03
  - ELEMCHOICE-04
  - ELEMCHOICE-05
  - ELEMMENU-01
  - ELEMMENU-02
  - ELEMMENU-03
  - ELEMMENU-04
---

# Phase 89 Context

## Goal

Implement the first native choice and menu behavior layer for the v1.16 element library, including `select`/`option`, checkable choices, radio groups, menu items, diagnostics, tooling, docs, and the shipped navigation language selector migration.

## Locked Decisions

- Implement distinct native behavior only where behavior differs: `select`, `checkbox`, `switch`, `radio-group`/`radio`, and `menu`.
- Keep `segmented-control`, `menu-item`, `command-item`, `separator`, and `preference-row` as configured source elements unless a distinct runtime need appears.
- `select` is a compact trigger with a vertical option popup/listbox represented inside the component/runtime tree.
- Static child `<option value="...">Label</option>` is the Phase 89 authoring API; rich data-driven options are deferred.
- `checkbox`, `switch`, and `radio` should reuse the existing checked/toggle state path where possible, with source-tag-specific roles and group behavior.
- `menu` is a roving-focus command list. Nested menus are deferred unless existing code forces them.
- Use `onchange` for value/checked/selection changes. Use `onclick` or `onactivate` for menu commands.
- Diagnostics belong in `mesh-core-elements` and cover invalid option/value/group/menu relationships.
- Style hooks come from source tags, `data-mesh-element`, existing state flags, and metadata attributes.
- Only migrate the shipped navigation language selector from a custom horizontal button menu to native `select`/`option`.

## Current Code Shape

- `crates/core/frontend/compiler/src/tags.rs` lowers choice tags to existing runtime primitives. `select`, `option`, `switch`, `checkbox`, `radio`, `radio-group`, and `segmented-control` currently lower through the toggle/input path.
- `crates/core/frontend/compiler/src/render.rs` preserves source tags in `data-mesh-element` and derives accessibility metadata from source contracts.
- `crates/core/shell/src/shell/component/input` handles pointer focus, keyboard focus, text input, sliders, and checkable state, but several paths still inspect runtime tags instead of source tags.
- `crates/core/shell/src/shell/component/runtime_tree.rs` annotates input, slider, and checkable values into runtime nodes.
- `modules/frontend/navigation-bar/src/components/language-button.mesh` still uses a custom button plus horizontal menu for language selection.
- `crates/tools/lsp/src/knowledge/tags.rs` is the authoring completion source for element tags and attributes.

## Out Of Scope

- Rich data-driven option APIs.
- Native segmented-control behavior beyond configured source metadata.
- Nested menu popups.
- Separate surfaces for select popups.
- Full gallery proof. Use focused compiler/shell/navigation tests.

## Success Proof

- Source choice/menu tags have coherent metadata, diagnostics, accessibility, LSP, and docs.
- `select` options are visible in the component tree and dispatch `change` with the selected value.
- Checkboxes/switches/radios dispatch checked/value changes using source-tag-aware runtime behavior.
- Menus support roving focus and activation while respecting disabled items.
- Shipped navigation language selection uses native `select`/`option` and still publishes `SetLocale`.
