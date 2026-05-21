# Phase 48: Parley Text And Selection Integration - Context

**Gathered:** 2026-05-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 48 introduces a Parley-backed text adapter as proof/evidence infrastructure behind the `renderer-parley` feature flag. It does NOT replace cosmic-text as the authoritative text stack. The goal is to prove Parley's text shaping and line layout output on shipped navigation/audio nodes so that Phase 49 (Vello paint backend) can consume Parley's layout model directly when the full paint path arrives.

cosmic-text remains authoritative for all production text measurement, font discovery, glyph rasterization, selection geometry, and painter behavior. The Parley adapter is adapter-owned evidence only — no production code path changes without the `renderer-parley` feature flag.

</domain>

<decisions>
## Implementation Decisions

### Parley Replacement Posture

- **D-01:** Phase 48 adopts the Phase 46 proof posture for Parley, NOT the Phase 47 strict-replacement posture used for Taffy. Parley's real payoff is as input to the Vello paint backend (Phase 49). Replacing cosmic-text now and again when Vello arrives doubles the work.
- **D-02:** Keep cosmic-text as the authoritative text stack. Parley is added adapter-owned behind the `renderer-parley` Cargo feature, which already exists from Phase 46.
- **D-03:** Full cosmic-text removal is deferred to Phase 49 when Vello is also ready to consume Parley's layout output, making the replacement worthwhile end-to-end.

### Text Measurement Coupling

- **D-04:** The Parley adapter is paint/proof only — it does NOT replace the `TextMeasurer` used by Taffy layout. Layout sizing and intrinsic measurement continue to use cosmic-text's measurement path regardless of whether `renderer-parley` is enabled.
- **D-05:** This keeps the adapter boundary clean: Parley produces shaped text evidence (line positions, glyph data) for the proof snapshot, but does not affect widget geometry or layout in Phase 48.

### Proof Evidence Targets

- **D-06:** The adapter should populate `FocusedTextEvidence.parley_text` in `proof.rs` for shipped navigation/audio text nodes. The existing `parley_text` field already exists in `FocusedProofNode` — the adapter should fill it with real Parley shaping output rather than the current placeholder string.
- **D-07:** Selection evidence (`selection_background`, `selection_foreground`, `selection_anchor`, `selection_focus`) in `FocusedTextEvidence` should be populated from Parley's cursor/line geometry where the feature is enabled, proving anchor/focus coordinates align with shaped glyph positions.

### Fallback And Diagnostics

- **D-08:** When `renderer-parley` is disabled (default), all behavior is identical to Phase 47 output. No diagnostic noise from the Parley code path in the default build.
- **D-09:** When `renderer-parley` is enabled, unsupported text cases (complex emoji, unsupported script coverage, missing fontique font discovery) should surface as non-fatal diagnostics in the proof snapshot rather than panics or silent incorrect output.

### Claude's Discretion

The planner has discretion over exact module placement within `mesh-core-render`, the internal API shape of the Parley adapter struct, font discovery strategy with fontique (system fonts vs. embedded), and how Parley's `Layout` output maps to the existing `FocusedTextEvidence` schema. The proof snapshot schema in `proof.rs` should be respected — extend it minimally rather than replacing it.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone Scope

- `.planning/PROJECT.md` — v1.9 renderer-library integration goal; Phase 48 sits between Taffy (landed) and Vello (Phase 49).
- `.planning/REQUIREMENTS.md` — TEXT-01 through TEXT-03 requirements; note TEXT-03 explicitly requires keeping current text path authoritative for unsupported cases.
- `.planning/ROADMAP.md` — Phase 48 goal ("where ready" language), success criteria, and downstream phase sequencing.
- `.planning/STATE.md` — Carried-forward renderer migration decisions and current milestone state.

### Prior Phase Context

- `.planning/phases/46-renderer-library-dependency-and-adapter-foundation/46-CONTEXT.md` — Establishes the proof/adapter posture, feature flag conventions (`renderer-parley`), and rollback boundary that Phase 48 inherits.
- `.planning/phases/47-taffy-layout-adapter-integration/47-CONTEXT.md` — Phase 47 chose strict replacement for Taffy. Phase 48 deliberately does NOT extend that posture to Parley — read this to understand the contrast.

### Renderer Migration Contracts

- `docs/renderer-migration.md` — Migration principles, promotion gates, and dependency record that Phase 48 may need to update.
- `docs/renderer-ownership.md` — Authoritative vs. adapter-owned boundaries. Parley stays adapter-owned in Phase 48.
- `docs/frontend/renderer-contract.md` — Public `.mesh` renderer contract; Phase 48 must not change author-visible text behavior.

### Current Text Pipeline

- `crates/core/frontend/render/src/surface/text.rs` — Current authoritative `TextRenderer` using cosmic-text (FontSystem, SwashCache). 766 lines covering measurement, caching, ellipsis, alignment, RTL, wrapping, and selection geometry. This is what Parley will eventually replace but must NOT be modified in Phase 48.
- `crates/core/frontend/render/src/surface/painter/text.rs` — Text painter that calls `TextRenderer`, renders selection highlights, and reads `_mesh_selection_*` node attributes for geometry.
- `crates/core/ui/elements/src/layout.rs` — `TextMeasurer` trait and `measure_taffy_node()` — the measurement path that Phase 48 must NOT change.
- `crates/core/frontend/render/src/proof.rs` — `FocusedProofNode.parley_text: Option<FocusedTextEvidence>` and `FocusedTextEvidence` fields — the proof schema the Parley adapter must populate.
- `crates/core/frontend/render/src/library_adapters.rs` — `renderer-parley` feature scaffold and `RendererLibraryStatus` tracking.

### Cargo Configuration

- `Cargo.toml` — Workspace-level `parley = { version = "0.7.0", ... }` dependency already present.
- `crates/core/frontend/render/Cargo.toml` — `renderer-parley = ["dep:parley"]` feature already defined; Parley is already an optional dependency in this crate.

### Shipped Surface Fixtures

- `modules/frontend/navigation-bar/src/main.mesh` — Primary shipped surface; text nodes here are the proof targets.
- `crates/core/shell/src/shell/component/tests` — Existing test infrastructure for shipped-surface proof and invalidation checks.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- `FocusedTextEvidence` in `crates/core/frontend/render/src/proof.rs`: already has `parley_text: String`, `selection_anchor: Option<(f32, f32)>`, and `selection_focus: Option<(f32, f32)>` fields. The Parley adapter should target this schema directly — it was designed for Parley output.
- `renderer-parley` feature in `crates/core/frontend/render/Cargo.toml`: already wired; `parley = "0.7.0"` already in workspace Cargo.toml. The adapter has a ready home.
- `RendererLibraryStatus` for `"parley"` in `library_adapters.rs`: already tracks `enabled: cfg!(feature = "renderer-parley")`. Status reporting is already in place.

### Established Patterns

- Parley adapter is adapter-owned, not authoritative. Follow Phase 46 conventions: new code lives in or below `crates/core/frontend/render`, behind `#[cfg(feature = "renderer-parley")]` guards.
- Proof snapshot is the evidence contract. Populate `FocusedProofNode.parley_text` for text nodes on shipped navigation/audio surfaces. Proof tests in `crates/core/shell/src/shell/component/tests/restyle/selection.rs` already reference `parley_text`.
- Do NOT touch `text.rs` or the `TextMeasurer` trait. The Parley adapter adds to the proof path, not the production paint path.

### Integration Points

- `crates/core/frontend/render/src/proof.rs`: Add Parley adapter call inside `focused_text_evidence()` when `renderer-parley` feature is on, to populate the real `parley_text` string instead of the current placeholder.
- `crates/core/frontend/render/src/lib.rs`: May need to expose the Parley adapter module when the feature is enabled.
- `crates/core/frontend/render/Cargo.toml`: No changes needed — Parley dependency already defined under `renderer-parley` feature.

</code_context>

<specifics>
## Specific Ideas

The user explicitly reversed the initial full-replacement direction after reviewing that Parley only does shaping/layout (not glyph rasterization) and that full replacement would require rebuilding font discovery, glyph painting, and the entire TextRenderer. The proof/adapter posture is the deliberate choice — not a placeholder.

The user selected the proof posture specifically because Parley's value is as a Vello feed. Phase 48 should demonstrate that Parley's shaped line output maps correctly to MESH text nodes so Phase 49 can consume it directly.

</specifics>

<deferred>
## Deferred Ideas

- Full cosmic-text removal — deferred to Phase 49 alongside Vello paint backend integration.
- Parley feeding into TextMeasurer for layout sizing — deferred until Parley is authoritative for shaping (Phase 49+).
- fontique font discovery replacing fontdb/cosmic-text FontSystem — deferred to full replacement milestone.

### Reviewed Todos (not folded)

- Audio Popover Transition Delay Polish — deferred to v1.10 animations/motion-fidelity milestone; not Phase 48 scope.

</deferred>

---

*Phase: 48-Parley Text And Selection Integration*
*Context gathered: 2026-05-18*
