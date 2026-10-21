# Status

**Updated:** 2026-10-21

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- **Pipeline execution, wave 1:** W1 landed. Patched-frame restyle was already
  path-directed, and paint-only patches now keep geometry. Dashboard frames are
  bound by layout-class patches, so W2 (layout propagation) is the dashboard
  lever. W6 landed: presentation no longer waits for the GPU worker (1–2 ms of
  UI time back per GPU frame). W4 landed: `tools/check-performance --live`
  shows the live dock repainting continuously while idle and hover frames walking the
  whole tree. That is the next lever. Wave 2 (W2, W5) is not started. Plan and waves are in the
  [pipeline note](todos/pending/2026-10-21-change-proportional-pipeline.md).

- **Dock flares and dock tooltips fixed:** new `corner-shape: scoop` gives the
  flares real concave transparency; tooltips that leave their surface are
  `xdg_popup`s. Settings theme-switch repaint still unreproduced.

- **Dashboard pass audit and P3 keyed native lists landed:** spectrum frames no
  longer rebuild Player (tree build 1.0 ms → 0); validation 1,657 → 151 and
  extents 763 → 151 visits per frame; p50 2.92–3.00 → 2.11–2.17 ms. Remaining:
  typed setters, restyle scan on patched frames (~336 visits), layout
  sync/discovery. See the performance log.

- **P2 interaction-frame scope landed:** steady dock hover annotates 8–14
  nodes per paint (was 1,879) and restyles 12 (was 1,878), with no retained
  discovery. Hover-change and animation-only frames are ~2.5× faster. No
  selector-dependents index was needed (no combinators). Also fixed targeted
  restyles dropping a surface root's inherited text values. New structural
  gate in `tools/check-performance`. See the P2 step 1 performance-log entry.
- **P0 visit counters landed** (live replay gate still open).

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

- **Change-proportional frame pipeline (P0–P4)** is the implementation
  priority. P1 (typed node state, typed runtime fields, `Prop` vocabulary with
  change classes) landed 2026-10-21: ~7% dashboard, ~18% steady dock hover.
  Next: P2 layout propagation and the patched-frame restyle scan, display-list
  ancestors, semantics and child-surface scopes; P0 live replay gate.
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
