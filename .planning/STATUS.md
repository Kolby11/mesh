# Status

**Updated:** 2026-10-27

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- Frutiger Aero light/dark modes and theme-controlled glass backdrop materials are
  complete. Real-surface tests: 96 passed; token/material contract tests: 12 passed.
  Both Aero Settings modes were rendered and visually checked.

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
