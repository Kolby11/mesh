# Status

**Updated:** 2026-10-24

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- **Scene-buffer retry complete.** Transient export failures wait for dmabuf
  release and resume without another raster. Three required-GPU release runs
  pass; presentation and shell suites pass. The P2 sibling scan has a linked
  producer-delta design and remains open.

- **Change-proportional frame pipeline.** Full layout now records geometry,
  fit extents and projection in one traversal. Its structural gate passes;
  elements: 314 passed, GPU-feature shell: 1,017 passed.
- **Replay gate aligned.** Shared absolute input deadlines and warm-up/settle
  sequences pass three live GPU and three forced-CPU runs with zero outliers.
- Paint-time effect delivery, asynchronous scene-completion coverage and
  parallel frame-test isolation are committed and validated. Recent outcomes
  and measurement limits are in the October and performance logs.

## Next

- P2 remainder: layout write-back and overflow annotation still visit every
  child of a re-laid-out parent; display patches rebuild under clip/scale/scroll
  scopes.
- D1: record compound table writes instead of converting every compound global
  during handler sync; indirect list reads still compare every item.
- Gate shared-tree popup animation preparation by targets that can present.
- Measure glass first open live (picker over IPC, launcher by hand).
- Continue glass raster work and the in-surface blur engine after pipeline work.

## Blocked / known baselines

- CPU glass dashboard open remains over budget; the CPU dock budget remains
  noisy at its edge. This batch changes neither budget.
- GPU panel recorded-scene parity fails identically on the baseline and current
  tree; the defect is in the backlog and its tolerance remains unchanged.
- NVIDIA as compositor main device and other compositor imports remain unchecked
  live.
