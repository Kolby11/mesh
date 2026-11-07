# Cross-section findings — interim, November 7

This is a source-review checkpoint, not the final audit. See [coverage](00-coverage.md) for unread files and incomplete independent layers. No source changes, builds, tests or benchmarks were performed during this read-only continuation. Earlier uncommitted implementation edits remain separate and are not evidence that their backlog items are complete.

## Reported shell-wide settings latency

The reported roughly four-second delay has not been reproduced or traced end to end. The following mechanisms are independently visible in source; their costs must not be added together or presented as a measured explanation.

| Boundary | Evidence | Consequence to measure |
| --- | --- | --- |
| Settings to every frontend root | [S10-PERF-21](sections/10-frontend-compiler-and-host.md): theme invalidation drops authored-tree, layout and retained rendering caches; Settings also instantiates inactive pages | Cold preparation work across visible roots and hidden page instances |
| Appearance projection | S10-PERF-29 in the same report: broad effects repeat resource, theme, font, locale and glass projections | Repeated catalog traversal and derived-state writes per actual settings change |
| Frontend frame publication | S10-PERF-22: observation and diagnostic summaries are reconstructed across embedded runtimes | Extra frame work even when those summaries are unchanged |
| Render resource identity | [S12-PERF-21/22](sections/12-rendering-and-paint.md): global completion epochs and alias-map switching invalidate unrelated recordings | Lost reuse across stable surfaces while other surfaces load resources or use distinct aliases |
| File notification to graph preparation | [Section 15](sections/15-shell-core-and-orchestration.md): known settings paths and unknown authoring changes require different preparation | Distinguish effective settings application from later watcher-triggered duplicate work; account for existing uncommitted routing edits |

The next measurement needs one timestamped change followed through durable commit, effective settings publication, per-root preparation, resource completion and compositor presentation. Use theme, font family, text size and shell scale separately; include visible surface count, Settings page state and cold/warm resource state. Record repeated release ranges and final-value correctness. Historical isolated timings in section reports are not an end-to-end baseline.

## Cross-section correctness contracts

- Service identity must advance independently of visible payload equality. [S11-LOGIC-27](sections/11-luau-runtime-and-sandbox.md) traces a context guard that fails this while the frontend cache advances correctly.
- Paint identity, subtree rejection and compositor blur collection must use the same geometry and fixed-position transform rules. [S12-LOGIC-21/22](sections/12-rendering-and-paint.md) identifies disagreements that retained reuse can expose.
- Surface policy must carry the same supported fields from authoring through live diff and protocol configuration. [Section 13](sections/13-surface-policy-and-configuration.md) traces omitted centering and ambiguous optional clearing; [section 14](sections/14-wayland-platform-and-presentation.md) adds logical-output fallback and seat-event cancellation defects.
- Authoring tools must preserve source bytes and canonical component structure. [Section 16](sections/16-developer-and-authoring-tools.md) records UTF-8 formatting corruption, invalid completion boundaries, raw block lookup and prop/public-state confusion.

## Remaining evidence limits

Sections 13, 14 and 16 have complete assigned-file reads, but independent review layers and the cross-section audit remain incomplete. Section 11 has 83 full and 10 partial files; those partial files still contain unread bodies/tests. Section 12 has 35 full, 8 partial and 35 unread. Section 10 and shell coverage remain partial. Sections 01–09 retain historical evidence rather than a claimed current reread. The earlier quota-limited pass was resumed with new section agents; unsaved candidates were not counted as verified findings.

## Later November 7 continuation

The renewed section agents completed tooling file reads and expanded runtime, shell, painter and input coverage. Newly recorded contracts include preserving supervision history across successful restarts, continuing recovery after replacement init failure, retaining bounds through the final shell inbox, reclaiming completed ticket/task records, and dispatching hidden effects from optimistic state publication. Input review found missing invalidation for commit-only IME edits and shortcut overrides preceding reserved slider mechanics. Painter review found unbounded repeated diagnostics, inconsistent masked-selection offsets and oversized backdrop-cache admission. Details, caveats and proposed regressions are in sections 10–12 and 15–16; these are source-backed findings, not executed reproductions or measured speedups.
