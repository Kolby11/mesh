# Status

**Updated:** 2026-09-21

## Now

The render-pipeline batch is largely landed. Service polls on the shipped
navigation bar now take the narrow path end to end — derived Luau state no
longer escalates to `TREE_REBUILD` — and a `backdrop-filter` grows damage only
by the blur kernel reach instead of collapsing the frame to the full surface.
Sparse display-list frames patch a retained batch-material index rather than
rebuilding the ordered primitive stream, and command storage is segments the
replay consumes directly. Measurements, and three corrections to the harness
that produced the original 2026-08-08 numbers, are in
[the performance log](log/performance-log.md).

The remaining frame-pipeline work is landed: semantic capture follows animation
and the authoritative retained diff, display signatures consume the retained
render fingerprints, and targeted finalization scopes accessibility and runtime
annotation work while retaining selection projections. Differential tests and
release measurements are recorded in [the performance log](log/performance-log.md).
Validation matches the unchanged revision's 32 shell and 10 elements fixture
failures; the renderer suite passes.

The accepted platform direction is consolidated in
[Platform Philosophy](../docs/spec/00-philosophy.md). Core owns platform
invariants and built-in settings/storage, inspection, and management mechanisms;
ordinary components provide their UIs. Luau is current; TypeScript/JavaScript
remains undecided. The public specs and audit prompt use this single authority.

The documentation work is complete. Four resulting implementation gaps are
tracked in [the backlog](../docs/BACKLOG.md): component profile roots, mandatory
typed service contracts, interface-defined service permissions, and props-layer
introspection. They are targets, not newly shipped behavior.

The 2026-09-01 audit synthesis is complete with 648 in-scope files and zero
unassigned. The next audit implementation item remains S02-LOGIC-001 /
S02-LOGIC-002. Current foundations include revision-checked settings/profile
commits, shared package transactions and recovery, immutable activation and
resource snapshots, canonical authoring contracts, resolved capability grants,
and CSS-derived surface geometry. The capability catalog is still closed.

Recent implementation evidence, validation limits, and known baseline failures
are in [September's log](log/2026-09.md) and [August's log](log/2026-08.md).
Measurements remain in [the performance log](log/performance-log.md).
