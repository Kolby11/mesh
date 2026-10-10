# Status

**Updated:** 2026-10-10

What is in flight right now. One page, overwritten as work moves. History and
measurements are in [`log/`](log/README.md). Open work is in
[`docs/BACKLOG.md`](../docs/BACKLOG.md).

## Now

- **Block review Priority 3 (wrong output).** UI-01–08, CMP-01, CMP-03, RES-01,
  RES-02 and RES-07's locale default have landed. Next are RND-09, RND-06/07,
  RES-03/04/14 and the RES-07 env option. See [October's log](log/2026-10.md).
- **Scroll wave 4 item 15:** the [bounded scroll raster cache](todos/pending/2026-09-30-scroll-raster-cache.md).
  - Byte shifting and exposed-strip repair pass their pixel tests.
  - Cache allocation, invalidation, composition and profiling are not yet
    wired into production.
  - Re-check this plan against D6 before investing further.

## Next

- Wave 5: the [in-surface blur engine](todos/pending/2026-09-30-in-surface-blur-engine.md).
- **Runtime direction, in the order from the 2026-10-10 plan.** The scheduler,
  typed `mesh.shell` calls and `class:` / `style:` directives have landed,
  uncommitted (see [October's log](log/2026-10.md)). Next: the D1 prototype on
  the navigation bar, compared with `navigation_frame_cost_profile` and
  `navigation_hover_profile`. Then D4 alongside S03-DEAD-001.
  [Direction note](todos/pending/2026-10-10-runtime-architecture-direction.md).
- Audit implementation: S02-LOGIC-001 / S02-LOGIC-002.

## Blocked / known baselines

- The shell and elements suites carry known fixture failures: 30 in shell
  (release, 2026-10-10) and 10 in elements; `mesh-core-module` carries 9. The
  renderer suite passes. Compare against these counts instead of treating
  them as regressions.
- `navigation_bar_catalog` has no `mesh.wm` provider, so navigation benches
  render the start slot as error placeholders.
