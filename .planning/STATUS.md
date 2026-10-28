# Status

**Updated:** 2026-10-28

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- Continue the whole-codebase audit; sections 01–07 and 09 are reviewed, while section 08 needs deeper logic/test coverage. Sections 10–16 and cross-section review remain; resume from the coverage inventory.
- Extract the frontend component runtime from shell (SH-08).

## Next

- Investigate intermittent fractional retained-layout parity.
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
