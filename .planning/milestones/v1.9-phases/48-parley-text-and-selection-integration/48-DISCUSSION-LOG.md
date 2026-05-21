# Phase 48: Parley Text And Selection Integration - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-18
**Phase:** 48-parley-text-and-selection-integration
**Areas discussed:** Replacement scope, Text measurement coupling

---

## Replacement Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Replace cosmic-text shaping, keep glyph renderer | Parley owns shaping/layout; existing SwashCache/glyph paint path stays | |
| Full replacement — Parley + fontique | Remove cosmic-text entirely; re-implement font discovery and glyph painting against Parley's output | ✓ (initial) |
| Parley alongside cosmic-text (proof only) | Keep cosmic-text authoritative; Parley as adapter-only evidence behind feature flag | ✓ (final) |

**User's choice:** Initially selected "Full replacement — Parley + fontique", then reversed after clarification.

**Notes:** The user asked whether replacing cosmic-text with Parley actually brings benefits given that many features would need to be reimplemented from scratch. After reviewing that: (1) Parley only does shaping/layout, not glyph rasterization, (2) full replacement would require rebuilding font discovery, the glyph paint pipeline, and the 766-line TextRenderer, and (3) Parley's real value is as input to the Vello paint backend (Phase 49) — the user selected the proof/adapter posture. This deliberately does NOT extend Phase 47's strict-replacement posture to Parley.

---

## Text Measurement Coupling

| Option | Description | Selected |
|--------|-------------|----------|
| Parley adapter is paint/proof only — keep cosmic-text measurement | Parley produces shaped evidence but does not replace TextMeasurer; layout sizing stays driven by cosmic-text | ✓ |
| Parley replaces measurement when feature is on | When renderer-parley enabled, Taffy's measure_text() calls use Parley for accurate line sizing | |

**User's choice:** Parley adapter is paint/proof only — keep cosmic-text measurement.

**Notes:** Keeping the adapter out of the TextMeasurer path maintains a clean separation — Parley shapes text for proof evidence without affecting widget geometry or layout. Deferred measurement replacement to Phase 49+ when full replacement is warranted.

---

## Claude's Discretion

- Exact module placement within `mesh-core-render` for the Parley adapter
- Internal API shape of the Parley adapter struct
- Font discovery strategy with fontique (system fonts vs. embedded)
- How Parley's `Layout` output maps to the existing `FocusedTextEvidence` schema

## Deferred Ideas

- Full cosmic-text removal — deferred to Phase 49 alongside Vello paint backend
- Parley feeding into TextMeasurer for layout sizing — deferred until Parley is authoritative for shaping
- fontique font discovery replacing fontdb/cosmic-text FontSystem — deferred to full replacement milestone
- Selection geometry and fallback policy — not discussed; left to planner discretion
