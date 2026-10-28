# Section 05 — Localization and i18n

## Scope and coverage

The three assigned locale crate files, including inline tests and benchmark
bodies, were inspected across performance, redundancy and logic reviews.
Relevant shell, component and Luau callers were sampled; they were not
exhaustively reviewed. See [coverage](../00-coverage.md).

## Process tree

Locale selection is normalized into a fallback chain. Graph-authorized,
bounded catalog sources are read, parsed and compiled into ordered module and
language-pack layers. An immutable catalog and selection snapshot serves scoped
translators and formatters; shell and Luau consumers publish and refresh it
across selection, reload and profile changes.

## Performance findings

Date/time calls construct ICU formatters repeatedly, including shipped clock
paths; [interim 51](../INTERIM.md) specifies release workloads and output,
timezone and cache-bound parity. Impact is unmeasured. Catalog preparation
spawns a worker and immediately joins it, leaving the caller blocked; this
overlaps existing blocking-I/O work. Benchmark 4/40/400 catalogs × 100 keys
and 1/16 consumers, recording frame lateness and input latency. A 2026-10-27
release source-preparation fixture measured 400 warm catalogs, three runs of
10 samples: p50 17.378–19.578 ms and p95 20.685–23.448 ms; that is not a
UI-latency measurement. See the performance log for the workload.

## Dead code and redundancy

Legacy flat `TranslationSet` and loading adapters are public and have only
test/benchmark repository callers; their external use remains unknown. This
overlaps the historical S05-DEAD-001 review. Module point and bulk resolution
now share precedence traversal, while core APIs still disagree on terminal
fallback as described below.

## Logic and core mechanics

- Core point and bulk translation APIs omit English last resort when the
  argument-aware resolver uses it. No production repository consumer of the
  public core translator was found. [Interim 50](../INTERIM.md).
- Placeholder validation misses an unmatched closing brace before a later
  placeholder. [Interim 52](../INTERIM.md).

Both are confirmed source traces; add the targeted regressions described in
the interim report rather than assuming existing tests cover them.

## Existing backlog or audit overlap

Catalog I/O, live preparation and localized component-mount cost already have
backlog entries. Older broad claims about an owned translator module ID and
duplicated module lookup precedence are stale: IDs are borrowed and the module
path shares one iterator. The flat adapter remains a public API review item.

## Refuted suspicions

The current module lookup supports layered fallback with shared point/bulk
precedence. ICU plural and decimal formatter caches are already bounded and
benchmarked; do not repeat those experiments. Graph-owned catalog handles use
bounded, contained reads; authoring diagnostics' separate catalog read remains
the distinct issue recorded in interim finding 11.

## Tests and benchmarks needed

`nix develop -c cargo test -p mesh-core-locale --lib` passed 28 tests with two
release profiles ignored. Add core lookup parity and malformed-brace tests.
For the proposed component harness, mount two modules with overlapping key
names, switch locale and pack order, assert text, provenance and semantics,
then verify failed or stale catalog preparation retains the previous snapshot.
Measure locale mount/reload latency separately from formatter and frame cost.

## File coverage

Assigned and inspected: three files; remaining: zero. External callers were
sampled. Build outputs, binary assets, Git data and vendored code are excluded
globally.
