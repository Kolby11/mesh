# Section 07 — Component language

## Scope and coverage

All 18 assigned component and expression files, including inline tests and
manifests, were read in the logic and redundancy passes. The performance pass
examined every file and the relevant production paths, but sampled rather than
read every inline test in the largest files. Compiler, runtime and tooling
callers were traced selectively, not exhaustively. See [coverage](../00-coverage.md).

## Process tree

`.mesh` source enters top-level block scanning, Luau props/script analysis,
brace and XML template lowering, shared CSS parsing and cross-block semantic
validation. Compiled expressions and imports feed the frontend compiler and
real Luau runtime, which create the element tree. Tooling uses recovery mode
and source spans for partially edited files. Reloads re-enter this pipeline.

## Performance findings

The following are source-confirmed work patterns, not measured speedups; they
overlap the existing S07 parser and semantic-index backlog:

- Component script parsing spawns and joins a deep-stack parser thread per
  parse, including inline handlers. Measure 1/32/256 handlers and 1/100
  standalone expressions, cold and warm, with thread counts, allocations,
  p50/p95 compile/LSP latency and diagnostics/span parity.
- AST and token access merging uses linear membership checks. Measure
  10/100/1,000/5,000 distinct accesses in valid and incomplete scripts;
  preserve source order, spans and recovery.
- Synthetic control-flow close-span lookup scans brace tokens. Measure
  16/256/1,024/4,096 blocks and token inspections, retaining span parity.
- Tooling recovery recursively retries shorter prefixes. Measure
  1/10/100/1,000 malformed constructs in 10–100 KiB files, latency and stack,
  retaining earlier completed nodes and diagnostics.

The existing expression parser worker and shared CSS cache have prior measured
work; do not infer their results apply to these separate paths.

## Dead code and redundancy

Standalone binding/call validation uses an unscoped identifier visitor while
event expressions use scoped compiled-expression metadata; this creates the
correctness issue in [interim 59](../INTERIM.md). Import and other script
metadata also use raw Luau literal text while another path decodes it
([interim 63](../INTERIM.md)). Share canonical analyses after parity tests.

`EventHandler` and `EventHandlerCall` public AST variants have only test
constructors in the repository but production consumers still accept them;
`SourceTag::type_name` also lacks repository callers. External users are
unknown, so review compatibility before removal. Props declaration checks
overlap between parser and validator; any consolidation must retain explicit
field-presence and diagnostic semantics.

## Logic and core mechanics

An isolated executable using the real parser reproduced the following; durable
component and frontend regressions remain necessary:

- Comments and CDATA containing braces can be treated as live expressions,
  and ordinary `--[` Luau line comments can be rejected. [Interim 58 and 62](../INTERIM.md).
- A style directive can evaluate its expression three times. [Interim 60](../INTERIM.md).
- Large accepted integral prop values saturate when projected to CSS.
  [Interim 61](../INTERIM.md).
- Escaped import literals can fail classification; this is not a path-escape
  claim. [Interim 63](../INTERIM.md).
- Authored marker-shaped attribute text can become a binding. [Interim 64](../INTERIM.md).
- The unscoped reference helper reports a function parameter as a global;
  full compiler behavior still needs a regression. [Interim 59](../INTERIM.md).

Relative background URL lowering and strict container bounds also cross this
section; see [interim 3 and 49](../INTERIM.md). A size prop with numeric bounds
accepts a unit-bearing string outside those numeric bounds, but the intended
policy for heterogeneous CSS units needs clarification before calling it a bug.

## Existing backlog or audit overlap

Parse sharing, semantic validation and CSS lowering have existing S07 backlog
work. New concrete defects are recorded in the backlog with interim evidence.
Public AST cleanup is an API-review candidate, not proven globally dead.

## Refuted suspicions

The component and theme paths share CSS parsing; props use Luau AST parsing;
the expression parser has a bounded cache and long-lived worker. Historical
claims of absent top-level validation or entirely separate CSS parsers are
stale. Tooling recovery remains an intentional API, not dead code.

## Tests and benchmarks needed

`nix develop -c cargo test -p mesh-core-component -p mesh-core-expression --lib`
passed 109 component and eight expression tests; one release profile was
ignored. Add regressions for the reproduced inputs, through actual Luau and
rendered styles where appropriate. The proposed author-facing component corpus
should compare valid and mid-edit runtime/tooling behavior, Unicode/CRLF spans,
imports, props, event closures and full-rebuild output. The performance
workloads above should report release distributions and allocations, with
correctness parity and no per-node instrumentation in timing runs.

## File coverage

Assigned and inspected across the review layers: 18 files; remaining: zero.
The performance pass's test-body sampling and selective cross-package caller
coverage are stated above. Global exclusions are in the coverage inventory.
