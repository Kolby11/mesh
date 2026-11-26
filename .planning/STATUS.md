# Status

**Updated:** 2026-11-27

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and measurements live in [`log/`](log/README.md).

## Now

- Parent and promoted surfaces now admit scene recordings based on their own
  pending resources. The dock reused recordings during Settings wallpaper
  preview loads. [Measurements](log/performance-log.md#2026-11-27--scope-scene-cache-admission-to-each-target).

- Popup input uses retained geometry, keyed child instances retire after
  removal, and backend ingress bounds ordered messages while coalescing state.
  The release pointer, lifecycle and burst checks passed.
  [Measurements](log/performance-log.md#2026-11-26--promoted-popup-input-keyed-lifetime-and-backend-ingress).

- Expanded Appearance lists retain their scrolling content and shift pixels
  on uniform backdrops. The release workload shifted 59 of 60 finger frames.
  [Measurements](log/performance-log.md#2026-11-26--restore-retained-appearance-scrolling).

- Plain wallpaper reuses decoded pixels on eligible outputs. The backdrop
  tests and raster gate passed. [Memory measurements](log/performance-log.md#2026-11-26--reuse-decoded-wallpaper-pixels).

- The glibc shell returns free heap pages during idle turns after startup
  and surface visibility changes. [Memory measurements](log/performance-log.md#2026-11-26--reclaim-idle-heap-pages).

- The five Settings & Shell designs have selectable desktop profiles with
  matching themes, artboard-specific panels and shelves, and a shared volume
  and brightness overlay. The backlog tracks the remaining lock, window
  switcher, desktop-card and overlay designs.

- Template bindings share a locals snapshot per loop item. The keyed-list and
  Glass release suites passed. [Memory measurements](log/performance-log.md#2026-11-26--share-template-loop-locals).

- Luau and compiled-template translation hits skip diagnostic record
  construction. The Settings label workload passed both relative performance
  checks. [Measurements](log/performance-log.md#2026-11-26--text-only-translation-hits).

- Settings follows the exported sidebar order and page-specific card layouts.
  Appearance opens with theme previews, compact controls and wallpaper tiles
  when the provider has previews. Paired cards wrap at narrow sizes.

- Direct `font-family` values resolve roles declared only in the surface
  module's font overrides. The renderer regression checks shaping and pixels
  across author and user override changes.

- Numeric icon fill, weight, grade and optical size support CSS transitions.
  Axis endpoints that use the font's default apply immediately.

- Popover semantics derive trigger expansion from hidden and promoted popovers.
  Recording attempts reserve distinct output directories before starting the
  recorder.

- Named render sources support independent offscreen layout and explicit
  imports from children mounted on the same surface. The integration test
  covers pixels, prop and layout updates, and export removal.

- Authored components can use SVG paths and child masks, keyed interactive
  drawings, programmable materials, named same-instance render sources, and
  live provider textures. The compiler, renderer, shell, and new frontend
  integration tests passed.

- Completed the correctness wave for Reduce transparency, viewport-fixed
  rendering, backend supervision, and authoring queries. The shell, renderer,
  component, language-server, config, module and theme suites passed. The
  Settings glass-strength regression passed in the shipped component.

- Scoped theme token identity, icon axis style projection and bounded painter diagnostics passed the theme, element, module and renderer library suites and production icon integration tests.

- The November 21 correctness sweep is verified across the workspace suite and the final headless GPU-feature shell rerun. Live Hyprland verification covered the input wakeup and all five active compositor matrix cases. [Sweep report](codebase/audits/2026-11-21-correctness-sweep/README.md).

- The 2026-11-11 performance scan landed its backend I/O fixes (shared D-Bus connection, pidfd exec waits, compiled contract validation) and signal-driven Bluetooth/power-profile providers; its unfixed findings are under "Scan findings" in the backlog.

- 2026-11-14 audit correctness sweep landed (see the November log): module graph trust/closure, extension points, profiles and forks, store races, bounded reads, style inheritance and reference bounds, stylesheet images, focus traversal and scroll-into-view, template parsing, service contracts and mocks, socket connects, ticket-owned call results, LSP semantic tokens.

## Next

- Implement consent-managed window capture through the portal and PipeWire.
  A synthetic EGL DMA-BUF import with an acquire fence passed locally.
  [Design](todos/pending/2026-11-25-native-window-capture.md).

- Address independent correctness defects from the recent audit in scoped waves, prioritizing input, rendering and backend lifecycle. Keep shared tracking changes centralized.
- Continue the incomplete whole-codebase audit and reconcile its coverage.
- Measure performance candidates against representative workloads before changing them; tackle larger architectural features through their linked designs and dependencies.

## Blocked / known baselines

- D1 compound-table write tracking remains deferred pending supported runtime observation.
- GPU panel recorded-scene parity exceeds its pixel budget on the baseline; NVIDIA and other compositor imports remain unchecked live.
- Default-parallel shell checks have intermittent fractional-layout and debug Surfaces assertions; isolated reruns and the complete two-thread run have passed.
- The Settings debug integration test fails its expected audio volume request
  on clean `e399e827` and the programmable rendering changes.
