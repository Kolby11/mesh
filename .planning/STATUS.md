# Status

**Updated:** 2026-11-06

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- Reduce theme and font change latency. Text-size dragging commits on release, icon revisions retain text layouts, and theme selection/mode/token composition runs on a worker; retained theme-tree invalidation, other preparation paths, and end-to-end measurements remain open.

- Validate independent text size and shell scale in a running shell, including profile switching, popup geometry and accessibility.

- Implement the requested SH-08, Material reference shell/composition, control parts, theme fixtures, slider glide, glass Settings readability, and style-index/gap fixes. The retained-layout key-adoption regression passes with the element crate tests.

- Continue the whole-codebase audit; sections 01–09 have review reports, including section 08's continuation and stated coverage limits. Sections 10–16 and cross-section review remain; resume from the coverage inventory.
- Extract the frontend component runtime from shell (SH-08).
- Implement the Material reference shell (08), including the desktop editor, widget picker, side rail and dock.

## Next

- Investigate intermittent fractional retained-layout parity; the Settings scroll-shift failure turned out to be fractional-phase paint coverage, fixed separately.
- Design style-only control parts for scrollbars and inputs; slider, progress, meter, checkbox, radio and switch have parts.
- Fix the Settings and style-fixture tests that still expect glass panels under the solid themes.
- Continue trusted live glass source work and the in-surface blur engine.

## Blocked / known baselines

- D1 compound-table write tracking remains deferred pending supported runtime
  observation; indirect list reads still compare every item.
- GPU panel recorded-scene parity exceeds its pixel budget on the baseline.
- The scrollbar part palette regression passes when it supplies damage to the
  replay selector. The retained-display geometry workload shares the resource
  test lock with revision-mutating tests; three two-thread renderer runs pass.
- NVIDIA as compositor main device and other compositor imports remain unchecked
  live.

- Default-parallel shell checks have intermittent fractional-layout and debug
  Surfaces assertions; isolated reruns and the complete two-thread run pass.
