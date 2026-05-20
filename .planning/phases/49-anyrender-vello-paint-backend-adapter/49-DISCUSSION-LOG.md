# Phase 49: AnyRender/Vello Paint Backend Adapter - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-20
**Phase:** 49-anyrender-vello-paint-backend-adapter
**Areas discussed:** Proof vs. replacement posture, Library focus: anyrender vs vello_encoding, Parley + Vello unification, Display-list command coverage

---

## Proof vs. Replacement Posture

| Option | Description | Selected |
|--------|-------------|----------|
| Proof posture | Adapter encodes display-list commands into anyrender/vello scene and populates FocusedPaintEvidence with encoding evidence. Software painter still produces actual PixelBuffer output. Same pattern as Phase 48 Parley adapter. | ✓ |
| Replacement posture | Adapter replaces the software painter for commands it can handle. Software painter is fallback only for unsupported commands. | |
| You decide | Claude picks posture based on PAINT-03 and Phase 48 D-03 coupling goal. | |

**User's choice:** Proof posture (Recommended)
**Notes:** Consistent with Phase 46/48 adapter posture. Phase 47's strict replacement pattern does not extend to paint in Phase 49.

---

## Library Focus: anyrender vs vello_encoding

| Option | Description | Selected |
|--------|-------------|----------|
| anyrender (Recommended) | Higher-level abstraction. Translates display-list commands into anyrender scene operations. More portable, less GPU-specific. | ✓ |
| vello_encoding | Vello's encoding layer directly. More GPU-specific. | |
| Both as independent adapters | Implement both under their respective feature flags. More scope. | |
| You decide | Claude picks based on phase title and clean proof evidence without GPU dependency. | |

**User's choice:** anyrender (Recommended)
**Notes:** `renderer-vello-encoding` stays scaffolded but unimplemented in Phase 49.

---

## Parley + AnyRender Unification

| Option | Description | Selected |
|--------|-------------|----------|
| Unify Parley into paint adapter (Recommended) | When both renderer-parley and renderer-anyrender are enabled, text nodes are encoded as glyph runs using Parley's shaped output. | ✓ |
| Keep adapters independent | Paint adapter treats text as opaque display-list commands, no Parley coupling. | |
| You decide | Claude picks based on Phase 48 D-03's explicit deferral intent. | |

**User's choice:** Unify Parley into paint adapter (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| Degrade gracefully | anyrender-only: text as opaque commands. parley-only: Phase 48 behavior. Both together: full glyph-run encoding. | |
| Require both or disable | If only one is enabled, combined text-in-paint path is skipped entirely. Non-fatal diagnostic emitted. | ✓ |
| You decide | Claude picks the combination minimizing conditional compilation complexity. | |

**User's choice:** Require both or disable
**Notes:** Avoids complex single-flag conditional logic. Each feature remains independently useful, but the combined glyph-run text path requires both.

---

## Display-List Command Coverage

| Option | Description | Selected |
|--------|-------------|----------|
| Shipped-surface subset (Recommended) | Cover backgrounds, borders, text (glyph runs when both flags active), icons. Sliders, inputs, scrollbars documented as deferred subset. | ✓ |
| All command types | Translate every DisplayPaintContent variant including Slider, Input, Scrollbars. | |
| You decide | Claude picks minimum subset satisfying PAINT-02. | |

**User's choice:** Shipped-surface subset (Recommended)

| Option | Description | Selected |
|--------|-------------|----------|
| Extend FocusedPaintEvidence (Recommended) | Add anyrender_encoded: bool to existing struct. Minimal schema addition. | ✓ |
| New FocusedAnyrenderEvidence struct | Separate anyrender_paint: Vec<FocusedAnyrenderEvidence> in FocusedProofSnapshot. | |
| You decide | Claude picks approach minimizing proof.rs schema churn. | |

**User's choice:** Extend FocusedPaintEvidence (Recommended)
**Notes:** User confirmed "please go with recommended for all" for remaining questions.

---

## Claude's Discretion

- Exact anyrender API surface and scene builder types
- Internal module placement within `mesh-core-render`
- How background colors and border radii map to anyrender primitives
- How icon encoding works (raster blit vs. vector path)
- Whether `anyrender_scene_ops` field is bool, count, or string description
- Exact non-fatal diagnostic message when combined Parley+anyrender path is skipped

## Deferred Ideas

- Full cosmic-text removal — deferred beyond Phase 49 (requires both Parley and anyrender/Vello to be production-authoritative paths)
- `renderer-vello-encoding` implementation — deferred past Phase 49
- Slider, Input, Scrollbars encoding — documented lossless subset per PAINT-01
- anyrender → pixel output (rasterization) — future milestone concern
