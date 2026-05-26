# Roadmap: MESH

## Milestones

- ⏭️ **v1.16 Elements Improvements** — queued after v1.15
- ✅ **v1.15 Persistent Storage System** — Phases 81-85 shipped 2026-05-26 ([archive](milestones/v1.15-ROADMAP.md))
- ✅ **v1.14 Unified Luau Scripting Runtime** — Phases 74-80 shipped 2026-05-26 ([archive](milestones/v1.14-ROADMAP.md))
- ✅ **v1.13 Manifest I18n Contract** — Phases 70-73 shipped 2026-05-24 ([archive](milestones/v1.13-ROADMAP.md))
- ✅ **v1.12 Module Object Contract** — Phases 65-69 shipped 2026-05-23 ([archive](milestones/v1.12-ROADMAP.md))
- ✅ **v1.11 Surface Keybind Completion** — Phases 60-64 shipped 2026-05-23 ([archive](milestones/v1.11-ROADMAP.md))

## Current Status

v1.15 shipped `self.storage` as shell-backed, component/provider
instance-scoped persistent key-value storage. The implementation includes
scoped JSON files, Luau `self.storage` bindings, lifecycle load/flush behavior,
render dependency integration, diagnostics, tests, docs, and shipped navigation
language-selector proof.

Active requirements have been archived. Start the next milestone with
`$gsd-new-milestone`.

## Queued Milestone: v1.16 Elements Improvements

**Goal:** Add common native markup controls that reduce custom component
workarounds and improve shipped UI behavior.

**Planned scope:**

- First-class `<select>` and `<option>` element support in MESH markup
- Visible dropdown/popup behavior with vertical option layout
- Keyboard navigation, focus, selection, disabled states, and accessibility metadata
- Value binding/change events suitable for Luau component state
- Styling hooks that fit the existing shell CSS profile without requiring browser CSS compatibility
- Shipped proof by replacing the navigation bar language selector's horizontal custom menu

## Backlog

### Future: Package Distribution

Remote package fetching, third-party dependency resolution, and LSP import
completion remain future work after the runtime import contract is stable.
