# Phase 44 Handoff

## Selected Path

Advance the MESH-owned focused-crate path to Phase 44.

Blitz remains reference/blocker evidence for v1.8. Do not advance direct Blitz adoption until the high-level `blitz` crate compile blocker and production shell ownership questions are resolved.

## Evidence Summary

- Shared fixture: `.planning/prototypes/phase43/fixtures/phase43-scenarios.json`
- Blitz evidence: `.planning/prototypes/phase43/evidence/blitz-reference.md`
- Focused-crate evidence: `.planning/prototypes/phase43/evidence/focused-crate.md`
- Final comparison: `.planning/phases/43-comparable-renderer-prototype-proofs/43-PROTOTYPE-COMPARISON.md`

Blitz result: `PROTO-01: blocker evidence produced`.

Focused-crate result: `PROTO-02: focused-crate retained evidence produced`.

## Integration Boundary

Phase 44 should build a constrained production proof behind existing MESH renderer and presentation ownership. The first proof should adapt retained MESH data into focused-crate layout/text/paint/accessibility evidence without replacing `mesh-core-render` or `mesh-core-presentation` wholesale.

The production path should remain reversible and should not require Winit or Blitz shell ownership.

## Preserved MESH Contracts

The Phase 44 proof must preserve retained node identity, typed invalidation categories, damage/profiling payloads, non-fatal diagnostics, and AccessKit-compatible accessibility boundary.

It should also preserve the current navigation/audio shipped-surface behavior and keep MESH node IDs authoritative across layout, text, paint, interaction, and accessibility mapping.

## Remaining Risks

- The focused-crate proof is structured evidence, not pixel output.
- Taffy and Parley adapter APIs still need real production integration decisions.
- AnyRender backend selection remains open; Skia/rust-skia stays fallback evidence only.
- Blitz dependency cost and compile failure remain useful reference data but not a selected integration path.

## Phase 44 First Proof Targets

- Convert a retained navigation/audio surface slice into focused layout and text evidence behind the existing renderer boundary.
- Preserve typed invalidation categories for geometry, material, text, and accessibility changes.
- Keep damage/profiling payloads visible through the existing debug/profiling path.
- Record non-fatal diagnostics when focused-crate adaptation cannot represent a retained node.
- Emit an AccessKit-compatible retained-node update boundary.

## Out of Scope for Handoff

Phase 45 owns broad migration planning.

Do not design a whole-renderer rewrite in Phase 44. Do not add a general `.mesh` to HTML/Blitz translator. Do not replace Wayland/layer-shell production presentation with Winit.

