# Section 04 — Themes

## Scope and coverage

Three passes inspected all 18 assigned files, including theme crate source,
embedded tests, token data and shipped CSS. Stylesheets were read directly or
compared line by line with a fully read baseline. Shell and component callers
were sampled rather than exhaustively reviewed. See [coverage](../00-coverage.md)
and [interim findings](../INTERIM.md).

## Process tree

Graph-owned theme descriptors and modes lead to bounded source reads and shared
CSS parsing. Base, pack, module, family and user layers compose tokens, rules,
defaults and provenance. Revisioned theme data and token resolution feed
component styling, animation and bounded `mesh.theme` transport; reload errors
are reported while the previous usable state remains available.

## Performance findings

- **S04-PERF-01:** CSS source-location lowering repeatedly scans from the
  beginning of the stylesheet. Benchmark cold release parses at 100/1,000/
  10,000 rules, measuring p50/p95 and allocations with UTF-8/span parity; a
  line index is a candidate only if measured. Related to existing CSS parse work.
- **S04-PERF-02:** Detail pagination repeatedly serializes and hashes the full
  snapshot. [Interim 47](../INTERIM.md) specifies sizes, consumers and parity
  checks. Both are confirmed repeated work with unmeasured impact.
- Alias resolution can recurse deeply on acyclic chains; see
  [interim 45](../INTERIM.md) for the resource bound and depth benchmark.

## Dead code and redundancy

Global and module style-rule lowering duplicate token/default/selector logic;
share the operation while preserving distinct at-rule policy and test ordering.
[Interim 46](../INTERIM.md). Several public theme accessors lack repository
callers, but external API compatibility is unknown. Similar shipped theme
stylesheets are independently editable packages; similarity alone does not
establish a defect.

## Logic and core mechanics

- Relative background URLs are rewritten to absolute paths rejected by style
  parsing; theme `@module` bodies also miss their asset base. [Interim 3](../INTERIM.md).
- Appended module rules overwrite provenance indices. [Interim 44](../INTERIM.md).
- Scoped token splitting mishandles dotted module IDs in overrides and
  non-flattened lookup. [Interim 48](../INTERIM.md).
- Strict container query comparisons lose strictness. [Interim 49](../INTERIM.md).
- Public mutable module-token and keyframe APIs can leave flattened or
  revision-keyed caches stale; no current production mutation demonstrating
  those paths was found. Test compose-then-mutate and warmed animation cache
  before narrowing APIs. These were noted in the older audit.

The new issues above are source traces, not targeted regression results; the
interim report records their proposed tests and confidence.

## Existing backlog or audit overlap

CSS parsing, resource invalidation and theme-switch workloads have existing
performance work. The module-token/keyframe public API concerns are historical;
avoid describing them as demonstrated live failures. The URL issue is shared
with component language and elements.

## Refuted suspicions

Theme sources use graph-authorized contained handles, the CSS parser is shared,
general component animation consumes theme keyframes, and duplicate theme IDs
are rejected. The parser cache is bounded. Those current paths supersede older
claims of a graph-bypassing loader, separate parsers and tooltip-only keyframes.

## Tests and benchmarks needed

`nix develop -c cargo test -p mesh-core-theme --lib` passed 49 tests with two
release measurements ignored. Add the focused URL, provenance, dotted-ID,
strict-boundary and alias-depth regressions. A reusable component scenario
should mount shared token users, switch mode and scoped values, then verify
last-known-good behavior after malformed theme input against full-rebuild pixels
and semantics. Profile parse, composition, affected restyle, layout and damage
at 100/1,000/10,000 nodes; existing D9 theme switching excludes file loading
and durable writes.

## File coverage

Assigned and inspected across the three layers: 18 files; remaining: zero.
External callers were sampled; global exclusions are documented in coverage.
