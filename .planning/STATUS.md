# Status

**Updated:** 2026-10-24

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- **Geometry-changing patches stay sparse in display and rounding.** Sparse
  display entries and the persistent command patch accept local geometry
  changes and update ancestor coverage in place; write-back rounds instead of
  Taffy's whole-tree pass. At 1,024 siblings: clean-column paint 817–1,186 →
  232–243 µs, display entries 1,027 → 0. Dirty paths share prefixes.
- **Keyed lists read from a member take item deltas from the state sync**: one
  item write compares one item (was 1,024); handler 2.3–2.5 → 1.0–1.1 ms.
- **Fixed on the way:** structural relayout kept stale text measurements.
- D9 CPU-raster wall times rose (dashboard median 64 → 73 ms) with identical
  instruction counts; Skia raster stages moved 32 bytes in alignment. Recorded
  as code-placement noise. Details: [performance log](log/performance-log.md).
- **Uncommitted:** this work sits on top of the earlier uncommitted P2 changes.
  Live inspection of both remains pending.

## Next

- P2 remainder: layout write-back and overflow annotation still visit every child
  of a re-laid-out parent; fold `seed_projection` into full writes.
- D1: handler sync still converts every compound global (~1 ms for 1,024 items).
- Correct live replay injection, namespace isolation and GPU first-commit timing.
- Continue glass raster work and the in-surface blur engine after frame-pipeline work.

## Blocked / known baselines

- CPU glass dashboard open remains over budget; the dock gate remains noisy at
  its edge. Neither budget was changed by this work.
- Parallel renderer/keyed-hover fixture isolation remains open; the suites above
  ran serially. NVIDIA as compositor main device and other compositor imports
  remain unchecked live.
