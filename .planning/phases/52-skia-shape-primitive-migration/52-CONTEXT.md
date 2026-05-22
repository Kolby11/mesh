# Phase 52: Style Profile And Lowering Compatibility - Context

**Gathered:** 2026-05-22
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 52 locks the bounded XML/.mesh, CSS-like parser, and theme-token style
profile that the painter engine promises to render. It inventories the current
style surface, documents supported/diagnostic/deferred/out-of-scope properties,
and ensures painter-relevant style values lower into backend-neutral render data
without changing author-facing syntax. It should not migrate widget/control
painting, Skia primitive execution, effects, animation invalidation, or damage
policy; those belong to later phases.

</domain>

<decisions>
## Implementation Decisions

### Style Profile Scope
- The supported profile is MESH shell CSS, not browser CSS. Keep XML/.mesh tags
  and existing MESH element vocabulary authoritative; do not add arbitrary HTML
  or DOM compatibility.
- Classify style properties as implemented, diagnostic-only, deferred, or
  out-of-scope. This classification should be visible in docs/tests, not only in
  code comments.
- Preserve existing parser/resolver support for color, size, spacing, border,
  radius, opacity, transform, shadow, filter, layout, font, animation, and
  transition properties that already compile.
- Treat unsupported web-like properties as diagnostics. Silent acceptance of
  properties MESH cannot lower/render is not acceptable.

### Token Compatibility
- Theme tokens remain resolved through the existing `mesh-core-theme` and
  `StyleResolver` path. Do not introduce a parallel token system for painter
  work.
- CSS custom properties that already work as local variables remain supported;
  painter profile documentation should distinguish them from theme tokens.
- Token resolution failures should stay actionable and testable, especially for
  animation and painter-relevant visual properties.
- Shipped navigation/audio module styles are compatibility fixtures for this
  phase.

### Lowering Boundary
- Style data passed toward render objects, display lists, and painter commands
  must remain backend-neutral. No `skia_safe` types belong in `mesh-core-elements`
  style structs, retained display-list data, or render-object data.
- Phase 52 may add profile metadata, documentation, diagnostics, and tests, but
  broad command lowering and helper bypass removal belong to Phase 53.
- Existing `ComputedStyle`, `StyleDiagnostic`, `supported_css_properties`, and
  `StyleResolver` are the preferred integration points.
- Parser/resolver changes should be conservative and compile-safe; avoid new
  parser architecture unless current structures cannot express the profile.

### Autonomous Planning Defaults
- Prefer focused plans that write a support matrix/documentation first, then add
  resolver diagnostics/tests, then prove shipped style compatibility.
- Verification should include targeted `mesh-core-elements` style tests and
  shell/frontend fixtures for shipped navigation/audio styles where existing
  test harnesses make that practical.
- If a property is already parsed but not yet rendered, mark it diagnostic-only
  or deferred according to current behavior rather than pretending painter
  support exists.
- Leave animation behavior implementation to Phase 56 while documenting the
  currently accepted animation property surface.

### the agent's Discretion
The planner may choose exact file names for the style profile document and may
decide whether the property matrix lives in docs, render docs, or
`mesh-core-elements` tests, provided `.planning/REQUIREMENTS.md` traceability and
author-facing compatibility are preserved.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/ui/elements/src/style/types.rs` defines
  `StyleDiagnostic`, `SUPPORTED_CSS_PROPERTIES`, `ComputedStyle`, and the
  backend-neutral style value structs used by layout/render.
- `crates/core/ui/elements/src/style/resolve.rs` owns `StyleResolver`,
  selector matching, token/custom-property resolution, and diagnostics for
  unsupported CSS properties.
- `crates/core/ui/elements/src/style/parse.rs` contains shorthand and visual
  property parsers for transform, overflow, transitions, animation, border,
  font, filters, and shadows.
- `crates/core/ui/component/src/parser/styles.rs` parses `.mesh` style blocks
  before element style resolution.
- `config/themes/*.json` and `modules/frontend/navigation-bar/**/*.mesh` are
  existing token/style compatibility fixtures.

### Established Patterns
- Renderer/style changes should preserve MESH ownership of XML/.mesh parsing,
  CSS-like style resolution, layout, animation state, retained display-list
  ordering, damage, input, module boundaries, and presentation.
- Existing tests in `crates/core/ui/elements/src/style.rs` already assert
  supported CSS property coverage, unsupported property diagnostics, transition
  safe keyframe properties, and animation token diagnostics.
- `StyleResolver` returns resolved style plus diagnostics; this should remain
  the path for author-facing style warnings.
- Tests prefer descriptive behavior names and colocated Rust test modules.

### Integration Points
- `ComputedStyle` feeds retained layout/render state and must stay Skia-free.
- `supported_css_properties()` and `is_supported_css_property()` are natural
  hooks for a documented painter style profile.
- `resolve_node_style_with_diagnostics` is the key path for unsupported property
  diagnostics and shipped style compatibility fixtures.
- Phase 51 decisions D-01 through D-11 remain locked, especially the MESH/Skia
  ownership boundary and no Skia types in retained data.

</code_context>

<specifics>
## Specific Ideas

Create a compact painter-style support matrix that names every supported visual
property category and explains whether it is implemented today, diagnostic-only,
deferred to a later v1.10 phase, or out-of-scope browser compatibility.

</specifics>

<deferred>
## Deferred Ideas

- Element/control command coverage belongs to Phase 53.
- Skia primitive execution belongs to Phase 54.
- Shadows, blur, images, gradients, and layer effects belong to Phase 55.
- Animation invalidation and transition paint integration belong to Phase 56.
- Damage/visual-bounds correctness belongs to Phase 57.
- Backend observability/rollback belongs to Phase 58.
- Full browser CSS, arbitrary HTML parsing, DOM APIs, and browser layout modes
  remain out of scope.

</deferred>
