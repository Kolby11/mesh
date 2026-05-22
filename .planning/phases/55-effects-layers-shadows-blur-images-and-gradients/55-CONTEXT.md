# Phase 55: Effects, Layers, Shadows, Blur, Images, And Gradients - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning
**Mode:** Smart discuss fallback defaults accepted automatically because interactive questions are unavailable in this runtime mode.

<domain>
## Phase Boundary

Phase 55 implements the compact CSS visual-effects subset expected from the
MESH painter engine. It should lower supported box shadows, blur,
backdrop-filter blur, opacity, blend behavior, images, gradients, and clipping
combinations into backend-neutral painter commands and Skia-backed execution
where current contracts already allow it. It must define source/lifetime rules
for image and gradient render data that fit existing module assets, theme/token
style data, retained display-list identity, and diagnostics. It should not
redesign the animation system, move visual-bounds/damage ownership into the
backend, implement full browser CSS behavior, or add a production Vello backend.

</domain>

<decisions>
## Implementation Decisions

### Layer And Effect Semantics
- **D-01:** Treat Skia as authoritative for supported effect execution, but keep
  layer creation, retained ordering, z-index, clipping intent, damage inputs,
  and command filtering owned by MESH.
- **D-02:** Lower opacity, non-default blend behavior, filter, backdrop-filter,
  and clip/effect combinations into explicit `PushLayer`/`PopLayer`,
  `ApplyFilter`, `DrawShadow`, `DrawImage`, and gradient/image draw commands
  rather than ad hoc direct helper bypasses.
- **D-03:** Prefer minimal layer insertion: create layers only when a style
  combination needs isolation for opacity, blend, filter, backdrop sampling, or
  clipped descendants. Plain background/border/shadow paths should remain simple
  command sequences when isolation is unnecessary.
- **D-04:** Preserve existing direct widget-tree and retained display-list
  equivalence. Any new direct-path effect lowering must have retained replay
  parity or an explicit documented gap.

### Images And Gradients
- **D-05:** Represent images and gradients in backend-neutral painter data; no
  `skia_safe` image, shader, paint, or filter types may enter
  `mesh-core-elements`, render-object data, display-list data, or style data.
- **D-06:** Image sources should resolve through existing module asset and icon
  source boundaries. Missing, inaccessible, or unsupported image assets should
  emit diagnostics instead of silently painting nothing.
- **D-07:** Gradients should start with the bounded profile already named by the
  style matrix: compact linear-gradient support sufficient for shell UI
  backgrounds. Avoid arbitrary browser gradient syntax unless the parser/profile
  already declares it supported.
- **D-08:** Reuse existing image/icon raster cache and profiling concepts where
  practical, but do not make cache expansion the primary goal of this phase.
  Cache tuning belongs to later proof work if profiling shows misses.

### Diagnostics And Capability Gaps
- **D-09:** Unsupported effect combinations, excessive blur, unsupported blend
  modes, missing assets, deferred image forms, and backend capability gaps must
  produce concise painter diagnostics with backend id, feature id, message, and
  source node/style context where available.
- **D-10:** Diagnostics should be non-fatal for authoring/runtime paths unless a
  malformed asset or style value violates an existing parser/manifest invariant.
  The rendered result may fall back or omit the unsupported effect, but the gap
  must be inspectable.
- **D-11:** Keep capability checks backend-neutral. Skia may report support for
  implemented effects; Vello compatibility should remain an API-shape concern,
  not a production backend requirement in Phase 55.

### Proof And Boundaries
- **D-12:** Verification should focus on command lowering, Skia effect pixels,
  retained replay parity, diagnostics, and clipped/out-of-bounds effect cases.
- **D-13:** Include visual-bounds fixtures for shadows/filter/image/gradient
  output because Phase 55 success criteria names them, but avoid broad damage
  policy redesign. Deeper damage/stacking policy is Phase 57.
- **D-14:** Keep animation-specific invalidation and animated visual bounds out
  of Phase 55 unless needed to preserve current non-animated styles. Phase 56
  owns current CSS/token animation integration.
- **D-15:** Shipped navigation/audio surfaces remain compatibility proof, but
  this phase should also add targeted synthetic fixtures for effects because
  shipped surfaces may not exercise every supported combination.

### Pending Todos
- **D-16:** Do not fold the pending Phase 31 audio popover transition-delay todo
  into Phase 55. It is accepted polish debt and belongs to animation/polish
  follow-up, not effect/layer implementation.
- **D-17:** Do not fold the module install requirement-resolution todo into
  Phase 55. It is module/planning scope, not painter-effect scope.

### the agent's Discretion
The planner may choose exact command names, fixture names, and implementation
ordering. Prefer small slices that first lock backend-neutral data and
diagnostic expectations, then migrate one effect family at a time. If a style is
already parsed but cannot be safely executed in this phase, mark it as explicit
diagnostic/deferred behavior rather than pretending visual support exists.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project And Requirements
- `.planning/PROJECT.md` — Current v1.10 painter-engine goal, ownership split,
  and milestone scope.
- `.planning/REQUIREMENTS.md` — Phase 55 requirements EFFECT-01, EFFECT-02,
  EFFECT-03, and LAYER-01 plus later-phase boundaries.
- `.planning/ROADMAP.md` — Phase 55 roadmap entry, autonomous task seed, and
  success criteria.
- `.planning/spikes/MANIFEST.md` — Validated painter-engine direction and
  browser-scope exclusions from the Skia/painter roadmap spikes.

### Prior Phase Context
- `.planning/phases/52-skia-shape-primitive-migration/52-CONTEXT.md` — Bounded
  style profile, token compatibility, backend-neutral style lowering, and
  deferred effects scope.
- `.planning/phases/53-element-and-display-list-primitive-coverage/53-CONTEXT.md`
  — Direct/retained command parity, retained identity/order ownership, and
  deferred Phase 55 effect/layer/image/gradient scope.
- `.planning/phases/54-skia-shape-path-text-highlight-and-border-migration/54-CONTEXT.md`
  — Skia authoritative shape execution, text boundary, key painter files, and
  backend-neutrality verification expectations.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/frontend/render/src/surface/painter/backend.rs` defines
  `PainterCommand`, `PainterLayer`, `PainterPaint`, `PainterFilter`,
  `PainterImage`, backend capabilities, diagnostics, Skia command execution,
  and current deferred image/filter diagnostics.
- `crates/core/frontend/render/src/surface/painter/tree.rs` already lowers
  box shadow, backdrop filter, background filter, border, text, input, slider,
  and icon behavior from both direct widget-tree and retained display-node
  paths.
- `crates/core/frontend/render/src/display_list.rs` already stores
  `box_shadow`, `filter`, and `backdrop_filter` in `DisplayPaintNode`, hashes
  them for retained reuse, expands visual clips/damage candidates for shadow and
  blur radius, and adds batching barriers for opacity/translucency/clip.
- `crates/core/ui/elements/src/style/types.rs` and
  `crates/core/ui/elements/src/style/parse.rs` already expose bounded
  `BoxShadow`, `VisualFilter`, filter/backdrop-filter, and style-profile
  metadata for image/gradient categories.
- `crates/core/frontend/render/src/surface/icon.rs` and existing image/icon
  raster metrics provide asset/raster precedent for image-related painter work.

### Established Patterns
- MESH-owned style/layout/animation/display-list/damage/presentation data must
  stay backend-neutral and Skia-free.
- Direct rendering and retained display-list replay should converge on the same
  command classes before broad pixel proof.
- Unsupported browser-like style behavior should produce diagnostics rather than
  silent missing visuals.
- Tests are colocated in renderer/style modules with descriptive behavior names;
  focused render tests should use local Nix graphics library paths when Skia or
  font/image dependencies require them.

### Integration Points
- Direct path: `FrontendRenderEngine::render_node_self` and related helpers in
  `crates/core/frontend/render/src/surface/painter/tree.rs`.
- Retained path: `DisplayPaintNode`, `DisplayPaintContent`, and
  `render_display_node_self` through `crates/core/frontend/render/src/display_list.rs`
  and `crates/core/frontend/render/src/surface/painter/tree.rs`.
- Backend path: `PaintBackend::execute_commands` and Skia command handling in
  `crates/core/frontend/render/src/surface/painter/backend.rs`.
- Style path: `StyleResolver`, `ComputedStyle`, `BoxShadow`, `VisualFilter`,
  `SUPPORTED_CSS_PROPERTIES`, and style diagnostics in
  `crates/core/ui/elements/src/style/**`.
- Proof path: `crates/core/frontend/render/src/surface/painter/tests.rs`,
  display-list retained tests, and shipped navigation/audio shell regression
  tests.

</code_context>

<specifics>
## Specific Ideas

The target proof should read like:

```text
ComputedStyle effect/image/gradient data
  -> retained DisplayPaintNode / DisplayPaintContent stays backend-neutral
  -> direct and retained paths emit equivalent PainterCommand classes
  -> Skia executes supported commands or emits explicit painter diagnostics
  -> visual bounds include the supported out-of-layout pixels needed by Phase 55
```

Start with supported shell UI cases, not browser compatibility. A practical
first gradient/image target is "MESH background image/linear gradient for
bounded boxes", not arbitrary CSS image syntax.

</specifics>

<deferred>
## Deferred Ideas

- Animation/transition invalidation, paint-only animation updates, and animated
  visual-bounds correctness belong to Phase 56.
- Broad stacking, clipping, visual-bounds, and damage-policy refinement belongs
  to Phase 57.
- Backend selection/rollback observability and expanded capability reporting
  belong to Phase 58.
- Full shipped-surface proof and renderer documentation closure belong to Phase
  59.
- Phase 31 audio popover transition-delay polish remains accepted debt outside
  Phase 55.
- Module install requirement resolution remains module/planning scope outside
  Phase 55.

</deferred>

---

*Phase: 55-Effects, Layers, Shadows, Blur, Images, And Gradients*
*Context gathered: 2026-05-23*
