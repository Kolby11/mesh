# Status

**Updated:** 2026-10-26

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- **Colour icons and remaining host APIs complete.** Non-symbolic named/file
  icons retain colours; symbolic files and explicit pack policies retain tint
  control. Bounded yielding HTTP replaces provider `curl`; the StatusNotifier
  tray module owns or joins watchers through generic D-Bus services and scoped
  dynamic-client grants. Release: scripting 377, modules 337, icons 36,
  capabilities 15, shell 1,037 and GPU-feature production icons 6 passed.
  The final release CLI builds; private-bus tray integration passes.

- **Glass backlog batch (2026-10-26).** Committed and pushed: `mesh-shell focus
  <id>` for keyboard-only chrome entry; one platform focus ring in shipped
  modules; Hyprland focus grab for click-open popovers (lost trigger clicks);
  hidden layers destroyed instead of null-buffer unmapped (sway `wrong configure
  serial`); `mesh.launch_app` plus contract-only service-call authorization (dock
  activation was always denied); no tooltip overlay reserve when popups exist
  (bar layer 1272×248 → 1272×48); scooped borders and overflow clips; the
  launcher late keystroke (popup display lists trusted a dirty set from a
  later retained generation, and GPU scenes ignored carried damage). Each was checked live in a nested Hyprland
  or headless sway; see the October and performance logs.

- **Hiding surfaces unmap when their exit settles (`f9a5dac4`).** The root's
  opacity duration is now only the bound; glass dashboard and picker exits
  settle in 115–117 ms instead of 280 ms. Release GPU-feature shell: 1,036
  passed. The launcher late-keystroke bug did not reproduce in a nested
  Hyprland; it stays in the backlog pending a GPU repro.

- **Cold icon lookup fix complete.** Painting
  uses nonblocking prepared results; cold searches run on shared binding snapshots
  without holding the live registry lock. Live first opens in three paired fresh
  processes improve from 460–514 to 117–141 ms CPU, and 482–508 to 58–70 ms GPU.
  Pending icons repaint on completion rather than showing false missing markers.

- **Launcher first-open attribution complete.** Cold icon commands take
  250–269 ms, about 95–98% of first child preparation in three fresh CPU and
  three fresh GPU processes. CPU stacks identify synchronous XDG theme lookup
  and file metadata probes before raster scheduling. Canvas/backdrop setup is
  small. Release GPU-feature shell: 1,034 passed. Evidence and profiling-disabled
  controls are in the performance log.

- **P2 retained display spans complete.** Persistent span branches replace
  full-array copies and scrollbar repair scans. Ancestor rebuilds share offset
  child spans; frame plans publish the repaired immutable metadata. Repeated
  moves, all scope variants and old snapshots pass command/frame-plan/pixel
  parity. Release renderer: 343 passed; release GPU-feature shell: 1,034 passed.
  Sparse timings and conservative-control tradeoffs are in the performance log.

- **P2 shrinking display coverage complete.** Persistent child bounds replace
  sibling union scans, preserving retained snapshots. Command/coverage and
  full/partial pixel parity pass at 16/1,024 siblings; ancestor visits stay
  constant and index repair is logarithmic. Release renderer: 342 passed;
  release GPU-feature shell: 1,034 passed. Timings are in the performance log.

- **P2 display-scope command patches complete (`4005afb4`).** Stable geometry
  and text patches retain layer, scrollbar, clip, scale and scroll ancestors. Layer clips and
  filter regions repair from retained bounds; retained scroll offsets replace
  owner commands without visiting content. First/middle/last, growth/shrink,
  full-command/entry/coverage and partial-pixel parity pass at 16/1,024 siblings.
  Release renderer: 341 passed; release GPU-feature shell: 1,034 passed.

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

- **Live glass first-open measurement complete.** Three fresh release processes
  on the main Hyprland compositor: picker first buffer commit 43–45 ms, launcher
  305–490 ms; later opens 10–18 / 42–66 ms. First launcher child preparation
  dominates; attribution is the follow-up in the backlog and performance log.

- **Popup animation readiness complete.** Animation-only shared-tree work waits
  for an affected popup's presentation slot; parent/input/resource changes keep
  the conservative path. Release GPU-feature shell: 1,024 passed; the blocked
  popup preparation gate passes three runs.
- Compound-table write tracking is deferred for this branch pending a supported
  runtime observation mechanism; existing-key writes bypass `__newindex`.

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

- D1 remains deferred pending supported runtime table-write tracking; indirect
  list reads still compare every item.
- Continue glass raster work and the in-surface blur engine after pipeline work.

## Blocked / known baselines

- CPU glass dashboard open remains over budget; the CPU dock budget remains
  noisy at its edge. This batch changes neither budget.
- GPU panel recorded-scene parity fails identically on the baseline and current
  tree; the defect is in the backlog and its tolerance remains unchanged.
- NVIDIA as compositor main device and other compositor imports remain unchecked
  live.
