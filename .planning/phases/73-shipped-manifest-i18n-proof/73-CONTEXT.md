---
phase: 73-shipped-manifest-i18n-proof
created: 2026-05-24
status: discussed
autonomous: true
---

# Phase 73 Context

## Goal

Migrate bundled manifests and publish docs/tests proving the author contract for localized manifest text.

## Requirements

- MPROOF-01: Bundled navigation-bar keybind metadata uses explicit localized text objects instead of raw dotted keys.
- MPROOF-02: Bundled layout/settings examples use either literal strings or explicit localized text objects consistently.
- MPROOF-03: Regression tests cover parsing, propagation, runtime resolution, diagnostics, and fallback behavior.
- MPROOF-04: Author docs explain how `mesh.i18n`, `mesh.contributes.i18n`, and `{ "t": "...", "fallback": "..." }` work together.

## Current State

- Phase 70 added `LocalizedText` parsing and migration diagnostics for suspicious raw dotted key strings.
- Phase 71 preserves localized text through installed graph keybind and layout records.
- Phase 72 resolves localized keybind text in runtime `this.keybinds` and debug keybind metadata, with missing-key diagnostics.
- `modules/frontend/navigation-bar/module.json` still declares keybind label, description, and category as raw dotted strings.
- `modules/frontend/navigation-bar/config/i18n/{en,sk}.json` already contains the matching keybind catalog entries.
- `modules/frontend/audio-popover/module.json` uses literal keybind text and has audio i18n catalogs for UI copy.
- `docs/module-system.md` mentions `mesh.i18n` and `mesh.contributes.i18n` but does not yet document field-local localized text objects in the keybind section.

## Decisions

- Migrate `@mesh/navigation-bar` keybind metadata to explicit translation objects because its current raw dotted strings are exactly the ambiguity this milestone removes.
- Keep plain human text as literal strings for layout/settings examples unless a field is intentionally backed by a catalog key.
- Add shipped regression tests that load the real module manifests and assert source metadata survives into graph/runtime/debug boundaries.
- Update author docs in the canonical module-system guide rather than creating a separate localization page.

## Risks

- Real shipped module fixtures may use both canonical and legacy manifest shapes; tests should target behavior rather than force a broader migration.
- Missing catalog keys in shipped modules should fail tests before runtime diagnostics appear in normal shell use.
- Documentation must avoid implying that raw dotted strings localize; raw strings are literals.
