# Status

**Updated:** 2026-08-31

## Now

Section 15's first twelve items and Section 16's path-safe uninstall, atomic
package mutation, typed live profile switching, shared CLI/shell package
ownership contract, shared CLI/shell package transaction engine, canonical
graph-authoring snapshot, typed update flags with clean replace behavior, and
syntax-aware LSP analysis are complete. Package operations now share one
core-owned typed owner/operation contract, locked durable transaction journal,
fsynced snapshots, staged source writes, root/profile/lock persistence, exact
rollback reconciliation, startup recovery, and failure-injection regressions;
the CLI and shell abort failed mutations through that boundary, and new
journals record their package authority and operation while schema-v1 journals
remain recoverable. Live
profile switches now wait for a typed committed/rejected generation
acknowledgement, and failed pre-commit activation restores the exact prior
active-profile pointer without overwriting a newer external change. CLI,
doctor, LSP, and runtime consumers now share the resolved canonical graph
snapshot and its content revision. LSP manifest diagnostics now run the same
canonical module and root runtime contracts as activation while retaining
source-aware editor diagnostics; component tooling now retains partial
template/script ASTs and reports parser-owned Luau member spans during edits.
JSON authoring now uses standards-aware tokenization and strict parsed code
maps for decoded values and source-accurate spans, while Luau completion
contexts use the full_moon token stream for comment- and string-safe recovery.
The active runtime publishes immutable
activation snapshots, prepared frontends remain hidden until commit, and
shutdown advances through explicit quiescing, teardown, flushing, and stopped
phases. Module activation now rejects unsatisfied required dependency closures,
incompatible module/interface versions, invalid composition pins, and duplicate
contract identities before runtime contributions are indexed. Module lifecycle
and health are now reconciled
at graph commit, frontend/backend activation, candidate failure, recovery,
quarantine, and teardown boundaries. The module lock now uses schema v3 with
validated versions, direct dependency requirements, reverse requesters,
composition provenance, and rollback metadata rebuilt from restored manifests.
The next open item is splitting renderer/Wayland/package/debug policy out of
the compiler-facing frontend host ABI.
Host icon/font resources now use one explicit immutable graph/profile candidate
catalog and copy-on-write registry handle, with failed preparation, recovery,
and package rollback retaining the last-known-good snapshot. Icon resolution
now validates canonical module identities, keeps vocabulary mappings scoped to
their requesting owner, preserves typed color policy and chain order, and
reports effective requirement provenance from that snapshot.
`aspect-ratio` is now part of the bounded style profile, and the shipped
navigation bar derives its control sizing from the bar root through it rather
than restating px sizes per component. Surface geometry now follows the same
CSS authority: the measured root box supplies content size, its margins lower
to layer-shell placement, and the anchored outer edge supplies the exclusive
zone; overlays opt out with `exclusive-zone: none`.

The transition animator now runs one in-flight instance per entry of a
comma-separated `transition`, and the `transition-*` longhands now retain
independent comma lists with CSS-style repeat/truncate matching. Entries with
different durations, delays, and easings animate on independent timelines.
See
[`.planning/log/2026-08.md`](log/2026-08.md).
