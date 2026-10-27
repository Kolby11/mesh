# Status

**Updated:** 2026-10-27

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- Settings pop-out sizing is fixed; the user confirms touchpad scrolling now
  works well in the live desktop. The scrolling follow-up is closed.
- RT-04 implemented: synchronous `mesh.exec` uses Tokio worker handoff on
  multithreaded runtimes. Single-worker fairness and compatibility tests pass;
  full scripting suite: 380 passed, zero failures, seven ignored.
- Desktop widget visibility and independent live sizing are complete. The final
  GPU-feature shell suite passed: 1,043 tests, zero failures, 37 ignored.

## Next

- Continue glass raster work and the in-surface blur engine.

## Blocked / known baselines

- D1 compound-table write tracking remains deferred pending supported runtime
  observation; indirect list reads still compare every item.
- CPU glass dashboard open remains over budget; the CPU dock budget remains
  noisy at its edge.
- GPU panel recorded-scene parity exceeds its pixel budget on the baseline.
- NVIDIA as compositor main device and other compositor imports remain unchecked
  live.
