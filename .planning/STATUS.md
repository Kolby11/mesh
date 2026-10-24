# Status

**Updated:** 2026-10-24

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- **Replay timing alignment complete.** CPU and live dock sweeps share absolute
  input deadlines and a warm-up/leave/settle sequence. Three GPU and three CPU
  release replays pass with zero visit outliers.
- **Full-layout projection in flight.** The write records fit extents and cached
  geometry in one traversal; correctness checks pass, controlled timings pending.

- **Backlog reliability batch complete.** Activation-test deadlines and process
  isolation for cache/keyed-hover/geometry/text regressions pass parallel shell
  and renderer suites. Replay boot-state and successful GPU commit accounting
  are covered; the GPU-feature submission/completion CI gate also passes.
  Paint-time effect requests now reach the shell and queued work wakes the loop.

- **Popup/dashboard frame isolation.** Tooltip and popover redraw isolation,
  timer patches during child lifecycle, transform-based meters/history/spectrum,
  and selective service fallback are implemented. Three release GPU captures
  confirm zero bar validation during popup/timer windows, zero spectrum layout,
  and six layout frames for six CPU updates. Evidence in
  `profiles/popup-live/summary.json` and the performance log. Live captures also
  found shared-tree preparation outrunning child callbacks; it is in the backlog.

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
- Measure glass first open live (picker over IPC, launcher by hand).
- Continue glass raster work and the in-surface blur engine after frame-pipeline work.

## Blocked / known baselines

- CPU glass dashboard open remains over budget; the dock gate remains noisy at
  its edge. Neither budget was changed by this work.
- NVIDIA as compositor main device and other compositor imports remain unchecked live.
