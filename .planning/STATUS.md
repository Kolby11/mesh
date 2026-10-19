# Status

**Updated:** 2026-10-19

What is in flight right now. One page, overwritten as work moves. History and
measurements are in [`log/`](log/README.md). Open work is in
[`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

- **CSS profile gaps implemented:** gradient shorthand, theme color syntax,
  multiple evenly spaced stops and shadow lists are supported. CPU render
  tests pass (306); elements retain the 10 documented fixture failures.
  GPU files are unchanged; see October's log.

- **Worker test isolation implemented:** image-decode and SVG worker fixtures
  run in child processes so parallel painters cannot consume their broker
  admission budget. Gradient-shader test counts follow their thread-local cache.

- **GPU paint and dmabuf presentation shipped:** surfaces paint into a
  retained Skia-GL target on the UI thread, and presentation copies damage
  into fenced dmabuf textures on the compositor's device (AMD 780M under
  Hyprland here). On by default; `MESH_RENDER_GPU=0`, missing dmabuf feedback,
  or a failed export falls back to SHM. A full repaint of a frosted
  1840×1800 px surface is about 1.9 ms instead of 36 ms (performance log).
  Remaining: resource residency, D9 gates, scene-thread move after SH-11.

- **Rounded raster correctness implemented:** rounded overflow, image/gradient
  edges and shadows have exact partial/full repaint regressions. Renderer,
  geometry and shell scroll/theme checks pass; measured color-raster caches
  replace the slower mask prototype (performance log, 2026-10-19).

- **Unread service updates schedule no frame:** an update no evaluated
  template reads settles effects at delivery; Appearance under unrelated
  audio traffic went from a full template pass per update to no frame. Patched
  frames still pay whole-tree passes (backlog, Performance).

- **D8 implemented:** native D-Bus, PipeWire, route netlink and wlr toplevel
  sources share the bounded stream lifecycle with inotify/socket sources.
  Stream callbacks and bounded shutdown yield through D2. NetworkManager,
  UPower and audio prefer native sources with monitor/poll fallbacks.
  Native grants are approved in `config/module.json`; a live shell spawns no
  monitor subprocesses. Measurements are in the performance log.

- **Signal shutdown and `mesh.dbus_call` implemented:** signals run the ordinary
  teardown (no orphaned monitors); scoped `dbus.call:` grants allow method calls.
  Next D-Bus step is A8: migrate UPower/NetworkManager reads off `exec.argv`.

- **Finalized memo entries and D7 matching implemented:** component memo
  entries retain pre-animation styles and semantics; class/state reverse
  dependencies include component and theme rules. Rebuilds defer compiler
  layout to the authoritative final cascade. Release failure sets match the
  baseline and structural gates pass; measurements are in the performance log.
  Remaining string declaration lowering stays in the backlog.

- **Performance follow-through (2026-10-18):** Priority 9 closed; in-surface
  blur phases 0–2 (correctness fix, per-element routing, 04 §10 contract);
  scroll frames keep retained pixels and whole-pixel scrolls shift in place;
  D7 keyword lowering. Blur caches (phases 3–5) stay gated on D6; the
  remaining scroll-raster cases are in the backlog.

- **Performance batch implemented:** shared reload maintenance,
  blocking file-watch control, validated backend state/fingerprints, and direct
  unrefracted wallpaper paint. Release gates pass; experiments and ranges are in
  the performance log. Included in the workspace checkpoint.

- **Priority 6 complete:** SH-07, UI-10/RT-08/CMP-05/CMP-12, MOD-11, RES-11
  and RT-07 are done (most landed in the checkpoint; confirmed and logged
  2026-10-18). Rounded fills and borders now paint clip-invariantly; the LSP
  manifest schema and the scene-thread test are fixed. SH-09/RES-13,
  RES-09/RES-10, UI-12, RND-05 and RT-11/CMP-11/CMP-13 are verified and
  closed (2026-10-19); the contract-free LSP completion fallback is removed.
  Rounded clips, gradients, images and shadows are now fixed too. The retained
  render tests share the resource-state lock, revision bumps run in child test
  processes, and LSP theme discovery uses its own profile fixture. Image/SVG
  worker fixtures and gradient-shader counters are isolated too.

- **Priority 5 implemented:** MOD-05/06/07/08/09/10/12,
  SH-10 and RES-08. Shared core package engines, configuration paths/locks,
  profile-aware CLI settings/locale, inherited removal deltas, safe composition
  detachment and bounded manifest retention are validated. Live compositor
  package activation has not been exercised.
- **Priority 4 committed:** CMP-07, CMP-02/08, A1, A7, A5 and RES-06/RES-12
  landed in `934c4648`. Inspector Ctrl+Tab and niri/Sway providers are unchecked live.

- **D6 preparation:** immutable scene submissions and a bounded render-thread
  handoff and owned, budgeted font preparation are validated. Icon snapshots,
  production resource admission and moving GPU paint to that thread remain
  open; GPU paint runs on the UI thread until SH-11.
- **Runtime direction follow-ups:** D1 template patching/effects, D2 task
  scheduling with suspending backend callbacks, D3 grants, D4 typed
  contracts and D5 popover disclosure have landed (see
  [October's log](log/2026-10.md)). D8 completes D2 stream hooks and `stop`.
  D5 disclosure still needs a check in a live compositor.
- **Scroll wave 4 item 15:** the [bounded scroll raster cache](todos/pending/2026-09-30-scroll-raster-cache.md).
  Production wiring is deferred by the revised D6 ownership and measurement contract.

## Next

- GPU resource residency (icons, images, glyph bitmaps) measured on the GPU
  path; GPU frame-cost and parity gates.
- Remaining D5 (dialog focus scope, menu typeahead), D6 renderer integration
  and D9 workload gates.
- Wave 5: the [in-surface blur engine](todos/pending/2026-09-30-in-surface-blur-engine.md).

## Blocked / known baselines

- `mesh-core-shell` serial release at `17224ae1` (2026-10-19): 45 failures;
  diff failure names against a HEAD worktree, not the counts below.
- Known fixture failures (serial release, 2026-10-13): shell 27, plus three
  load-sensitive `shell::tests::scheduler` tests that fail in some full runs
  and pass alone; elements 10; `mesh-core-module` 9; `mesh-core-frontend` 2
  (`phase87_layout_display_source_semantics_survive_lowering`,
  `shipped_settings_surface_compiles_with_local_pages`, failing at
  `3b7068c2` too). Compare against these counts instead of treating them as
  regressions.
- `navigation_bar_catalog` has no `mesh.wm` provider, so navigation benches
  render the start slot as error placeholders.
