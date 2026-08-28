# Status

**Updated:** 2026-08-28

## Now

Section 15's first twelve items and Section 16's path-safe uninstall plus
atomic package mutation items are complete. Package operations now share a
locked, durable transaction journal with fsynced snapshots, staged writes,
startup recovery, and failure-injection regressions; the CLI and shell abort
failed mutations through that boundary. The active runtime publishes immutable
activation snapshots, prepared frontends remain hidden until commit, and
shutdown advances through explicit quiescing, teardown, flushing, and stopped
phases. The next open item is Section 16's typed, exact-generation live profile
switching.
See [`.planning/log/2026-08.md`](log/2026-08.md).
