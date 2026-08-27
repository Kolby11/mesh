# Status

**Updated:** 2026-08-27

## Now

Section 15's first seven items are complete. Profile activation now prepares one
immutable `ActivationPlan`, resolves candidate components and backends against
its interface/capability snapshots, and publishes one `RuntimeGeneration` at
the commit point. Backend enable/disable, package graph changes, and installed
graph filesystem deltas now enter that same staged activation coordinator.
Backend tasks, bridges, messages, events, results, and restart deadlines now
carry activation generations and provider epochs, with stale identities
rejected at the shell boundary. File-watch, IPC, backend bridge, and restart
workers now own safe wake handles and lifecycle guards that remain valid until
their worker is stopped or joined. Component/runtime callback, tick, build,
render, reload, mount, and unmount failures now receive bounded placeholders,
diagnostics, sibling isolation, and repeated-failure quarantine. CoreRequest
effects now run through a fair bounded scheduler with causal budgets and cycle
detection. The next open item is publishing provider unavailable/recovery
transitions from committed provider generations.
See [`.planning/log/2026-08.md`](log/2026-08.md).
