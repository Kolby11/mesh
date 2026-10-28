# Section 02 — Module system and installation

## Scope and coverage

The performance and redundancy passes inspected all 49 assigned crate files
and 82 shipped manifests. Logic passes collectively inspected all crate
files, embedded and standalone tests, and read the manifests by category;
the final inventory pass also checked manifest JSON, unique names and primary
entrypoints. Relevant shell, CLI and runtime callers were traced selectively,
not exhaustively. See [coverage](../00-coverage.md) for assigned paths. The
following findings retain detailed locations, confidence, and suggested tests
in the [interim report](../INTERIM.md); no fixes were implemented.

## Process tree

Root graph, profiles, manifests and lock enter discovery and provenance
assessment; composition and fork substitutions feed dependency closure,
provider selection and contribution indexing. Package changes prepare and
review candidates, journal durable file and profile mutations under locks,
publish immutable activation snapshots and commit, then clean up. Fork creation
and rebase have separate file mutation paths. Discovery failures, trust policy,
activation rollback and recovery cross the shell and CLI boundaries.

## Performance findings

- **S02-PERF-01:** Serial per-module signature subprocesses on graph load are
  a measurement hypothesis. Benchmark 1/32/128 signed and unsigned modules,
  cold/warm release runs, process counts, p50/p95 and signature parity.
  [Interim 31](../INTERIM.md).
- **S02-PERF-02:** Repeated authoring token-set construction, backend parsing
  per interface, dependency failure propagation, and whole-file fork/digest
  reads need the workloads and measurements in the interim report's
  graph-load profiling notes. Their cost is unmeasured; existing S02/S15 items
  already cover related parse, hashing and graph preparation work.
- The source scan's existing byte budget does not bound traversal depth or
  rejection count; this is a resource-bound correctness concern, not a
  measured speedup. [Interim 14](../INTERIM.md).

## Dead code and redundancy

- **S02-DEAD-01:** Composition duplicates the canonical settings JSON merge;
  consolidate after nested object, scalar, array and null parity tests.
  [Interim 20](../INTERIM.md). Confirmed source duplication; impact is
  maintainability.
- Public graph and manifest accessors and unversioned lock/profile writers
  have no production repository consumers; external users remain unknown.
  Review API compatibility before narrowing. The historical duplicate install
  review and unbounded source-byte findings are stale; current code uses shared
  review and bounded source reads.
- Runtime manifest normalization appends lists twice; this is observable data
  drift as well as redundant work. [Interim 19](../INTERIM.md).

## Logic and core mechanics

The most consequential confirmed findings are trust-rejected dependencies being
reactivated by closure and dependencies of blocked roots remaining active;
both were reproduced in isolated graph fixtures. [Interim 21 and 33](../INTERIM.md).
Further source-traced findings cover fork creation/rebase and root rewiring
(1, 12, 15, 22); composition roots and CLI profile mutations (13, 23, 24,
32); extension-point versions, types, cardinality, identity and replacement
(26–28, 30, 36); bounds and loading isolation (11, 14, 16, 29, 37); snapshot
publication, pointer writes and post-commit cleanup (17, 34, 35); binary
dependency checks (25); and late interface blocking (38). The interim report
distinguishes reproduced defects from high-confidence source traces and
explains each regression. No blanket claim of live reproduction is made.

## Existing backlog or audit overlap

The existing S02 package/store, manifest-sharing and authoring-refresh items
remain applicable. The transaction cleanup outcome relates to S15 post-commit
recovery; graph startup and signature workloads relate to S15 preparation
profiling. Each genuinely new issue is linked from the backlog. Historical
audit conclusions were checked against current code; fixed source-byte bounds,
signature path checks and serialized CAS are not repeated as open defects.

## Refuted suspicions

Empty localized keybind triggers have an explicit runtime diagnostic and
fallback. Directly disabled but trusted required dependencies are intentionally
inferred; that behavior is distinct from reactivating trust-rejected modules.
Shared Luau VMs are permitted; the per-contribution VM comment is documentation
drift, not evidence of an isolation violation.

## Tests and benchmarks needed

The module library suite passed in the earlier pass (346 passed, one ignored).
Add graph regressions for trust and viable-root closure; graph-to-component
fixtures for extension-point cardinality, identity and replacement; temporary
profile/CLI fixtures that reload durable results; failure injection for fork
rebase and post-commit cleanup; and bounded read and scan tests. Measure graph
startup phases separately from frame profiling, with the workload sizes and
parity checks in the interim report. Component scenarios can reuse the existing
memo and shipped-surface fixtures as described there.

## File coverage

Assigned: 131 files, comprising 49 crate files and 82 module manifests.
Inspected across the three layers: 131; no assigned files remain. Cross-section
callers were sampled, not exhausted. Build outputs, binary assets, Git data and
vendored dependencies are excluded globally.
