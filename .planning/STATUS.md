# Status

**Updated:** 2026-10-24

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- **Glass first open.** File icons and thumbnails no longer rasterize on the
  frame; GPU scenes draw shadows and rounded gradients natively; showing a
  hidden surface restyles its retained tree and paints once, after configure;
  idle prewarm paints never-shown hidden surfaces; the launcher builds only its
  active view (dock 1,877 → 676 nodes). Offline numbers in the
  [performance log](log/performance-log.md); a live measurement is still due.
- **Clean frames repaint from the retained tree.** A paint with nothing dirty
  took a full template build (1,005 validation / 184 layout visits on the glass
  bar); the recording timer's own tick was already a one-node patch. Not
  measured live.
- Earlier today: sparse text, geometry and keyed-list patches (committed in
  `a163218a`). D9 CPU-raster wall times move with code placement, not work.
- Live Settings scroll re-captured after the size-flip fix (2.8 / 4.8 ms
  p50/p90). Fixed the glass bar animating every frame after a popover was
  dismissed mid-exit; the CPU glass rim now shades only its edge band.

## Next

- P2 remainder: layout write-back and overflow annotation still visit every child
  of a re-laid-out parent; fold `seed_projection` into full writes.
- D1: handler sync still converts every compound global (~1 ms for 1,024 items).
- Correct live replay injection, namespace isolation and GPU first-commit timing.
- Measure glass first open live (picker over IPC, launcher by hand).
- Continue glass raster work and the in-surface blur engine after frame-pipeline work.

## Blocked / known baselines

- CPU glass dashboard open remains over budget; the dock gate remains noisy at
  its edge. Neither budget was changed by this work.
- Parallel renderer/keyed-hover fixture isolation remains open; the suites above
  ran serially. NVIDIA as compositor main device and other compositor imports
  remain unchecked live.
