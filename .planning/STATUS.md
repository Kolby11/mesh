# Status

**Updated:** 2026-10-20

What is in flight right now. One page, overwritten as work moves. History and
measurements are in [`log/`](log/README.md). Open work is in
[`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

- **K parallel CPU raster implemented:** multi-surface CPU frames record paint
  in the VM phase and replay into each buffer on rayon before present; 3×
  1840×1800 full repaints 74.8–76.1 → 39.2–40.6 ms. Shell failure names match
  the baseline. Child popups and the SHM copy stay serial; frame pipelining is
  next in the backlog.

- **Composition startup/live parity implemented:** inherited roots, settings,
  resources and user deltas resolve before startup and candidate mounts; shared
  settings reloads preserve them. Glass/Desk profiles no longer repeat defaults.
  Regression coverage includes round-trip switches, revisions and rollback;
  Glass → Desk → Glass passed live on Wayland through CLI acknowledgements.

- **Liquid-glass showcase implemented:** responsive media/weather cards,
  calendar and control-center popups, dock magnification and app search, lighter
  shared materials and English/Slovak controls. Local popovers promote correctly.
  Three integration tests cover seven widths, service commands and input; nine
  disclosure tests pass. Live GPU rendering checked on Hyprland at 2880×1800.

- **D9 CPU workload gates implemented:** idle maintenance deadlines, navigation
  hover and service delivery, Settings scroll, popover first paint and theme
  switching have explicit CI budgets and a focused runner. Gate failure-path
  checks and all six budgets pass across three release samples. Measurements
  are in the performance log; GPU presentation budgets remain in the renderer
  backlog.

- **Inherited-variable diagnostic fix:** live style checks preserve finalized
  custom properties, avoiding the false `battery.foreground` warning. All six
  style application tests pass.

- **D5 interaction primitives implemented:** modal dialog/sheet focus scopes,
  focus entry/return and dismissal, menu typeahead and nested-menu navigation.
  Nine new shell regressions and 50 interaction tests pass. The broader shell
  interaction suite retains the same six failures as today's recorded baseline.

- **SH-11 → D6 integrated:** explicit frame paint inputs and complete immutable
  Skia scenes pin glyph/icon/image/backdrop resources. The worker owns EGL,
  retained pixels and fenced export slots; Wayland owns descriptor imports and
  release leases. Renderer/presentation suites and required-GPU live presentation
  pass. Shell failure names match the 41-name baseline exactly; release
  measurements are in the performance log (small partial frames regress).
  Presentation currently waits for completion; frame pipelining stays separate.

- **Glass theme coherence implemented:** glass surfaces, text, gradients and
  shadows derive from the shared material family or contract roles; Violet
  Glass retains its tuned values. Doctor and LSP warn about fixed component
  colours, respecting independent modules and trailing literal exemptions.
  All eight shipped theme modes and the component parser suite pass.

- **Theme/component compatibility implemented:** contract 1.1 with platform
  defaults, theme `@module` values over module defaults, `library` token
  families, internal-selector and undeclared-hook diagnostics,
  `mesh-shell theme coverage`, and a theme that follows the user across shells
  (compositions recommend, profiles pin, `set_theme { scope }`). Module tests
  match the baseline. Shell comparison against pre-change `2fb51469` is complete:
  one introduced registry-field expectation is fixed; the remaining 41 failure
  names match exactly, including six theme-named failures. See October's log.

- **Render sweep follow-through validated:** hover/transition frames retain
  pixels, paint-only hover skips layout, resource loads repaint only consumers
  (including child targets), and quiet surfaces borrow their IDs. Focus-grab
  and callback-escape deadlines join the parallel session's repeat deadline.
  Exact parity and GPU checks pass; release shell failure names match the
  baseline (45), renderer passes (306). Measurements are in the performance log.

- **Two-way value bindings implemented:** compiler-generated Luau change
  callbacks write input, slider and checkbox/switch edits back to script
  variables before explicit change handlers. Six binding regressions and
  thirteen scripting callback tests pass; compiler retains its two known failures.

- **CSS profile gaps implemented:** gradient shorthand, theme color syntax,
  multiple evenly spaced stops and shadow lists are supported. CPU render
  tests pass (306); elements retain the 10 documented fixture failures.
  GPU files are unchanged; see October's log.

- **Worker test isolation implemented:** image-decode and SVG worker fixtures
  run in child processes so parallel painters cannot consume their broker
  admission budget. Gradient-shader test counts follow their thread-local cache.

- **GPU paint and dmabuf presentation shipped:** surfaces paint into a
  retained Skia-GL target on the scene worker, which copies damage
  into fenced dmabuf textures on the compositor's device (AMD 780M under
  Hyprland here). On by default; `MESH_RENDER_GPU=0`, missing dmabuf feedback,
  or a failed export falls back to SHM. A full repaint of a frosted
  1840×1800 px surface is about 1.9 ms instead of 36 ms (performance log).
  Remaining: resource residency and GPU workload gates.

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

- **Runtime direction follow-ups:** D1 template patching/effects, D2 task
  scheduling with suspending backend callbacks, D3 grants, D4 typed
  contracts and D5 popover disclosure have landed (see
  [October's log](log/2026-10.md)). D8 completes D2 stream hooks and `stop`.
  D5 disclosure and modal/menu interactions still need a live compositor check.
- **Scroll wave 4 item 15:** the [bounded scroll raster cache](todos/pending/2026-09-30-scroll-raster-cache.md).
  Production wiring is deferred by the revised D6 ownership and measurement contract.

## Next

- GPU resource residency (icons, images, glyph bitmaps) measured on the GPU
  path; GPU frame-cost and parity gates.
- GPU workload gates; frame-N/frame-N+1 pipelining after the scene integration.
- Wave 5: the [in-surface blur engine](todos/pending/2026-09-30-in-surface-blur-engine.md).

## Blocked / known baselines

- `mesh-core-shell`, default-feature serial debug (2026-10-20): pre-compatibility
  `2fb51469` and corrected `f03ce07d` have the same 41 failure names, including
  six theme-named failures. The introduced `scope` registry expectation is fixed.
  Use fresh failure-name comparisons rather than historical counts below.
- Known fixture failures (serial release, 2026-10-13): shell 27, plus three
  load-sensitive `shell::tests::scheduler` tests that fail in some full runs
  and pass alone; elements 10; `mesh-core-module` 9; `mesh-core-frontend` 2
  (`phase87_layout_display_source_semantics_survive_lowering`,
  `shipped_settings_surface_compiles_with_local_pages`, failing at
  `3b7068c2` too). Compare against these counts instead of treating them as
  regressions.
- `navigation_bar_catalog` has no `mesh.wm` provider, so navigation benches
  render the start slot as error placeholders.
