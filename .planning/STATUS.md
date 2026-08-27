# Status

**Updated:** 2026-08-27

## Now

Section 15's first three items are complete. Profile activation now prepares one
immutable `ActivationPlan`, resolves candidate components and backends against
its interface/capability snapshots, and publishes one `RuntimeGeneration` at
the commit point. Backend enable/disable, package graph changes, and installed
graph filesystem deltas now enter that same staged activation coordinator.
Backend tasks, bridges, messages, events, results, and restart deadlines now
carry activation generations and provider epochs, with stale identities
rejected at the shell boundary. The next open item is giving detached workers
owned wake handles and lifecycle guards that outlive the eventfd until join.
See [`.planning/log/2026-08.md`](log/2026-08.md).
