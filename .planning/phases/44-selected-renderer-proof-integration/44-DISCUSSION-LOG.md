# Phase 44: Selected Renderer Proof Integration - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-05-18
**Phase:** 44-Selected Renderer Proof Integration
**Areas discussed:** Selected path and boundary, preserved contracts, text/selection/accessibility, dependency and rollout discipline

---

## Selected Path And Boundary

| Option | Description | Selected |
|--------|-------------|----------|
| MESH-owned focused-crate integration | Advance the Phase 43 selected path behind existing MESH renderer/presentation ownership. | Yes |
| Direct Blitz integration | Try to adopt Blitz now despite the high-level crate compile blocker and unresolved shell ownership model. | |
| Broad renderer replacement | Replace `mesh-core-render` or `mesh-core-presentation` wholesale in this phase. | |

**User's choice:** Execute-mode fallback selected the Phase 43 recommendation.
**Notes:** Phase 43 explicitly selected the MESH-owned focused-crate path and handed off a constrained production proof boundary for Phase 44. Blitz remains reference/blocker evidence.

---

## Preserved Contracts

| Option | Description | Selected |
|--------|-------------|----------|
| Preserve existing MESH observability contracts | Keep stable node identity, typed invalidation, damage/profiling payloads, and non-fatal diagnostics visible through existing boundaries. | Yes |
| Emit separate proof-only evidence | Add disconnected adapter evidence that does not flow through current profiling, damage, or diagnostics surfaces. | |
| Relax contracts for faster crate adoption | Let crate integration define identity/invalidation shape first, then reconcile MESH contracts later. | |

**User's choice:** Execute-mode fallback selected preservation of existing MESH contracts.
**Notes:** This directly tracks INTG-01 and Phase 43's preserved-contracts handoff.

---

## Text, Selection, And Accessibility

| Option | Description | Selected |
|--------|-------------|----------|
| Focused proof with MESH-owned semantics | Prove focused text/layout/accessibility while preserving theme-owned selection colors and MESH node IDs. | Yes |
| Static text labels only | Record text shaping evidence without exercising selection geometry or selection paint behavior. | |
| Full accessibility runtime | Attempt broad AccessKit runtime wiring beyond the retained-node update boundary required by INTG-04. | |

**User's choice:** Execute-mode fallback selected focused proof with MESH-owned semantics.
**Notes:** This satisfies INTG-03 and INTG-04 without expanding into future text editing, IME, or complete accessibility runtime scope.

---

## Dependency And Rollout Discipline

| Option | Description | Selected |
|--------|-------------|----------|
| Narrow reversible adapter | Add only the focused crates and adapter/proof boundary needed for the production proof. | Yes |
| Root-level broad adoption | Add the full candidate renderer stack across production crates before proof acceptance. | |
| Skia-first fallback | Shift to Skia/rust-skia before proving the focused Taffy/Parley/AnyRender/AccessKit path. | |

**User's choice:** Execute-mode fallback selected narrow reversible adapter.
**Notes:** Phase 45 owns broad migration planning. Skia remains fallback evidence only if the selected paint boundary cannot satisfy the proof.

---

## the agent's Discretion

- Exact file/module placement for the adapter boundary is left to planning, with a preference for the render-object/display-list area.
- Exact feature gating and dependency shape are left to planning, provided the proof remains reversible and constrained.
- Test grouping is left to planning, but shipped navigation/audio behavior, invalidation/profiling, selection geometry/colors, and AccessKit-compatible node updates must be covered.

## Deferred Ideas

- Audio popover transition delay polish remains deferred in `.planning/todos/pending/2026-05-13-phase31-audio-popover-transition-delay.md`.
