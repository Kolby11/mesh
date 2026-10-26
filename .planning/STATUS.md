# Status

**Updated:** 2026-10-26

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- **P2 geometry producer deltas and overflow indexes complete.** Sparse
  1,024-sibling width/text patches project four nodes and annotate ten, with
  Taffy's own traversal counted separately. Large change sets keep a linear
  full traversal. Release GPU-feature shell: 1,032 passed; elements: 320;
  interaction: 52. Layout, command, scroll-clamp and pixel parity pass.

- **Glass bug batch committed.** Tabs are one Tab stop with arrow roving
  (core); the dashboard scrim covers the output (overlay reserve dropped on
  spanning axes); bar ring, launcher outlines and volume-chip arrows fixed in
  modules. The launcher click loss on Hyprland remains open in the backlog.

- **Bubble-options authoring cleanup complete.** A pure item projection carries
  rotation and arrival classes into the template. Ten shipped popup transition
  and selection tests pass; the backlog item is removed.

- **Initial container-query sizing complete for unambiguous outputs.** First
  queries use logical output geometry without changing intrinsic measurement;
  configured content sizes take precedence. GPU-feature shell: 1,018 passed;
  presentation: 116 passed. Differing outputs before placement remain open.

- **Scene-buffer retry complete.** Transient export failures wait for dmabuf
  release and resume without another raster. Three required-GPU release runs
  pass; presentation and shell suites pass.

- **Change-proportional frame pipeline.** Full layout now records geometry,
  fit extents and projection in one traversal. Its structural gate passes;
  elements: 314 passed, GPU-feature shell: 1,017 passed.
- **Replay gate aligned.** Shared absolute input deadlines and warm-up/settle
  sequences pass three live GPU and three forced-CPU runs with zero outliers.
- Paint-time effect delivery, asynchronous scene-completion coverage and
  parallel frame-test isolation are committed and validated. Recent outcomes
  and measurement limits are in the October and performance logs.

## Next

- P2 display remainder: patches rebuild under clip/scale/scroll/layer scopes,
  and shrinking ancestor coverage still re-unions children.
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
