# Status

**Updated:** 2026-10-23

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- **Optional-feature test baseline cleared:** all features pass across the
  workspace excluding the CLI (shell 978 passed / 33 ignored; renderer 355 / 19).
  CLI allocation profiling and Tracy pass separately (18 tests each); their
  global allocators are intentionally incompatible. Fixed stale optional-renderer
  fixtures and a CPU-only profiler fixture choosing GPU presentation. The testing
  guide records the supported matrix; see October's log.

- **P2 keyed-list hover and CPU glass fixture implemented:** patched binding IDs
  join restyle and diagnostic targets across animation-generation gaps.
  The 16/1,024-item gate passes with identical restyle work, full
  style/pixel parity and complete retained updates. The new 2880×114 CPU fixture
  supplies wallpaper/output context and deferred replay, with direct pixel parity
  and a backdrop-effect check. CPU replay is 31.4–35.4 ms with the synthetic
  output backdrop versus 2.2–2.6 ms with zero-output context. Release shell suite:
  976 passed / 32 ignored. Measurements are in the performance log; the recorded
  inspector baseline failure was cleared by the test fixes described below.

- **Default-feature test baseline cleared:** the full workspace suite passes.
  Shell: 977 passed / 32 existing ignored; module: 330 passed; frontend: 92 passed;
  scripting: 357 passed. Compiler builtin validation and Advanced settings prop
  calls are fixed; fixtures follow current composition and lifecycle contracts.
  Signal and fast-file-read test races are fixed. Disabled inspectors now follow
  graph activation and capability rules; graph-reading tests share the existing
  environment lock to prevent temporary-path and theme races. See October's log.

- **W6 implemented:** bounded node recordings reuse retained paint payloads;
  complete scenes retain live effects, scroll scopes and tooltips. Resource and
  atlas publication invalidate stale recordings. One changed node records one
  picture beside 16 or 1,024 siblings; full/direct pixel parity and both GPUs'
  workload gates pass. See the performance log for timings and limits.

- **W3 implemented:** display ancestors resolve through retained paths; animation
  snapshots/sampling use restyled and registered nodes; retained semantic/layout
  updates reuse structural validation. Child lifecycle classes restore without
  rebuilding, and child requests refresh from indexed dependencies, pruning
  closed descendants. The 16/1,024-node gates and pixel/style parity pass.
  Timings include concurrent W7 work; see the performance log.

- **W7 implemented:** stable frames synchronize producer-reported dirty nodes
  and layout changes through snapshot paths; discovery is test-only. The full
  debug verifier finds no omissions. Dashboard discovery is 151 → 0 visits per
  frame across nine samples; timing ranges overlap. See the performance log.

- **Launcher sizing fixed:** promoted popups preserve intrinsic height, and
  clipped scroll content no longer inflates fit-sized ancestors. The 100-app
  launcher regression keeps its 320px viewport and fixed popup size while scrolling.

- **Live hover attributed:** forced-CPU dock frames spend 19.3–20.6 ms in
  raster replay out of 25.0–26.9 ms total. Presentation is 0.11–0.14 ms.
  Explicit GPU builds use the compositor's AMD device and spend 6.0–6.7 ms
  on UI-thread preparation/recording/submission; worker/display latency is
  excluded. The replay tool now accepts `MESH_LIVE_FEATURES=gpu` and prints
  the render-path reason. CPU live gates pass all three runs; GPU runs exceed
  the CPU fixture's animation envelope (15, 7, 16 frames). Fixture parity and
  actual GPU first-commit measurement remain in the backlog.
- **Surface timing attribution confirmed live:** dashboard CPU work is
  86.1–87.4 ms while unrelated surfaces report their own costs, rather than
  every surface inheriting the shared frame duration. See the performance log.

- **Pipeline execution, wave 1:** W1 landed. Patched-frame restyle was already
  path-directed, and paint-only patches now keep geometry. Dashboard frames are
  bound by layout-class patches, so W2 (layout propagation) is the dashboard
  lever. W6 landed: presentation no longer waits for the GPU worker (1–2 ms of
  UI time back per GPU frame). W4 landed, and its `--live` gate now
  passes. The idle dock repaint was a missing-icon resolution loop. Live hover
  rebuilds came from handler writes during input dispatch and from keyed loops
  with handlers being unpatchable; both are fixed (live hover mean 40 → 25 ms).
  W2 landed 2026-10-22: dashboard layout-class patches lay out only the
  patched branch (layout discovery and validation 151 → 0 per frame). W5 is
  implemented: typed binding targets and retained target paths; plain item-field
  keys reuse validation. The 16/1,024-sibling gate has one setter and no target
  discovery or runtime key validation. W3 is implemented. Plan and waves are in the
  [pipeline note](todos/pending/2026-10-21-change-proportional-pipeline.md).

- **Dock flares and dock tooltips fixed:** new `corner-shape: scoop` gives the
  flares real concave transparency; tooltips that leave their surface are
  `xdg_popup`s. Settings theme-switch repaint still unreproduced.

- **Dashboard pass audit and P3 keyed native lists landed:** spectrum frames no
  longer rebuild Player (tree build 1.0 ms → 0); validation 1,657 → 151 and
  extents 763 → 151 visits per frame; p50 2.92–3.00 → 2.11–2.17 ms. Remaining:
  snapshot item comparisons, shared restyle/layout paths, and downstream dirty-set
  consumers. See the performance log.

- **P2 interaction-frame scope landed:** steady dock hover annotates 8–14
  nodes per paint (was 1,879) and restyles 12 (was 1,878), with no retained
  discovery. Hover-change and animation-only frames are ~2.5× faster. No
  selector-dependents index was needed (no combinators). Also fixed targeted
  restyles dropping a surface root's inherited text values. New structural
  gate in `tools/check-performance`. See the P2 step 1 performance-log entry.
- **P0 visit counters landed** (CPU live gate passes; GPU envelope remains open).

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
  W3 scoping, W6 scene preparation and W7 discovery retirement are implemented.
  Keyed-list hover-handler restyles and the CPU glass timing fixture are implemented;
  live replay injection/timing alignment, layout annotation and shared paths remain open.
  [Design](todos/pending/2026-10-21-change-proportional-pipeline.md); backlog
  lines at the top of `docs/BACKLOG.md`. Single-pass scoping buys 3–10% and is
  deprioritized.
- Glass D9 gates, closed-popover pruning and held-animation sleep landed
  2026-10-21 (performance log).
- Wave 5: [in-surface blur engine](todos/pending/2026-09-30-in-surface-blur-engine.md).
- Continue customization with A6; see the backlog for other open work.

## Blocked / known baselines

- Live Hyprland presentation has been checked on AMD; NVIDIA as compositor main
  device and other compositor imports remain unchecked live.
