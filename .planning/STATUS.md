# Status

**Updated:** 2026-10-21

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- **GPU residency and workload gates implemented:** small image/icon/font-mask
  bitmaps share immutable pages; the worker retains bounded image identities
  and GPU cache headroom beside retained targets. Hardware gates check exported
  pixels, completed-frame budgets and zero warm bitmap uploads. AMD 780M and
  NVIDIA 4060 pass; renderer release is 324 passed / 17 ignored. The 256-instance
  asset workload improves; other timing ranges overlap. See the performance log.
  GPU CI requires a `mesh-gpu` runner and `MESH_GPU_RUNNER=enabled`.
- **Live glass performance pass:** backdrop copies are downsampled and stay
  resident across the theme's blur radii, custom properties are shared and
  fingerprinted once, and animated nodes no longer re-fingerprint their
  subtrees. Live dashboard frames are roughly a third of their former cost;
  see the performance log. Remaining glass items are in the backlog.
- **Recent shipped work:** A3 public style parts, parallel CPU raster,
  composition startup/live parity and the liquid-glass showcase. Details remain
  in October's log and the performance log.

## Next

- **Change-proportional frame pipeline (P0–P4)** is the next implementation
  priority: live benchmark and visited-node counters, typed node properties
  and state flags, one dirty set driving every pass, bindings as typed
  setters, then GPU-first pipelining.
  [Design](todos/pending/2026-10-21-change-proportional-pipeline.md); backlog
  lines at the top of `docs/BACKLOG.md`. Single-pass scoping buys 3–10% and is
  deprioritized.
- Glass D9 gates, closed-popover pruning and held-animation sleep landed
  2026-10-21 (performance log).
- Wave 5: [in-surface blur engine](todos/pending/2026-09-30-in-surface-blur-engine.md).
- Continue customization with A6; see the backlog for other open work.

## Blocked / known baselines

- Shell default-feature serial debug: `2fb51469` and corrected `f03ce07d` have
  the same 41 failure names (2026-10-20). Compare fresh failure sets, not counts.
- Known module/elements/frontend fixture failures and shell interaction failures
  remain recorded in October's log. GPU residency changes have passed the full
  renderer suite and shell compilation, not a fresh full shell test run.
- Live Hyprland presentation has been checked on AMD; NVIDIA as compositor main
  device and other compositor imports remain unchecked live.
