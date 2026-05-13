---
created: 2026-05-13
source: phase-31-uat
status: pending
type: polish
phase: 31-smoothness-proof-and-cpu-render-tuning
---

# Audio Popover Transition Delay Polish

Phase 31 live UAT confirmed the audio popover now opens/closes functionally and the same-hover trigger close works. The user still sees a slight delay, but explicitly asked to keep it for later instead of continuing the current gap-closure loop.

Future work:

- Design a shell-owned surface show/hide transition lifecycle instead of immediate hide/unmap only.
- Let `@mesh/audio-popover` close with configurable display-transition timing.
- Recheck that transition timing does not reintroduce first-click or first-drag input loss.
