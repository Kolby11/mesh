# Section 06 — Host resources and icon packs

## Scope and coverage

Three passes inspected all 15 assigned files, including source, manifests,
tests and tool files. The 4,266-entry codepoint map was structurally checked
for unique names and valid scalar values, not individually compared with
upstream. Shell and renderer consumers were sampled. See [coverage](../00-coverage.md).

## Process tree

Host XDG roots and font database combine with graph-selected packs and module
bindings. Bounded, cancellable preparation validates fonts and glyph data;
registry publication advances resource revisions. User and author overrides,
module and shell chains, host fallback and prepared workers resolve icons and
fonts for rendering. Worker completion identifies consumers to repaint; graph
replacement and cancellation retire prior-generation work.

## Performance findings

All are source-confirmed repeated work with unmeasured impact and overlap
existing S06 resolution or resource-preparation backlog work:

- Warm icon cache hits scan an LRU deque; measure 16/256/1,024/2,048 occupied
  entries and over-capacity churn, with cache and eviction parity.
- Font alias projection is reconstructed during paint; measure 1/16/128 packs,
  4/64 roles and 1/16 modules, allocations and frame p50/p95, retaining alias
  and pixel parity on revision changes.
- Candidate preparation reads and parses bundled faces more than once. Measure
  1/16/64 faces × 1/16 MiB, bytes, peak memory and cancellation/family parity.

## Dead code and redundancy

Coverage advice and live icon fallback use different vocabularies and owner
representations, producing advisory mismatches. [Interim 53](../INTERIM.md).
Some incremental default-registry setters have no repository consumers, but
the shared registry itself is live; external public API use is unknown. Review
compatibility before removing wrappers.

## Logic and core mechanics

- Duplicate JSON glyph names collapse before duplicate validation.
  [Interim 54](../INTERIM.md).
- Pack-qualified icon overrides bypass MESH mapping packs.
  [Interim 55](../INTERIM.md).
- An absent data home can place a system resource directory ahead of legacy
  user paths. [Interim 56](../INTERIM.md).
- Override-only font roles can be missing from renderer alias projection.
  [Interim 57](../INTERIM.md).

These are source traces; add targeted end-to-end regressions. The ordering of
hicolor versus dash-generalized names is a target-conformance question because
generalization is marked Target in the icon specification, not a shipped bug.

## Existing backlog or audit overlap

Resource lookup, invalidation and preparation measurements overlap existing
S06 performance work. Generalization and detailed font-pack target work remain
targets. Do not duplicate historical claims that the shared registry is dead or
that cold filesystem lookup holds the committed registry lock.

## Refuted suspicions

Positive and negative icon caches are bounded, and cold resolution operates on
private snapshots. Frozen, prepared font data and generation-aware publication
supersede older blanket claims about paint-time path reads or non-atomic resource
replacement; regressions should test the remaining seams rather than assume
those old paths remain.

## Tests and benchmarks needed

`nix develop -c cargo test -p mesh-core-icon -p mesh-core-resources --lib`
passed 36 and 29 tests respectively. Add advisor/live parity, duplicate glyph,
qualified override, absent-data-home and override-only font text tests. For a
component harness, mount two module-specific chains with cold, warm, missing and
pending icons; replace/remove a pack and assert pixels, provenance, repaint
scope and stale-completion suppression. Measure worker/IO and warm frame costs
separately at the sizes listed in the [interim report](../INTERIM.md).

## File coverage

Assigned and inspected across three layers: 15 files; remaining: zero. The
codepoint map check and sampled external consumers have the limits stated above.
Global build, binary, Git and vendored exclusions are in the coverage inventory.
