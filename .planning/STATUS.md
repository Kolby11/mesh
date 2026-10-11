# Status

**Updated:** 2026-10-11

What is in flight right now. One page, overwritten as work moves. History and
measurements are in [`log/`](log/README.md). Open work is in
[`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

- **Runtime direction, items 7–10 of the 2026-10-10 list:** D4 typed contracts,
  D2 `ticket:await()`, D3 acquisition-time grants and the single service call
  path have landed (see [October's log](log/2026-10.md)). Their follow-ups are
  backlog lines under "Runtime architecture direction" and "Authoring model".
- **Render suite flake:** `surface::icon` worker tests share the global
  resource revision with `display_list` tests; both now take
  `resource_state_test_lock()`.
- **Scroll wave 4 item 15:** the [bounded scroll raster cache](todos/pending/2026-09-30-scroll-raster-cache.md).
  It is not wired into production yet; re-check the plan against D6 first.

## Next

- D1: make template patching the default, then extend it to service-field
  reads and keyed lists.
  [Direction note](todos/pending/2026-10-10-runtime-architecture-direction.md).
- Backend task scheduler so `mesh.exec` and file reads yield (D2 with D8).
- Wave 5: the [in-surface blur engine](todos/pending/2026-09-30-in-surface-blur-engine.md).

## Blocked / known baselines

- Known fixture failures: shell 28 (release, `a42ed2bd`), plus three
  load-sensitive `shell::tests::scheduler` tests that fail in some full runs
  and pass alone; elements 10; `mesh-core-module` 9 (same names before and
  after 2026-10-11). Compare against these counts instead of treating them as
  regressions.
- `navigation_bar_catalog` has no `mesh.wm` provider, so navigation benches
  render the start slot as error placeholders.
