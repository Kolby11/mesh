# Phase 43 Prototype Comparison

## Shared Fixture

Shared fixture: .planning/prototypes/phase43/fixtures/phase43-scenarios.json

Both prototype paths used the same required scenario IDs:

- `nav-baseline`
- `nav-audio-trigger-hover`
- `audio-popover-visible`
- `audio-slider-change-release`
- `audio-popover-close`

## Blitz Reference Path

The Blitz reference path produced structured fallback evidence at `.planning/prototypes/phase43/output/blitz-reference.json` and blocker documentation at `.planning/prototypes/phase43/evidence/blitz-reference.md`.

Direct Blitz render evidence is blocked by a reproducible compile failure in `blitz-0.3.0-alpha.4`:

```text
error[E0425]: cannot find value `event_loop` in this scope
```

The attempted command was:

```bash
cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml --features blitz-reference
```

## MESH-Owned Focused-Crate Path

The focused-crate path produced retained structured evidence at `.planning/prototypes/phase43/output/focused-crate.json` and documentation at `.planning/prototypes/phase43/evidence/focused-crate.md`.

The output keeps MESH `stable_node_id` as the authority across layout, text, paint, interaction, and accessibility records. It includes `taffy_layout`, `parley_text`, `display_slot`, and `accesskit_node_id` evidence.

## Final Comparison Matrix

| Heading | Blitz reference path | MESH-owned focused-crate path | Decision effect |
|---------|----------------------|-------------------------------|-----------------|
| visual/layout fidelity | Structured HTML/CSS-equivalent output exists for all five scenarios, but direct Blitz render/pixel evidence is blocked by the current `blitz` crate compile failure. | Structured retained output exists for all five scenarios, including `taffy_layout` records for retained nodes. | Focused path is stronger for Phase 44 because it produced retained evidence without a dependency compile blocker. |
| interaction shape | Fixture-level interaction records exist for hover, click, slider change/release, and close behavior; live Blitz event dispatch was not proven. | Fixture-level interaction records exist for hover, click, slider change/release, and close behavior while preserving retained node targets. | Focused path better matches current MESH retained interaction boundaries. |
| retained identity fit | Fallback output can map fixture nodes, but direct Blitz DOM ownership and shell integration were not proven. | Every layout, text, paint, interaction, and accessibility record preserves `stable_node_id`. | Focused path advances because retained identity is explicit and native to the proof. |
| accessibility boundary | Fallback maps retained nodes to `blitz-accesskit-node-*`, but Blitz shell accessibility was not exercised. | Evidence maps retained nodes to `accesskit_node_id::*` with role and label fields. | Focused path gives the clearer Phase 44 AccessKit-compatible boundary. |
| build/dependency cost | Enabling the Blitz feature locked 318 additional packages and failed inside `blitz-0.3.0-alpha.4`. | Default focused harness compiles with the isolated prototype manifest and no root workspace dependency adoption. | Focused path is lower risk for constrained production proof. |
| blocker evidence | Concrete blocker: `cargo check --features blitz-reference` fails with `error[E0425]: cannot find value event_loop in this scope`. | No blocker found in the structured retained evidence path. | Blitz remains reference/blocker evidence only for v1.8 Phase 44. |
| Phase 44 integration readiness | Not ready for production proof until Blitz compile/API blocker and shell ownership model are resolved. | Ready for a constrained Phase 44 production proof behind existing MESH retained renderer/presentation boundaries. | Advance MESH-owned focused-crate path to Phase 44. |

## Phase 44 Recommendation

Advance the MESH-owned focused-crate path to Phase 44.

The selected path should integrate behind a constrained boundary that preserves existing MESH retained node identity, typed invalidation categories, damage/profiling payloads, non-fatal diagnostics, and AccessKit-compatible accessibility mapping. Blitz should remain a reference architecture and blocker record until its current high-level crate compiles cleanly and its shell ownership model can be evaluated without production Wayland/layer-shell risk.

## Requirement Coverage

| Requirement | Status | Evidence |
|-------------|--------|----------|
| PROTO-01 | blocked-with-evidence | `.planning/prototypes/phase43/evidence/blitz-reference.md` records the attempted harness, crate/API boundary, compile error, and reproduction command. |
| PROTO-02 | covered | `.planning/prototypes/phase43/evidence/focused-crate.md` and `output/focused-crate.json` record retained focused-crate evidence. |
| PROTO-03 | covered | Both paths use `.planning/prototypes/phase43/fixtures/phase43-scenarios.json` and this comparison uses the mandated headings. |

