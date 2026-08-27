# Status

**Updated:** 2026-08-27

## Now

Section 15's first two items are complete. Profile activation now prepares one
immutable `ActivationPlan`, resolves candidate components and backends against
its interface/capability snapshots, and publishes one `RuntimeGeneration` at
the commit point. Backend enable/disable, package graph changes, and installed
graph filesystem deltas now enter that same staged activation coordinator. The
next open item is tagging backend tasks, bridges, messages, events, results,
and restart deadlines with activation generations and provider epochs. See
[`.planning/log/2026-08.md`](log/2026-08.md).
