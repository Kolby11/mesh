# Status

**Updated:** 2026-10-29

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- Continue the whole-codebase audit; sections 01–09 have review reports, including section 08's continuation and stated coverage limits. Sections 10–16 and cross-section review remain; resume from the coverage inventory.
- Extract the frontend component runtime from shell (SH-08).
- Implement the Material reference shell (08), including the desktop editor, widget picker, side rail and dock.

## Next

- Investigate intermittent fractional retained-layout parity. The redesigned Settings appearance page now reproduces it deterministically (`settings_scroll_shift_matches_full_paint`).
- Extend styleable control parts from `slider` to scrollbars, inputs, toggles and progress.
- Continue trusted live glass source work and the in-surface blur engine.

## Blocked / known baselines

- D1 compound-table write tracking remains deferred pending supported runtime
  observation; indirect list reads still compare every item.
- GPU panel recorded-scene parity exceeds its pixel budget on the baseline.
- NVIDIA as compositor main device and other compositor imports remain unchecked
  live.

- Default-parallel shell checks have intermittent fractional-layout and debug
  Surfaces assertions; isolated reruns and the complete two-thread run pass.
