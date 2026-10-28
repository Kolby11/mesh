# Section 03 — Service contracts

## Scope and coverage

All six assigned service crate files, including embedded tests, were inspected
in performance, redundancy and logic passes. Shell and Luau proxy consumers
were sampled; cross-package consumers were not exhaustively reviewed. See the
[file inventory](../00-coverage.md) and [interim findings](../INTERIM.md).

## Process tree

Interface declarations are parsed, canonicalized and validated into compiled
schemas, policy and fingerprints. Immutable catalogs carry provider and version
selection to shell consumers and Luau proxies. State, event, argument and result
payloads are validated against those schemas; compatibility reports and
generated tooling artifacts derive from contract data. Provider and catalog
replacement cross the shell activation boundary.

## Performance findings

**S03-PERF-01:** Named-record validation in `contract.rs:783` reparses field
types for every nested record on each payload. Compile named field schemas once
if measurements justify it, preserving recursion/work bounds. Benchmark release
state/event validation for 1/100/1,000 records × 4/16 fields at 1/60 Hz,
measuring allocation and p50/p95 with validation parity. Confirmed repeated
work, impact unmeasured; overlaps the existing compiled-schema backlog.

## Dead code and redundancy

The raw and compiled contract paths remain parallel and are already tracked in
S03-DEAD backlog work. Most generator artifacts have no repository production
caller; they are public APIs, so external use must be checked before removal or
an authoring command added. Generator artifacts now use compiled schemas, so
the older report's raw-generator assertion is stale. `replace_catalog` remains
used by production shell paths. A snapshot-to-builder conversion discards graph
selection metadata; production impact is not established, so test selected and
blocked provider round trips before changing that public API.

## Logic and core mechanics

- Removing optional positional arguments is classified additive even though
  old callers can supply them. [Interim 2](../INTERIM.md).
- Generated mocks can violate arrays, records, ranges and return types; run
  generated Luau through compiled validation. [Interim 39](../INTERIM.md).
- Contract names can collide with proxy `state` and `events` namespaces.
  [Interim 40](../INTERIM.md).
- Required named-record additions can be classified additive despite breaking
  method inputs. [Interim 41](../INTERIM.md).
- Optional arguments preceding required ones can pass the proxy's count-based
  arity check without supplying the required position. [Interim 42](../INTERIM.md).
- Accepted service symbols can produce invalid Luau artifacts.
  [Interim 43](../INTERIM.md).

These are high-confidence source traces, not targeted regression results. Each
interim finding identifies its sources, recommended change and test.

## Existing backlog or audit overlap

Existing S03 compiled schema, immutable catalog and bounded transport work
remains relevant; repeated named-type parsing is a specific workload for that
work, not a new measured speedup. Previous claims that recursive named types
are unvalidated or generators use raw schemas are stale.

## Refuted suspicions

Named records and arrays are recursively validated with depth/work bounds.
Production catalog replacement is used. Builder resolution's full rebuild is
not established as a live shell hot path; do not characterize it as one without
a production trace and measurement.

## Tests and benchmarks needed

`nix develop -c cargo test -p mesh-core-service --lib` passed 63 existing tests.
Add the focused compatibility and real Luau proxy cases above, plus generated
mock execution against compiled state/result validation. The latter can become a
building block for the proposed component scenario harness. Run the specified
named-record release workload before changing the validation representation.

## File coverage

Assigned and inspected: six files, including their embedded tests. Remaining:
zero. External caller coverage is sampled; binary assets, build output,
vendored dependencies and Git data are excluded globally.
