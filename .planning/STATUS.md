# Status

**Updated:** 2026-11-07

Open work lives in [`docs/BACKLOG.md`](../docs/BACKLOG.md); history and
measurements live in [`log/`](log/README.md).

## Now

- Settings theme recommendation lookup and consumer-scoped scene invalidation are optimized and measured in the working tree. Cache admission while resources are pending, unchanged-frame overhead, observation snapshots, list virtualization and end-to-end settings latency remain open.

- Read-only codebase improvement scan with section agents, as clarified by the user. Record source-backed findings and exact coverage in the November 6 audit directory. Existing uncommitted implementation edits predate this clarification.

- Review the reported roughly four-second shell-wide settings delay through transactions, appearance resources and frontend/render invalidation. Global cache resets, repeated observation snapshots and broad resource effects are source-backed cost candidates; the end-to-end critical path remains unmeasured.

- Source-reading coverage: sections 13, 14 and 16 have complete assigned-file reads. Section 11 has 83 full / 10 partial files; section 12 has 35 full / 8 partial / 35 unread. Frontend and shell remain partial; independent review layers and cross-section reconciliation remain open.

- Earlier implementation work remains uncommitted and is not being extended during this scan: settings routing, appearance preparation, retained rendering, fixtures and component extraction. Validate it separately before treating backlog items as complete.


## Next

- Finish unread frontend, rendering and shell inputs plus partial runtime bodies/tests from the audit inventory.
- Complete independent review layers and cross-section reconciliation before issuing the final audit report.
- Resume implementation and verification from the canonical backlog after the scan; earlier dirty changes remain unverified as a combined set.

## Blocked / known baselines

- D1 compound-table write tracking remains deferred pending supported runtime
  observation; indirect list reads still compare every item.
- GPU panel recorded-scene parity exceeds its pixel budget on the baseline.
- The scrollbar part palette regression passes when it supplies damage to the
  replay selector. The retained-display geometry workload shares the resource
  test lock with revision-mutating tests; three two-thread renderer runs pass.
- NVIDIA as compositor main device and other compositor imports remain unchecked
  live.

- Default-parallel shell checks have intermittent fractional-layout and debug
  Surfaces assertions; isolated reruns and the complete two-thread run pass.
