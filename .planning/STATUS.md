# Status

**Updated:** 2026-10-16

What is in flight right now. One page, overwritten as work moves. History and
measurements are in [`log/`](log/README.md). Open work is in
[`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

- **Runtime direction follow-ups:** D1 template patching/effects, D2 task
  scheduling with suspending backend callbacks, D3 grants and D4 typed
  contracts have landed (see [October's log](log/2026-10.md)). Remaining D2
  work (stream hooks, `stop`) waits on D8.
- **Scroll wave 4 item 15:** the [bounded scroll raster cache](todos/pending/2026-09-30-scroll-raster-cache.md).
  Production wiring is deferred by the revised D6 ownership and measurement contract.

## Next

- D8 event-driven backend host APIs, then D5 reusable interaction primitives.
- Wave 5: the [in-surface blur engine](todos/pending/2026-09-30-in-surface-blur-engine.md).

## Blocked / known baselines

- Known fixture failures (serial release, 2026-10-13): shell 27, plus three
  load-sensitive `shell::tests::scheduler` tests that fail in some full runs
  and pass alone; elements 10; `mesh-core-module` 9; `mesh-core-frontend` 2
  (`phase87_layout_display_source_semantics_survive_lowering`,
  `shipped_settings_surface_compiles_with_local_pages`, failing at
  `3b7068c2` too). Compare against these counts instead of treating them as
  regressions.
- `navigation_bar_catalog` has no `mesh.wm` provider, so navigation benches
  render the start slot as error placeholders.
