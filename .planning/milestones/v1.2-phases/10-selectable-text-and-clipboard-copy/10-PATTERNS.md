---
phase: 10
slug: selectable-text-and-clipboard-copy
status: complete
created: 2026-05-06
---

# Phase 10 Pattern Map

## Existing Patterns To Follow

### Shell-owned stable runtime state

- Primary file: `crates/core/shell/src/shell/component.rs`
- Existing interaction state (`focused_key`, `pointer_down_key`, `hovered_path`, value maps, scroll offsets) is owned by `FrontendSurfaceComponent` and survives rebuilds through stable `_mesh_key` annotation.
- Phase 10 selection state should follow that pattern rather than introducing a separate transient render-only store.

### Modifier-aware window input already exists upstream

- Primary files:
  - `crates/core/ui/render/src/surface/bridge/dev_window.rs`
  - `crates/core/ui/render/src/surface/bridge/wayland_surface.rs`
  - `crates/core/shell/src/shell/mod.rs`
- Backends already produce key events with modifier state. The missing hop is shell routing into `ComponentInput`, not backend capability.
- Phase 10 should preserve that existing flow and extend the component input contract instead of inventing duplicate modifier tracking.

### Post-layout tree is the authoritative interaction surface

- Primary files:
  - `crates/core/shell/src/shell/component/rendering.rs`
  - `crates/core/shell/src/shell/layout.rs`
  - `crates/core/shell/src/shell/component/input.rs`
- Hit testing, hover, focus, click routing, and scroll state all work from the final rebuilt tree.
- Selection geometry and clear behavior should use the same final tree so hit testing, paint, and copy all agree on the same bounds/content.

### Text rendering logic is centralized in render surface helpers

- Primary files:
  - `crates/core/ui/render/src/surface/text.rs`
  - `crates/core/ui/render/src/surface/painter.rs`
- `TextRenderer` already owns `cosmic-text` measurement/render setup.
- `FrontendRenderEngine::render_text_node` already handles wrap width, padding, text alignment, and ellipsis conversion.
- Phase 10 should extend these helpers with selection-aware geometry/paint instead of creating a second text layout stack inside shell code.

### Theme-owned visual state tokens

- Primary files:
  - `config/themes/mesh-default-dark.json`
  - theme/token resolution code already used by supported CSS/property work from Phase 8
- Shell-level state styling is theme-owned today. Selection colors should become first-class theme tokens, not ad hoc component-local constants.

### Dedicated proof surfaces follow the frontend module pattern

- Primary files:
  - `modules/frontend/audio-popover/module.json`
  - `config/package.json`
- Non-root frontend surfaces already exist as enabled modules with their own manifest and surface settings.
- Phase 10's proof fixture should reuse that module pattern if an implementation-level proof surface is needed, rather than bolting selection proof into the navigation bar or audio controls.

## Phase 10 File Ownership

| Area | Files | Plan |
|------|-------|------|
| Input contract, modifier propagation, selection lifecycle, doc reconciliation | `.planning/ROADMAP.md`, `.planning/REQUIREMENTS.md`, `crates/core/shell/src/shell/types.rs`, `crates/core/shell/src/shell/mod.rs`, `crates/core/shell/src/shell/component.rs`, `crates/core/shell/src/shell/component/input.rs` | 10-01 |
| Wrapped selection geometry, selection tokens, highlight rendering | `crates/core/ui/render/src/surface/text.rs`, `crates/core/ui/render/src/surface/painter.rs`, `config/themes/mesh-default-dark.json`, theme/token resolution touchpoints as needed | 10-02 |
| Clipboard write path, proof fixture module, integration regressions | new shell clipboard abstraction file(s), backend bridge touchpoints, `config/package.json`, new dedicated proof fixture frontend module, shell/render tests | 10-03 |

## Tests To Mirror

- Existing `crates/core/shell/src/shell/component/tests.rs` integration-style tests that build real frontend surfaces and assert runtime behavior.
- Existing `crates/core/ui/render/src/lib.rs` and `crates/core/ui/render/src/surface/painter.rs` tests for layout/render behavior.
- Existing Phase 09 patterns for stable runtime-state annotation and post-restyle layout authority.

## Implementation Guidance

- Keep Phase 10 non-editable. Selection is stateful, but it should not turn text nodes into text editors.
- Avoid `WidgetNode::id` as a persisted selection identity. Use stable `_mesh_key` plus node validation against the rebuilt tree.
- Reject clipped or ellipsized text from the Phase 10 selection path early so paint, geometry, and copy all share the same boundary.
- Keep copy routing shell-owned and explicit. Do not make clipboard behavior available as general plugin scripting during this phase.
- Prefer direct geometry/range tests over broad pixel snapshots unless a specific highlight rendering regression requires image-level assertions.
