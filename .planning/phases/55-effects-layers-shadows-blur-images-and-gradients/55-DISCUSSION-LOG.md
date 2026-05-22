# Phase 55: Effects, Layers, Shadows, Blur, Images, And Gradients - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-23
**Phase:** 55-Effects, Layers, Shadows, Blur, Images, And Gradients
**Areas discussed:** Todo folding, Layer and effect semantics, Images and gradients, Diagnostics and capability gaps, Proof and boundaries

---

## Todo Folding

| Option | Description | Selected |
|--------|-------------|----------|
| Do not fold | Weak matches stay out of Phase 55 so the phase remains focused on painter effects/layers. | ✓ |
| Fold both | Capture both pending todos as Phase 55 context even though they are only loosely related. | |
| Review first | Inspect todo contents before deciding. | |

**User's choice:** Runtime fallback selected the conservative default because interactive questions are unavailable.
**Notes:** `2026-05-13-phase31-audio-popover-transition-delay.md` is accepted animation/polish debt. `2026-05-15-define-module-install-requirement-resolution.md` is module/planning scope.

---

## Layer And Effect Semantics

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal explicit layers | Insert `PushLayer`/`PopLayer` only when opacity, blend, filter, backdrop sampling, or clipped descendants require isolation. | ✓ |
| Layer every effectful node | Easier model, but likely adds unnecessary command/layer overhead. | |
| Keep helper-specific effects | Preserves current shape but risks bypassing painter command parity. | |

**User's choice:** Runtime fallback selected the conservative default because interactive questions are unavailable.
**Notes:** MESH keeps retained order, z-index, damage inputs, and command filtering. Skia owns supported effect execution.

---

## Images And Gradients

| Option | Description | Selected |
|--------|-------------|----------|
| Backend-neutral bounded data | Represent images and compact linear gradients in MESH-owned render data, then let Skia execute. | ✓ |
| Skia-native data early | Direct Skia shader/image handles would be convenient but violates backend-neutral retained data. | |
| Browser-compatible syntax | Too broad for the shell UI profile and current v1.10 scope. | |

**User's choice:** Runtime fallback selected the conservative default because interactive questions are unavailable.
**Notes:** Missing/unsupported assets should diagnose instead of silently disappearing. Start with bounded background image/linear-gradient shell UI cases.

---

## Diagnostics And Capability Gaps

| Option | Description | Selected |
|--------|-------------|----------|
| Non-fatal explicit diagnostics | Unsupported combinations emit backend/feature/source context while preserving runtime continuity. | ✓ |
| Hard failures | Too disruptive for authoring/runtime paths except existing parser/manifest invariants. | |
| Silent fallback | Rejected by the v1.10 bounded-profile requirement. | |

**User's choice:** Runtime fallback selected the conservative default because interactive questions are unavailable.
**Notes:** Diagnostics should cover unsupported combinations, excessive blur, missing assets, deferred image forms, and backend capability gaps.

---

## Proof And Boundaries

| Option | Description | Selected |
|--------|-------------|----------|
| Focused effect proof | Cover command lowering, Skia pixels, retained parity, diagnostics, and clipped/out-of-bounds fixtures. | ✓ |
| Shipped surfaces only | Useful compatibility proof, but shipped surfaces may not exercise all supported effects. | |
| Broad damage redesign now | Out of scope; Phase 57 owns deeper damage/stacking policy. | |

**User's choice:** Runtime fallback selected the conservative default because interactive questions are unavailable.
**Notes:** Include visual-bounds fixtures required by Phase 55 success criteria, but avoid Phase 56 animation and Phase 57 damage-policy expansion.

---

## the agent's Discretion

- Exact command names and helper placement.
- Exact fixture split and test names.
- Implementation ordering, provided it starts with backend-neutral data and diagnostics before broad execution paths.

## Deferred Ideas

- Phase 56: animation/transition paint integration.
- Phase 57: broad stacking, clipping, visual-bounds, and damage policy.
- Phase 58: backend capability/rollback observability.
- Phase 59: shipped-surface proof and renderer documentation.
- Phase 31 audio popover transition-delay polish.
- Module install requirement-resolution follow-up.
