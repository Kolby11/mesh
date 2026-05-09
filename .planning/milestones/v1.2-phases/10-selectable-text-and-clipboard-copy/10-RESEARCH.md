---
phase: 10
slug: selectable-text-and-clipboard-copy
status: complete
created: 2026-05-06
---

# Phase 10 Research - Selectable Text and Clipboard Copy

## Goal

Add mouse-driven single-node text selection, visible theme-owned highlight, and standard copy-shortcut behavior without regressing existing shell controls.

## Findings

### Current input and event routing

- `crates/core/shell/src/shell/mod.rs` already receives modifier-aware key events as `WindowKeyEvent::Pressed(key, mods)`, but it converts them to `ComponentInput::KeyPressed { key }` before component input handling.
- `crates/core/shell/src/shell/types.rs` therefore drops `ctrl` / `shift` / `alt` before `FrontendSurfaceComponent` sees a keypress, which means Phase 10 cannot currently distinguish `Ctrl+C` from plain `c`.
- `crates/core/shell/src/shell/component/input.rs` currently prioritizes focus, click handlers, slider dragging, checkbox/switch toggles, scrolling, and hover updates. Selection must fit into that routing order without stealing interactions from controls.
- `FrontendSurfaceComponent` already owns stable interaction/runtime state (`focused_key`, `pointer_down_key`, `hovered_path`, input/slider/checked/scroll maps). Phase 10 selection state should live beside those fields and follow the same shell-owned lifecycle.

### Current text and layout model

- `crates/core/ui/elements/src/element.rs` already exposes `text.selectable`, so Phase 10 does not need a new author-facing attribute for opt-in behavior.
- `crates/core/shell/src/shell/component/rendering.rs` rebuilds the tree, annotates runtime state, restyles, and recomputes layout before hit testing and paint. Selection geometry should treat that final post-layout tree as authoritative.
- `crates/core/ui/render/src/surface/painter.rs` renders text from node `text` / `content` attributes and applies ellipsis before painting when `text_overflow == ellipsis`.
- Because the painter may replace visible content with a truncated display string, clipped or ellipsized selection would need hidden-text mapping and copy semantics that do not match the approved Phase 10 boundary. Excluding clipped text for this release is the correct simplification.

### Current renderer boundary

- `crates/core/ui/render/src/surface/text.rs` builds a fresh `cosmic-text` `Buffer` for every measure/render pass today.
- The local renderer does not yet persist any selection metadata, glyph hit-testing helper, or selected-range paint primitive.
- `cosmic-text` upstream already exposes the primitives Phase 10 needs:
  - `Buffer::hit`, `Buffer::layout_cursor`, and `Buffer::layout_runs` for mapping pointer positions and layout ranges.
  - `Editor` and `Selection` for selection-aware drawing/editing workflows.
- Recommended implementation shape: keep Phase 10 on a shell-owned non-editing model and add a narrow helper in `text.rs` that builds selection geometry from `Buffer` APIs. If `Editor` materially simplifies selected-range painting, wrap it locally for draw-time behavior only rather than moving the whole runtime to an editing model.

### Clipboard boundary

- Repository search shows an existing capability classification for `shell.clipboard.write` in `crates/core/foundation/capability/src/lib.rs`, but no actual clipboard write implementation.
- `crates/core/platform/wayland/src/lib.rs` defines `ShellSurface` for surface geometry/visibility only; it has no clipboard method today.
- Phase 10 therefore needs a minimal shell-owned clipboard write abstraction. It should stay write-only for now, live above backend-specific surface plumbing, and remain unavailable to general frontend author code.
- Copy routing should remain conservative: `Ctrl+C` only copies Phase 10 text when a Phase 10 selection exists; otherwise existing focused-control behavior wins.

### Proof fixture recommendation

- The approved UI contract explicitly excludes navigation-bar chrome, sliders, switches, buttons, and other interactive shells as the first proof surface.
- The current module graph already supports enabled frontend modules that are not the root layout entrypoint, as shown by `@mesh/audio-popover`.
- Recommended proof strategy: add a small dedicated read-only frontend surface module or equivalent isolated fixture that contains one selectable text node plus any non-selectable framing copy. Tests should compile and paint that fixture directly, and manual verification can choose whether it starts visible or is toggled in a dev setup.

## Recommended Implementation Shape

1. Preserve modifier state through `ComponentInput` so shell components can detect `Ctrl+C` without inventing shell-global keyboard state.
2. Add shell-owned selection state to `FrontendSurfaceComponent`, keyed by stable `_mesh_key` identity and constrained to a single selectable text node.
3. Add a text-selection helper around `cosmic-text` layout primitives for wrapped-line hit testing, range normalization, visible substring extraction, and selected-range painting.
4. Add theme-owned `color.selection-background` and `color.selection-foreground` tokens and use them only for the selected state.
5. Add a minimal clipboard writer abstraction and route copy only when selection exists.
6. Prove the behavior with a dedicated passive text fixture plus focused shell/render tests for geometry, paint, clipboard payload, and control boundaries.

## Validation Architecture

Use Rust unit and integration-style tests through Cargo. Prefer targeted package tests while developing and full workspace tests before final verification.

- Quick command: `nix develop -c cargo test -p mesh-core-shell -p mesh-core-render selection`
- Full command: `nix develop -c cargo test`
- Primary coverage:
  - Modifier-aware copy routing distinguishes `Ctrl+C` from plain character input.
  - Drag selection starts only on selectable passive text and never on interactive control labels.
  - Wrapped selection maps pointer positions to stable ranges within a single text node.
  - Clipped or ellipsized text remains out of the selection path for this phase.
  - Selected ranges paint with `color.selection-background` / `color.selection-foreground`.
  - Clipboard payload matches the visible selected substring.
  - Selection clears deterministically when focus/intent moves elsewhere or the selected node disappears during rebuild/hide.
- Sampling rule: every task that changes runtime behavior must add or update an automated test in the same task.

## Requirement Reconciliation

- The approved Phase 10 scope is narrower than current `TEXT-04` language in `.planning/ROADMAP.md` and `.planning/REQUIREMENTS.md`.
- Planning should not silently broaden implementation to clipped or nested cross-node selection.
- The first execution plan should explicitly reconcile the milestone docs so downstream verification and later archival reflect the approved wrapped-single-node scope.

## Open Risks

- `cosmic-text` cursor indices are byte-based; selection extraction must be tested against multi-byte text so copy payloads do not split UTF-8 incorrectly.
- Tree rebuilds can invalidate selected node identity mid-drag or between render passes. Phase 10 needs deterministic clear rules when the selected `_mesh_key` disappears.
- Clipboard backend behavior will differ between the dev window path and live Wayland sessions, so at least one manual live-session verification remains appropriate even with strong unit coverage.
- A dedicated proof fixture must stay clearly passive. If implementation drifts toward existing interactive shells, it will violate the approved UI contract.
