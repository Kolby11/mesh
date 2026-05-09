# Phase 9: Responsive and Interaction Reactivity - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-05
**Phase:** 9-Responsive and Interaction Reactivity
**Areas discussed:** Restyle trigger granularity, Interaction state authority, State preservation contract, Proof surfaces

---

## Area Selection

Interactive `AskUserQuestion` was unavailable in Codex Default mode. The workflow fallback selected the recommended option: discuss all identified gray areas with conservative builder defaults.

| Option | Description | Selected |
|--------|-------------|----------|
| All areas | Covers trigger granularity, interaction state, state preservation, and proof surfaces before context is written. | ✓ |
| State authority | Focuses on hover/focus/active/checked ownership and pseudo-state restyling. | |
| Restyle triggers | Focuses on size/container-query invalidation and refresh timing. | |

**User's choice:** Fallback default selected because question UI was unavailable.
**Notes:** Decisions are marked as reviewable builder defaults in CONTEXT.md.

---

## Restyle Trigger Granularity

| Option | Description | Selected |
|--------|-------------|----------|
| Component-boundary invalidation | Compare current dimensions/state at the surface component boundary and run the normal rebuild/layout/paint path. | ✓ |
| In-place visual restyle only | Try to patch styles on the existing tree without a full rebuild. | |
| Full plugin reload | Reload the component source/runtime when size or state changes. | |

**User's choice:** Builder default: component-boundary invalidation.
**Notes:** Full reload conflicts with REACT-04. In-place restyle may be an optimization, but a conservative rebuild path better protects synchronization.

---

## Interaction State Authority

| Option | Description | Selected |
|--------|-------------|----------|
| Stable runtime keys | Preserve hover/focus/active/checked by `_mesh_key`/path across rebuilt trees. | ✓ |
| `NodeId` state | Use `mesh-core-elements::InputState` and node IDs as the authoritative state. | |
| Hybrid with new stable IDs | Introduce persistent IDs first, then use element-level state as authority. | |

**User's choice:** Builder default: stable runtime keys.
**Notes:** `WidgetNode` trees are rebuilt, so transient `NodeId` identity is risky for persistent interaction state.

---

## State Preservation Contract

| Option | Description | Selected |
|--------|-------------|----------|
| Preserve all runtime interaction state | Keep input values, sliders, checked state, scroll offsets, service/settings state, and focus/hover where valid. | ✓ |
| Preserve script state only | Keep Luau globals/service payloads but reset local interaction state. | |
| Reset on structural change | Clear state whenever restyle changes the tree structure. | |

**User's choice:** Builder default: preserve all runtime interaction state where the keyed node still exists.
**Notes:** This matches REACT-04 and current component state stores.

---

## Proof Surfaces

| Option | Description | Selected |
|--------|-------------|----------|
| Engine tests plus one real component regression | Cover resolver/layout/hit-test primitives and prove the shell component does not lose runtime state. | ✓ |
| Engine-only tests | Keep all tests in `mesh-core-elements`/`mesh-core-render`. | |
| Navigation-bar proof now | Use navigation-bar as the main proof surface. | |

**User's choice:** Builder default: engine tests plus one real component regression.
**Notes:** Phase 13 owns full navigation-bar proof; Phase 9 still needs a real component regression because state preservation lives in shell component code.

## the agent's Discretion

- Exact invalidation API and test fixtures.
- Whether to rebuild trees with injected state or use in-place `restyle_subtree`, as long as state and synchronization guarantees are met.

## Deferred Ideas

- Text selection, keyboard traversal/shortcuts, animation tokens/keyframes, and full navigation-bar proof remain later phases.
