# Status

**Updated:** 2026-07-28

This page describes the present and is meant to be overwritten. History lives in
[`log/`](log/); open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

Performance checkpoints on the widget-tree build path, run as a series of
measured, individually gated changes rather than a milestone.

The current stream has two threads:

- **Interning and typed representation.** Attributes are interned and flat,
  module ids and element tags are shared, template bindings keep their JSON type
  instead of stringifying. Recorded in
  [`log/backlog-archive-2026-07-28.md`](log/backlog-archive-2026-07-28.md)
  under *P2 — typing & interning*.
- **Interaction identity.** Scroll, checked, input, slider, hover, and focus
  state now key off stable `NodeId` rather than structural strings. Completion
  record: [`log/interaction-identity-2026-07-28.md`](log/interaction-identity-2026-07-28.md).

Cumulative effect on the representative 456-node tree build: 0.504–0.511ms
before the interning and theme-defaults checkpoints, 0.390–0.397ms after
(≈1.28x). Style resolution fell from 45% to 39% of the build.

## Next

From the attack order at the end of
[`docs/BACKLOG.md`](../docs/BACKLOG.md):

1. The two **P0 correctness risks** found in the 2026-07-28 subsystem scan —
   shared-Lua `this` isolation, and atomic live frontend-catalog propagation.
   Both are cross-component correctness, not performance, and both can silently
   corrupt state in release builds.
2. Nonblocking Wayland configure, and watcher-fed keyboard settings — removes a
   500ms shell-thread stall and two `fs::metadata` calls per keypress.
3. Retained text-measure state, subscriber-proportional service delivery, and
   batched multi-rectangle raster.

## Blocked

Nothing blocked.

## Verification

**Release measurement works.** Two backlog items recorded on 2026-07-28 that
measurement was "pending because the local Rust linker wrapper is unavailable".
Re-checked the same day under `nix develop` (rustc/cargo 1.94.0): the release
test binary builds, links, and runs, and a gate executes end to end —

```
cargo test --release -p mesh-core-elements --lib \
  shared_theme_defaults_beat_hashed_deep_clone -- --ignored --nocapture
→ 400,000 node resolutions: 35.8ms hashed+deep-clone vs 19.96ms cached, 1.79x
```

That gate passes. Note it came in at **1.79x against the 1.84–1.98x range
recorded when the checkpoint landed** — inside gate tolerance, but if the next
run also lands low, the recorded range is the thing to re-derive, not the gate.

These gates are implemented but still have no recorded release measurement, and
should be run before their items are treated as closed:

| Gate | Item |
| --- | --- |
| `node_id_slider_values_speedup` | interaction identity |
| `node_id_hover_path_speedup` | interaction identity |
| `node_id_focus_state_speedup` | interaction identity |
| typed declaration application | typed style declarations |

Full-shell suite state is stale in the record: the last written figure is 347
passing / 7 known-failing from 2026-06-22, predating a large amount of work.
Re-establish that baseline before trusting it.

## Standing constraints

- Every performance change lands with a representative benchmark and, where the
  win is structural, a checked relative gate.
- Correctness suites pass before a performance claim counts.
- Rejected approaches get recorded with their measurements — see the
  rejected-experiments table in [`log/performance-log.md`](log/performance-log.md).
