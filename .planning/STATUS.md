# Status

**Updated:** 2026-08-27

## Now

Section 15's first four items are complete. Profile activation now prepares one
immutable `ActivationPlan`, resolves candidate components and backends against
its interface/capability snapshots, and publishes one `RuntimeGeneration` at
the commit point. Backend enable/disable, package graph changes, and installed
graph filesystem deltas now enter that same staged activation coordinator.
Backend tasks, bridges, messages, events, results, and restart deadlines now
carry activation generations and provider epochs, with stale identities
rejected at the shell boundary. File-watch, IPC, backend bridge, and restart
workers now own safe wake handles and lifecycle guards that remain valid until
their worker is stopped or joined. The next open item is isolating
component/runtime callback, tick, build, render, and reload failures with
placeholders, diagnostics, and quarantine.
See [`.planning/log/2026-08.md`](log/2026-08.md).
