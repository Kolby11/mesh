# Status

**Updated:** 2026-08-28

## Now

Section 15's first twelve items and Section 16's path-safe uninstall, atomic
package mutation, typed live profile switching, shared CLI/shell package
ownership contract, and canonical graph-authoring snapshot are complete.
Package operations now share one core-owned typed owner/operation contract,
locked durable transaction journal, fsynced snapshots, staged writes, startup
recovery, and failure-injection regressions; the CLI and shell abort failed
mutations through that boundary, and new journals record their package
authority and operation while schema-v1 journals remain recoverable. Live
profile switches now wait for a typed committed/rejected generation
acknowledgement, and failed pre-commit activation restores the exact prior
active-profile pointer without overwriting a newer external change. CLI,
doctor, LSP, and runtime consumers now share the resolved canonical graph
snapshot and its content revision. The active runtime publishes immutable
activation snapshots, prepared frontends remain hidden until commit, and
shutdown advances through explicit quiescing, teardown, flushing, and stopped
phases. The next open item is Section 16's LSP manifest/schema validation. See
[`.planning/log/2026-08.md`](log/2026-08.md).
