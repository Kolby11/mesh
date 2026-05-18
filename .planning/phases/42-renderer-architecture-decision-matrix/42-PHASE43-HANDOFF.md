# Phase 43 Renderer Prototype Handoff

## Prototype Paths

- Blitz reference path
- MESH-owned focused-crate path

## Required Surfaces

- navigation bar
- audio popover

## Required Interaction Shape

- hover
- click
- slider
- open-close behavior

## Comparable Evidence

Both prototype paths should record visual output and interaction shape against the same navigation bar and audio popover inputs. The comparison should note layout fidelity, paint fidelity, hover/click response, slider response, open-close behavior, and the cost or blocker evidence needed for the Phase 44 integration decision.

## Non-Goals

- production renderer replacement
- real backend runtime
- diagnostics implementation
- profiling implementation

## Harness Constraint

Phase 43 prototypes are throwaway harnesses, not production-wired paths.

## Scope Guard

Do not reduce two-surface prototype scope; navigation bar and audio popover are both required.

## Prototype Inputs

| Surface | Existing source | Required behavior slice |
|---------|-----------------|-------------------------|
| navigation bar | `modules/frontend/navigation-bar/src/main.mesh` | Status text, control cluster, volume trigger, theme/settings buttons, hover and click behavior. |
| audio popover | `modules/frontend/audio-popover/src/main.mesh` | Header/icon state, volume slider, mute button, volume up/down buttons, open-close behavior. |

## Matrix Inputs

- Use `42-DECISION-MATRIX.md` as the source of candidate outcomes and path scores.
- Keep Blitz direct adoption behind the Wayland shell model fit and browser-engine-level performance overhead blockers.
- Treat Taffy, Parley, AnyRender, and AccessKit as the preferred standalone candidates for the focused-crate prototype.
- Keep Skia/rust-skia as fallback evidence unless AnyRender/Vello-style abstraction fails the proof.
