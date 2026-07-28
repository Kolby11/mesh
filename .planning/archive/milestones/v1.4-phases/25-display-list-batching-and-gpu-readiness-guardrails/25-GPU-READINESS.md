# Phase 25 GPU Readiness Proof

## Scope

Phase 25 does not implement a GPU backend. It leaves the retained renderer in a state where a later backend can consume explicit data boundaries instead of inferring correctness from the software painter.

## Ready Boundaries

- Retained widget invalidation reports typed dirty work.
- Retained render objects separate geometry, material, text, clip, opacity, transform, and accessibility-facing slots.
- Retained display-list entries have stable keys and retained reuse/damage metrics.
- Display-list batching reports compatible primitive groups without changing paint output.
- Barriers are explicit for text, icon, opacity, clip, translucency, and material changes.
- Partial-present support remains reported separately from computed damage opportunities.

## Handoff Criteria For A Future GPU Backend

- Consume retained display-list entries and batch summaries as inputs; do not bypass the retained render-object synchronization boundary.
- Treat every reported barrier as an ordering or state boundary until a backend-specific proof allows narrowing it.
- Preserve the software painter as the reference output while GPU parity tests are introduced.
- Keep backend capability flags honest: damage opportunity does not imply partial present support.

## Handoff Criteria For Parallel Paint/Layout

- Use retained ownership boundaries before moving work off-thread.
- Pass immutable render/display-list snapshots to workers.
- Keep style/layout mutation on the owner thread until data structures are explicitly partitioned.
- Preserve full-rebuild/full-repaint fallbacks for unsupported mutation patterns.
