# .mesh Renderer Contract

## Current Author Contract

- `.mesh template/script/style syntax remains the public authoring surface.`
- `Service proxies, theme tokens, locale helpers, capabilities, module dependencies, and explicit component imports remain the integration model.`
- `Focused proof snapshots and crate-facing conversion modules are not public author APIs.`

The renderer migration is internal unless a future migration step explicitly updates this contract. Authors should keep using MESH UI primitives, Luau scripts, scoped styles, module dependencies, and service interfaces rather than depending on candidate renderer crates.

## Stable During Migration

| Area | Contract |
|------|----------|
| Layout and control semantics | Current documented MESH UI primitives remain the author-facing layout and control model. |
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

The current ownership map is in `docs/renderer-ownership.md`. Public author behavior should change only after a migration step updates this document and verifies shipped surface behavior, diagnostics, profiling, selection, and accessibility gates.

## Not Promised

## Diagnostics And Verification

## Deferred Work
