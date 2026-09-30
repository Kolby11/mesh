# Status

**Updated:** 2026-09-30

## Now

Quick Settings and popover fixes landed 2026-09-30 (`eb9215f2`, `ada52286`).
Promoted popovers paint from their unclipped origin, size to their content,
and Quick Settings animates open and closed. Measured text widths now round up,
so short multi-word labels no longer wrap. S11-DEAD-001 (`62b0cfbb`) and
last-valid-manifest retention during live edits (`98efc62e`) are closed. Details
are in [September's log](log/2026-09.md).

Navigation/settings warning fixes are implemented: promoted child paint uses
the retained widget-tree revision, and frame effect snapshots discard only
superseded requests. Resource reloads no longer alter widget caller lineage;
bubble-options declares its icon-pack fallbacks. Validation and limits are in
[September's log](log/2026-09.md).
The follow-up closes terminal-animation dirty tracking and adds transform origin,
visibility and white-space to retained style fingerprints. Lineage diagnostics
now identify the root module/key; the runtime checks remain enabled.

A 2026-09-30 profiling pass on the navigation bar landed three fixes, committed
alongside the hover/focus work: failed runtime creation for a
missing interface is remembered instead of rebuilt every frame, runtime style
diagnostics re-resolve only retained-dirty nodes, and accessibility
normalization reads attributes in one pass. A volume poll went from 4.74–4.90 ms
to 3.06–3.17 ms. It also corrected `navigation_frame_cost_profile`, which forced
paints a session never makes and never hovered the bar. Details and the
reverted experiment are in [the performance log](log/performance-log.md).

The "Scroll and animation frames" waves 1–3 are implemented and measured,
committed: one paint per animated frame, paint-only scroll ticks, no blur of
empty in-surface backdrops, opt-in profiling overhead, cheaper paint-only
frames, and compositor blur through `ext-background-effect-v1` (Hyprland now
blurs the bar on its GPU; MESH does no pixel work for it). The settings scroll
gate went from 13–30 ms to 8.2–8.4 ms p50 over waves 1–2. Remaining: the
content-space scroll and raster shift (wave 4) and the in-surface blur engine
(wave 5), in [the backlog](../docs/BACKLOG.md); details in
[the performance log](log/performance-log.md).

Wave 4 item 14 is implemented for translation-only content without fixed or
compositing/readback dependencies. Offset-only frames retain descendant command
handles and entries under one anchored, rounded viewport translation. Nested
scrollers keep their existing geometry inside the outer content scope; inner
scroll changes rebuild that content. Complex cases retain the established path.
Three alternating release Appearance runs: p50 4.71–5.07 ms before versus
4.15–4.52 ms after; entry rebuilds fall from about 25,000 to zero. Renderer:
264 passed; focused shell scroll tests retain the prior five fixture failures.
Wave 4 item 15 is in flight: [bounded raster shifting](todos/pending/2026-09-30-scroll-raster-cache.md).
Overlap-safe byte shifting and exposed-strip text repair pass independent pixel
tests. Cache allocation, invalidation, composition and profiling are not wired
into production; item 15 remains open. Item 14 is committed as `9558a889`.
The live hover capture has no sampled client blur work, but no matched 20%
blur-saving claim is made. Measurements are in the performance log.

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
