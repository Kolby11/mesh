# Status

**Updated:** 2026-10-10

What is in flight right now. One page, overwritten as work moves. History and
measurements are in [`log/`](log/README.md). Open work is in
[`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

- **Runtime direction work, items 4–10 of the 2026-10-10 session list.**
  Block review Priority 3 closed on 2026-10-11 (see
  [October's log](log/2026-10.md)). In order: one hover-intent mechanism,
  `class:` / `style:` / `{#if}` across shipped modules, the D1 prototype on the
  navigation bar, D4 with S03-DEAD-001, D2, D3, and one service call path.
- **Scroll wave 4 item 15:** the [bounded scroll raster cache](todos/pending/2026-09-30-scroll-raster-cache.md).
  It is not wired into production yet; re-check the plan against D6 first.

## Next

- Hover intent as one mechanism, then `class:` / `style:` / `{#if}` across the
  shipped modules, then D1 on the navigation bar, D4 with S03-DEAD-001, D2,
  D3 and the single service call path.
  [Direction note](todos/pending/2026-10-10-runtime-architecture-direction.md).
- Wave 5: the [in-surface blur engine](todos/pending/2026-09-30-in-surface-blur-engine.md).

## Blocked / known baselines

- The shell and elements suites carry known fixture failures: 28 in shell
  (release, 2026-10-10, after the popup fix) and 10 in elements; `mesh-core-module` carries 9. The
  renderer suite passes. Compare against these counts instead of treating
  them as regressions.
- `navigation_bar_catalog` has no `mesh.wm` provider, so navigation benches
  render the start slot as error placeholders.
