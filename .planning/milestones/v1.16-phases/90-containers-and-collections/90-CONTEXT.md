---
phase: 90-containers-and-collections
title: Containers And Collections Context
status: planned
requirements:
  - ELEMCONTAINER-01
  - ELEMCONTAINER-02
  - ELEMCONTAINER-03
  - ELEMCONTAINER-04
  - ELEMCOLLECT-01
  - ELEMCOLLECT-02
  - ELEMCOLLECT-03
  - ELEMCOLLECT-04
---

# Phase 90 Context

## Goal

Implement the first native container and collection behavior layer for real shell surfaces, with shipped proof in the debug inspector.

## Locked Decisions

- Implement distinct native behavior for `popover`, `dialog`, `tabs`/`tab`, `accordion`, and `details`.
- Keep `panel` and `sheet` as configured containers for now.
- Implement `list`/`list-item` selection and activation first.
- Keep `table`, `tree`, and `empty-state` as semantic/configured elements unless a proof needs more.
- Migrate debug inspector view tabs to native `tabs`/`tab`.
- Migrate one debug inspector list/empty state to `list`/`list-item`/`empty-state`.
- Reuse existing cross-surface popover focus/escape behavior; add in-tree semantics only where tests need it.
- Do not build a full modal focus trap/backdrop system in Phase 90.

## Current Code Shape

- `tabs`, `tab`, `accordion`, `details`, `popover`, `dialog`, `sheet`, `list`, `table`, `tree`, and `empty-state` are already present in the element taxonomy.
- `tabs`/`tab` lower to box primitives, `list` lowers to column, `list-item` lowers to row, and `empty-state` lowers to row.
- Shell focus and keyboard activation are source-aware after Phase 89, but only choice/menu items get custom activation.
- Debug inspector view tabs are currently buttons in `modules/frontend/debug-inspector/src/components/view-tabs.mesh`.
- Debug inspector surface/backend views are currently `column` lists with boxed empty cards and boxed rows.

## Out Of Scope

- Full modal focus trap and backdrop.
- Nested/portal dialogs.
- Table/tree native row/column model.
- Data-driven collection virtualization.
- A broad gallery proof.

## Success Proof

- Tabs and list items preserve source semantics through compiler lowering and accessibility metadata.
- Keyboard activation works for source `tab` and `list-item`.
- Debug inspector uses native tabs and one native collection/empty-state proof without losing existing behavior.
- LSP/docs describe the Phase 90 scope and deferred richer containers/collections.
