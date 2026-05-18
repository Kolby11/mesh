# .mesh Renderer Contract

## Current Author Contract

- `.mesh template/script/style syntax remains the public authoring surface.`
- `Service proxies, theme tokens, locale helpers, capabilities, module dependencies, and explicit component imports remain the integration model.`
- `Focused proof snapshots and crate-facing conversion modules are not public author APIs.`

The renderer migration is internal unless a future migration step explicitly updates this contract. Authors should keep using MESH UI primitives, Luau scripts, scoped styles, module dependencies, and service interfaces rather than depending on candidate renderer crates.

## Stable During Migration

| Area | Contract |
|------|----------|
| Layout and control semantics | Taffy-backed layout computes in-scope `.mesh` row, column, stack, fixed size, gap, padding, absolute positioning, and container-width geometry after Phase 47; current documented MESH UI primitives remain the author-facing layout and control model. |
| Service-driven state updates | Frontends continue to consume interface state and methods through service proxies and declared module dependencies. |
| Theme tokens | Theme token lookup remains the supported styling boundary for shell and module visuals. |
| Theme-owned selection colors | Selection colors remain controlled by shell/theme tokens rather than renderer proof internals. |
| Localized text | Existing locale helpers and module i18n records remain the public text localization path. |
| Keyboard and pointer input | Documented event handlers, focus traversal, and shell surface input behavior remain the supported author contract. |
| Shell surface lifecycle | Surface show, hide, toggle, and layout ownership remain shell-controlled. |
| Diagnostics and profiling | Renderer migration must keep diagnostics and profiling visible through current debug paths or documented replacements. |
| Accessibility direction | Accessibility metadata is moving toward AccessKit-compatible retained-node updates while MESH node identity stays authoritative. |

## Renderer Migration Direction

Renderer migration is phased and reversible, and author-facing behavior changes require the migration gates in docs/renderer-migration.md.

Phase 46 adds disabled-by-default renderer-library features and an internal status seam only; `.mesh` syntax, service proxies, shell surface lifecycle, and author APIs do not change.

After Phase 47, in-scope `.mesh` layout semantics are computed by Taffy-backed layout beneath retained MESH nodes. Author-facing `.mesh` syntax, service proxies, shell surface lifecycle, presentation ownership, diagnostics routes, and runtime identity remain stable.

The current ownership map is in `docs/renderer-ownership.md`. Public author behavior should change only after a migration step updates this document and verifies shipped surface behavior, diagnostics, profiling, selection, and accessibility gates.

## Not Promised

- `.mesh is not HTML/CSS in a browser engine.`
- `Blitz is not the production authoring model.`
- `Winit is not replacing Wayland shell ownership.`
- `Arbitrary DOM/web platform behavior is not promised.`
- `Renderer proof snapshots are migration evidence, not an author API.`

## Diagnostics And Verification

- `NodeId retained identity remains a shell/runtime concern.`
- `typed invalidation remains visible through migration gates.`
- `damage, profiling, diagnostics, and debug payloads remain promotion gates.`
- `AccessKit-compatible retained-node updates are the accessibility migration direction.`

## Deferred Work

- `Audio Popover Transition Delay Polish remains deferred and is not part of renderer migration planning.`
- `Define Module Install Requirement Resolution remains a separate module-system task.`
- `Blitz crate dependency research is already captured by Phase 42 and Phase 43.`
