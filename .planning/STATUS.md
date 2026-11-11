# Status

**Updated:** 2026-11-11

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and measurements live in [`log/`](log/README.md).

## Now

- A correctness wave is in flight: optional service-feature visibility, optional surface-policy clearing, stepped animation deadlines, and XDG resource precedence.

- The 2026-11-11 performance scan landed its backend I/O fixes (shared D-Bus connection, pidfd exec waits, compiled contract validation) and signal-driven Bluetooth/power-profile providers; its unfixed findings are under "Scan findings" in the backlog.

## Next

- Address independent correctness defects from the recent audit in scoped waves, prioritizing input, rendering and backend lifecycle. Keep shared tracking changes centralized.
- Continue the incomplete whole-codebase audit and reconcile its coverage.
- Measure performance candidates against representative workloads before changing them; tackle larger architectural features through their linked designs and dependencies.

## Blocked / known baselines

- On 2026-11-11, `main` also fails `production_icon` (two cold-icon tests) and three `mesh-core-module` shipped-module tests (Settings focus-ring CSS, `glass.rim.inset`, volume widget icons), independent of that scan.

- D1 compound-table write tracking remains deferred pending supported runtime observation.
- GPU panel recorded-scene parity exceeds its pixel budget on the baseline; NVIDIA and other compositor imports remain unchecked live.
- Default-parallel shell checks have intermittent fractional-layout and debug Surfaces assertions; isolated reruns and the complete two-thread run have passed.
