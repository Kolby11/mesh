# Status

**Updated:** 2026-10-17

What is in flight right now. One page, overwritten as work moves. History and
measurements are in [`log/`](log/README.md). Open work is in
[`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

- **Priority 4 landed (uncommitted):** CMP-07, CMP-02/08, A1, A7, A5 and
  RES-06/RES-12 are in the working tree with log entries; not yet committed.
  Inspector Ctrl+Tab and niri/Sway providers are unchecked live.

- **D7 in progress:** shared theme alias resolutions and bounded color/number
  token projections, indexed literal operations, and narrow compound selector
  buckets are validated. Remaining property lowering and incremental matching stay open.

- **D6 preparation:** immutable scene submissions and a bounded render-thread
  handoff and owned, budgeted font preparation are validated. Icon snapshots,
  production resource admission, GPU raster and presentation remain open;
  the current painter is still authoritative.
- **Runtime direction follow-ups:** D1 template patching/effects, D2 task
  scheduling with suspending backend callbacks, D3 grants, D4 typed
  contracts and D5 popover disclosure have landed (see
  [October's log](log/2026-10.md)). Remaining D2 work (stream hooks, `stop`)
  waits on D8. D5 disclosure still needs a check in a live compositor.
- **Scroll wave 4 item 15:** the [bounded scroll raster cache](todos/pending/2026-09-30-scroll-raster-cache.md).
  Production wiring is deferred by the revised D6 ownership and measurement contract.

## Next

- Priority 5 control-plane consolidation: MOD-05, then MOD-06.
- D8 event-driven backend host APIs, then the rest of D5 (dialog focus scope,
  menu typeahead).
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
