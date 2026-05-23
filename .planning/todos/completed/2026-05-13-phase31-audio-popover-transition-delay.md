---
created: 2026-05-13
source: phase-31-uat
status: completed
type: polish
phase: 31-smoothness-proof-and-cpu-render-tuning
resolved_by: phase-56-animation-and-transition-paint-integration
completed: 2026-05-23
---

# Audio Popover Transition Delay Polish

Phase 31 live UAT confirmed the audio popover opens/closes functionally and the same-hover trigger close works. The user still saw a slight delay, but explicitly asked to keep it for later instead of continuing the Phase 31 gap-closure loop.

## Resolution

Phase 56 folded this item into shipped animation compatibility proof:

- `@mesh/audio-popover` exposes a bounded `hide_transition_ms()` of `120`.
- Surface exit state is exercised through `set_surface_exiting(true)` and root `mesh-surface-exiting` class proof.
- The first input after opening the audio popover is covered by `shipped_navigation_audio_popover_transition_does_not_consume_first_input`, which focuses the slider, sends `ArrowRight`, and verifies the `mesh.audio` `set_volume` service command path.

## Evidence

- `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs`
- `.planning/phases/56-animation-and-transition-paint-integration/56-05-SUMMARY.md`
- `.planning/phases/56-animation-and-transition-paint-integration/56-VERIFICATION.md`
