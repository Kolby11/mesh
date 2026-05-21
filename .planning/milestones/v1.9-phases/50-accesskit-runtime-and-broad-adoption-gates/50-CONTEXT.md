# Phase 50: AccessKit Runtime And Broad Adoption Gates - Context

**Gathered:** 2026-05-21
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 50 replaces proof-only accessibility string evidence with retained-node AccessKit runtime update construction behind the existing renderer-library adapter boundary. It must prove roles, labels, focusable/control metadata, and retained-node identity on shipped navigation/audio surfaces while closing the v1.9 adoption documentation gates.

This phase does not introduce a platform accessibility service, Wayland accessibility publication, or a new public `.mesh` author API. It creates the internal retained-node AccessKit update boundary and documents which renderer-library adapters are production, experimental, or deferred.

</domain>

<decisions>
## Implementation Decisions

### AccessKit Runtime Posture
- Build real `accesskit` update/node structures when `renderer-accesskit` is enabled, using retained MESH `NodeId` identity as the source of stable node ids.
- Keep the current focused proof snapshot as adapter-owned migration evidence, not a public author-facing API.
- Default builds must remain green with the feature disabled; feature-enabled builds must prove the AccessKit path without changing runtime rendering behavior.
- Do not add platform publication or compositor accessibility integration in this phase; that is a later runtime/platform concern.

### Accessibility Metadata Coverage
- Use existing semantic sources first: `WidgetNode.accessibility`, compiler-produced roles/focusability, `role`, `aria-label`, `content`, and focusable/control attributes already present in shipped `.mesh` modules.
- Cover shipped navigation and audio surfaces as canonical adoption gates, matching prior v1.9 phases.
- Preserve non-fatal diagnostics for missing labels or unsupported metadata rather than failing rendering.
- Include focusable/control metadata for native focusable tags and explicit `tabindex`/control cases where existing interaction code already defines behavior.

### Adoption Gates And Documentation
- Update renderer ownership and author-contract docs to classify Taffy, Parley, AnyRender, Vello encoding, and AccessKit with explicit production/experimental/deferred status.
- Keep broad adoption gates executable: default checks, relevant feature-enabled render checks, shipped shell surface checks, and workspace-level smoke checks.
- Any skipped platform/runtime publication should be documented as deferred, not silently implied complete.

### the agent's Discretion
The planner may choose the exact internal AccessKit adapter module shape, whether to extend `FocusedAccessKitUpdate` in place or add a feature-gated conversion helper, and the exact test split across `mesh-core-render`, `mesh-core-frontend`, and `mesh-core-shell`, provided the retained-node identity and default-off rollback contracts remain intact.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/frontend/render/src/proof.rs` already contains `FocusedAccessibilityEvidence`, `FocusedAccessKitUpdate`, and `build_accesskit_update`.
- `crates/core/frontend/render/src/library_adapters.rs` already exposes the disabled-by-default `renderer-accesskit` feature status.
- `crates/core/ui/elements/src/accessibility.rs` defines retained node accessibility roles and focusability metadata.
- `crates/core/frontend/compiler/src/render.rs` maps tags such as button/input/slider/checkbox/switch into accessibility roles and focusability.
- `crates/core/ui/interaction/src/focus.rs` is the existing source of focusability behavior for pointer/keyboard traversal.
- Shipped navigation/audio fixtures already exist in shell component integration tests and have been used as v1.9 gates.

### Established Patterns
- Feature-gated adapter modules live in `mesh-core-render` and are private unless there is a real public crate boundary reason.
- Proof/evidence fields are extended minimally and verified with focused unit tests plus shipped surface integration tests.
- Unsupported or partial library-backed paths emit diagnostics and preserve the current MESH-owned behavior as rollback.
- Tests use real domain structs and shipped component fixtures rather than mocks.

### Integration Points
- `build_focused_proof_snapshot` and `build_accesskit_update` are the current proof-to-accessibility conversion boundary.
- `WidgetNode.accessibility` should be the source of role/focusable/control metadata when available; raw `role`/`aria-label` attributes are fallback evidence only.
- Docs to update: `docs/renderer-migration.md`, `docs/renderer-ownership.md`, and `docs/frontend/renderer-contract.md`.

</code_context>

<specifics>
## Specific Ideas

- Treat AccessKit as closer to the Taffy "production replacement" posture than the AnyRender proof posture only for the internal accessibility update data, not for platform publication.
- Add tests that prove AccessKit node ids remain derived from retained MESH node ids across navigation/audio surfaces.
- Add feature-enabled checks for `renderer-accesskit` and aggregate `renderer-libraries` so adoption docs are backed by executable gates.
- Prefer using AccessKit's real `Node`/`TreeUpdate` types in feature-enabled code rather than string-only placeholders.

</specifics>

<deferred>
## Deferred Ideas

- Platform accessibility service publication.
- Wayland/compositor accessibility protocol integration.
- Full screen-reader manual UAT.
- New author-facing accessibility syntax beyond existing roles, labels, and focus/control metadata.
- Promoting AnyRender/Vello paint to authoritative pixel output.

</deferred>
