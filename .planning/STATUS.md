# Status

**Updated:** 2026-07-28

This page describes the present and is meant to be overwritten. History lives in
[`log/`](log/); open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

Performance checkpoints across the retained UI pipeline, run as a series of
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

Latest completed checkpoint: disjoint damage rectangles now share one selected
display-command traversal, raster canvas session, and profiling reset. The
representative four-region workload improved 3.31–3.48x and sixteen regions
improved 5.58–5.79x; one region remains on the original path. The full record
and checked gates are in
[`log/performance-log.md`](log/performance-log.md).

## Next

From the attack order at the end of
[`docs/BACKLOG.md`](../docs/BACKLOG.md):

1. The two **P0 correctness risks** found in the 2026-07-28 subsystem scan —
   shared-Lua `this` isolation, and atomic live frontend-catalog propagation.
   Both are cross-component correctness, not performance, and both can silently
   corrupt state in release builds.
2. Nonblocking Wayland configure, and watcher-fed keyboard settings — removes a
   500ms shell-thread stall and two `fs::metadata` calls per keypress.
3. Retained text-measure state and subscriber-proportional service delivery.

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

The aggregate run also supplied the previously missing release measurements for
`node_id_slider_values_speedup` (1.720x), `node_id_hover_path_speedup` (1.675x),
and `node_id_focus_state_speedup` (2.978x). The remaining implemented work with
no recorded release measurement is:

| Gate | Item |
| --- | --- |
| typed declaration application | typed style declarations |

**Full shell suite re-established.** On 2026-07-28,
`cargo test -p mesh-core-shell --lib` under `nix develop` reported 556 passing,
18 failing, and 123 ignored. The child-display-list checkpoint's complete
13-test `child_surface` slice passes; none of the 18 broader failures exercises
the 64-entry cap or eviction path. Treat 556/18 as the current shell baseline
until those failures are triaged.

**Performance gate suite has unrelated failures.** The multi-damage benchmark
passes its direct checked thresholds: at least 2.805x for four regions and
4.7175x for sixteen. The aggregate script aborted earlier in command order when
`stable_child_id_reuse_beats_rewriting_slots` measured 1.051x against its
1.25x self-assertion. The previously recorded
`shared_theme_revision_cache_speedup` failure (3.021x against a 4.40x
threshold) also remains unresolved. Neither baseline was weakened.

The retained-update `SmallVec` candidate was also measured and reverted: it
removed one allocation but improved the complete scoped path by only 1.005x.
That result is now in the rejected-experiments table.

## Standing constraints

- Every performance change lands with a representative benchmark and, where the
  win is structural, a checked relative gate.
- Correctness suites pass before a performance claim counts.
- Rejected approaches get recorded with their measurements — see the
  rejected-experiments table in [`log/performance-log.md`](log/performance-log.md).
