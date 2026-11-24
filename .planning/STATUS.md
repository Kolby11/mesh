# Status

**Updated:** 2026-11-25

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and measurements live in [`log/`](log/README.md).

## Now

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

- Continue scoped backlog correctness work after subprocess startup ordering, masked-input selection mapping and completed backend task retirement and Advanced Settings optimistic reconciliation; fixes and focused regressions are complete.

- Address independent correctness defects from the recent audit in scoped waves, prioritizing input, rendering and backend lifecycle. Keep shared tracking changes centralized.
- Continue the incomplete whole-codebase audit and reconcile its coverage.
- Measure performance candidates against representative workloads before changing them; tackle larger architectural features through their linked designs and dependencies.

## Blocked / known baselines

- D1 compound-table write tracking remains deferred pending supported runtime observation.
- GPU panel recorded-scene parity exceeds its pixel budget on the baseline; NVIDIA and other compositor imports remain unchecked live.
- Default-parallel shell checks have intermittent fractional-layout and debug Surfaces assertions; isolated reruns and the complete two-thread run have passed.
- The Settings material-policy test fails its Liquid Glass panel gradient
  assertion on clean `e399e827` and the correctness wave.
- The Settings debug integration test fails its expected audio volume request
  on clean `e399e827` and the programmable rendering changes.
