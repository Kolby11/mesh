# Phase 49: AnyRender/Vello Paint Backend Adapter - Context

**Gathered:** 2026-05-20
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 49 introduces an anyrender-backed paint adapter behind the retained display-list boundary using the `renderer-anyrender` Cargo feature. It does NOT replace the software painter as the authoritative pixel-output path. The goal is to encode retained display-list commands into an anyrender scene and populate `FocusedPaintEvidence` with encoding evidence, proving that the display-list → paint-library boundary works on shipped navigation/audio surfaces.

The `renderer-vello-encoding` feature stays scaffolded from Phase 46 but is NOT implemented in Phase 49. The software painter (`mesh-software-renderer`) remains authoritative for all actual `PixelBuffer` output.

Separately, when both `renderer-parley` AND `renderer-anyrender` are enabled together, text nodes are encoded as glyph runs using Parley's shaped output into the anyrender scene — fulfilling Phase 48 D-03's deferral. When only one of those flags is active, the combined text-in-paint path is skipped entirely (non-fatal diagnostic emitted).

</domain>

<decisions>
## Implementation Decisions

### Paint Adapter Posture

- **D-01:** Phase 49 uses proof posture, NOT replacement posture. The anyrender paint adapter encodes display-list commands into an anyrender scene and populates `FocusedPaintEvidence` with encoding evidence. The software painter still produces the actual `PixelBuffer` output — anyrender runs in parallel as adapter-owned evidence only.
- **D-02:** The current software painter (`mesh-software-renderer`) remains authoritative for all pixel output and all production rendering behavior when `renderer-anyrender` is disabled (the default).
- **D-03:** This is explicitly NOT the Phase 47 Taffy replacement posture. The anyrender adapter is not expected to take over paint authority in this phase.

### Library Selection

- **D-04:** anyrender (`renderer-anyrender` feature, `anyrender = "0.10.0"`) is the primary implementation target for Phase 49. It is a higher-level paint abstraction that avoids a hard GPU runtime dependency, making it appropriate for a proof-posture phase.
- **D-05:** `renderer-vello-encoding` (`vello_encoding = "0.5.1"`) remains scaffolded from Phase 46 but does NOT receive an implementation in Phase 49. It is not a deferred idea — just not Phase 49 scope.

### Parley + AnyRender Unification

- **D-06:** When BOTH `renderer-parley` and `renderer-anyrender` are enabled simultaneously, text nodes are encoded as glyph runs using Parley's shaped output into the anyrender scene. This fulfills the Phase 48 D-03 deferral ("cosmic-text removal deferred to Phase 49 when Vello is ready to consume Parley layout output").
- **D-07:** When only ONE of `renderer-parley` or `renderer-anyrender` is enabled (but not both), the combined text-in-paint glyph-run path is skipped entirely. A non-fatal diagnostic is emitted to make the skipped path visible. This avoids complex single-flag conditional logic.
- **D-08:** Cosmic-text is NOT removed in Phase 49. It remains the authoritative text measurement, glyph rasterization, and production text rendering path regardless of which feature flags are enabled.

### Display-List Command Coverage

- **D-09:** Phase 49 covers the shipped-surface subset of display-list commands: **backgrounds, borders, text (as glyph runs when both renderer-parley+renderer-anyrender are active), and icons**. This is sufficient to satisfy PAINT-02's shipped navigation/audio surface evidence requirement.
- **D-10:** `DisplayPaintContent::Slider`, `DisplayPaintContent::Input`, and `DisplayPaintCommandKind::Scrollbars` are documented as a deferred lossless subset per PAINT-01's "or to a documented lossless subset" language. They are not encoded by the Phase 49 adapter.

### Paint Evidence Shape

- **D-11:** Extend the existing `FocusedPaintEvidence` struct in `crates/core/frontend/render/src/proof.rs` with an `anyrender_encoded: bool` field (or `anyrender_scene_ops: Option<String>` if a description is more useful for test assertions). Do NOT add a separate `anyrender_paint: Vec<FocusedAnyrenderEvidence>` collection — extend in place to minimize schema growth.

### Claude's Discretion

The planner has discretion over: exact anyrender API surface and scene builder types used; internal module placement within `mesh-core-render`; how background colors and border radii map to anyrender primitives; how icon encoding works (raster blit vs. vector path); whether `anyrender_scene_ops` is a bool, count, or string description; and the exact non-fatal diagnostic message when the combined Parley+anyrender path is skipped.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone Scope

- `.planning/PROJECT.md` — v1.9 renderer-library integration goal; Phase 49 is the paint adapter step between Parley text (Phase 48) and AccessKit runtime (Phase 50).
- `.planning/REQUIREMENTS.md` — PAINT-01 (adapter translates commands to selected library or documented subset), PAINT-02 (shipped-surface paint evidence), PAINT-03 (software painter remains authoritative when disabled).
- `.planning/ROADMAP.md` — Phase 49 goal, success criteria, and sequencing.
- `.planning/STATE.md` — Carried-forward renderer migration decisions.

### Prior Phase Context

- `.planning/phases/46-renderer-library-dependency-and-adapter-foundation/46-CONTEXT.md` — Feature scaffold, renderer-library dependency decisions, adapter posture, rollback boundary. D-05 (current renderer default), D-06 (feature naming), D-07 (local bypass in addition to feature flag), D-09 (FocusedProofSnapshot is adapter-owned evidence).
- `.planning/phases/47-taffy-layout-adapter-integration/47-CONTEXT.md` — Strict replacement posture for Taffy. Phase 49 deliberately does NOT extend this posture to paint — read this to understand the contrast.
- `.planning/phases/48-parley-text-and-selection-integration/48-CONTEXT.md` — Proof posture for Parley. D-03 defers cosmic-text removal to Phase 49 when Vello is ready; D-04 keeps Parley out of TextMeasurer. Phase 49 inherits both decisions.

### Renderer Migration Contracts

- `docs/renderer-migration.md` — Migration principles, promotion gates, and dependency record. Phase 49 should update the anyrender adoption status entry.
- `docs/renderer-ownership.md` — Authoritative vs. adapter-owned boundaries. anyrender paint adapter stays adapter-owned in Phase 49.
- `docs/frontend/renderer-contract.md` — Public `.mesh` renderer contract; Phase 49 must not change author-visible paint behavior.

### Current Paint Pipeline

- `crates/core/frontend/render/src/display_list.rs` — `DisplayPaintCommand`, `DisplayPaintCommandKind` (Node, Scrollbars), `DisplayPaintContent` (Text, Icon, Slider, Input, None), `DisplayPaintNode`, `DisplayPaintStyle`, `SelectedDisplayListPaint`. This is the boundary the anyrender adapter translates from.
- `crates/core/frontend/render/src/surface/painter.rs` — Current software painter; authoritative pixel output path. Not modified by Phase 49.
- `crates/core/frontend/render/src/proof.rs` — `FocusedProofSnapshot`, `FocusedPaintEvidence` (extend with `anyrender_encoded` field), `FocusedProofNode`. Phase 49 extends this schema minimally.
- `crates/core/frontend/render/src/library_adapters.rs` — `renderer-anyrender` and `renderer-vello-encoding` feature stubs; `CURRENT_RENDERER_AUTHORITY`; `RendererLibraryStatus` tracking. The anyrender entry (id: "anyrender", feature: "renderer-anyrender", role: "paint-experimental") is already scaffolded here.
- `crates/core/frontend/render/src/parley_adapter.rs` — Phase 48 Parley adapter (created in Phase 48 Plan 01). When `renderer-parley` + `renderer-anyrender` are both active, the anyrender adapter should consume Parley's `Layout` output from this module for text glyph-run encoding.

### Cargo Configuration

- `Cargo.toml` — Workspace-level `anyrender = { version = "0.10.0", default-features = false }` already present.
- `crates/core/frontend/render/Cargo.toml` — `renderer-anyrender = ["dep:anyrender"]` feature already defined; anyrender is already an optional dependency.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- `crates/core/frontend/render/src/library_adapters.rs`: `renderer_library_statuses()` already reports `renderer-anyrender` enabled/disabled status — the paint adapter can use `cfg!(feature = "renderer-anyrender")` directly as established by Phase 46.
- `crates/core/frontend/render/src/parley_adapter.rs` (Phase 48): Parley `FontContext` and `Layout` output already available under `renderer-parley`. The anyrender adapter can `#[cfg(all(feature = "renderer-anyrender", feature = "renderer-parley"))]` import from this module for glyph-run text encoding.
- `crates/core/frontend/render/src/proof.rs` `FocusedPaintEvidence`: Already has `node_id`, `stable_node_id`, `display_slot`. Adding `anyrender_encoded: bool` is a minimal extension that stays consistent with the Phase 46 D-09 "extend minimally" principle.

### Established Patterns

- **Feature-gated adapter modules**: Phase 48 placed the Parley adapter in `crates/core/frontend/render/src/parley_adapter.rs` under `#[cfg(feature = "renderer-parley")]`. The anyrender adapter should follow the same pattern — a new `anyrender_adapter.rs` (or `paint_adapter.rs`) in the same crate, gated by `#[cfg(feature = "renderer-anyrender")]`.
- **Non-fatal diagnostics via `FocusedProofDiagnostic`**: Phase 48 used non-fatal diagnostics in the proof snapshot for unsupported cases. The "combined Parley+anyrender path skipped" diagnostic when only one feature is active should follow this same pattern.
- **Proof snapshot as the evidence surface**: The existing `proof_snapshot_captures_*` test pattern in `mesh-core-render` tests is the right target for Phase 49 anyrender encoding evidence assertions.

### Integration Points

- `SelectedDisplayListPaint::commands()` is the feed point — the anyrender adapter iterates this slice and translates each `DisplayPaintCommand` into anyrender scene operations.
- `FocusedProofSnapshot::paint` field (type `Vec<FocusedPaintEvidence>`) is where encoding evidence lands. The adapter must set `anyrender_encoded = true` on commands it successfully encodes.
- The display-list → selected paint slice path: `RetainedDisplayList::select_paint_commands()` → `SelectedDisplayListPaint` → anyrender adapter → encoding evidence.

</code_context>

<specifics>
## Specific Ideas

- Phase 48 D-03 specifically linked cosmic-text removal to "when Vello is also ready to consume Parley's layout output." Phase 49 partially fulfills this by encoding Parley-shaped glyph runs into anyrender when both flags are active — but cosmic-text removal itself remains deferred (Phase 49 is proof posture).
- The `anyrender_encoded: bool` field on `FocusedPaintEvidence` is intentionally coarse for Phase 49 — it proves the adapter ran and processed the command, not that it produced identical pixel output to the software painter. Pixel parity is not a Phase 49 success criterion.

</specifics>

<deferred>
## Deferred Ideas

- **Full cosmic-text removal**: Deferred beyond Phase 49. Removal becomes worthwhile only after both Parley text and anyrender/Vello paint are authoritative production paths, not proof-only adapters.
- **`renderer-vello-encoding` implementation**: Scaffolded from Phase 46, deferred past Phase 49. Phase 50 (AccessKit) or a future milestone is a better home once anyrender proof is landed.
- **Slider, Input, Scrollbars encoding**: Documented deferred subset per PAINT-01's "lossless subset" language. Not Phase 49 scope.
- **anyrender → pixel output**: Turning the anyrender scene into actual pixels (rasterization step) is a future phase concern. Phase 49 only proves encoding correctness via the proof snapshot.

</deferred>

---

*Phase: 49-anyrender-vello-paint-backend-adapter*
*Context gathered: 2026-05-20*
