# Status

**Updated:** 2026-10-27

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- Live widget follow-ups are complete: host-published props invalidate cached
  template values; card/spectrum dimensions stay synchronized, seeking holds its
  preview through stale samples, and component keyframes run with real durations.
- Full shell suite (two threads): 1,050 passed, 37 ignored. Scripting: 383 passed,
  7 ignored. Release binary rebuilt; GPU resized-card visuals checked in an isolated
  preview, which has been stopped. Live music-control input remains unchecked.

## Next

- Continue trusted live glass source work and the in-surface blur engine.

## Blocked / known baselines

- D1 compound-table write tracking remains deferred pending supported runtime
  observation; indirect list reads still compare every item.
- CPU glass dashboard open remains over budget; the CPU dock budget remains
  noisy at its edge.
- GPU panel recorded-scene parity exceeds its pixel budget on the baseline.
- NVIDIA as compositor main device and other compositor imports remain unchecked
  live.

- Default-parallel shell checks have intermittent fractional-layout and debug
  Surfaces assertions; isolated reruns and the complete two-thread run pass.
