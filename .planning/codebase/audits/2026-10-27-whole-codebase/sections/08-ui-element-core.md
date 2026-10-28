# Section 08 — UI element core

## Scope and coverage

All 56 inventory files received systematic triage across the three layers.
The redundancy pass examined production bodies; the logic pass examined main
production paths and sampled some remaining branches. Embedded and standalone
tests were sampled, not exhaustively read. Neighboring shell, compiler,
interaction and renderer callers were traced selectively. Further detailed
logic and test coverage remains open; this is not an exhaustive review.

## Process tree

Compiled widget trees carry attributes, identity and state into style matching,
custom-property resolution and inherited styles. Layout lowers these to Taffy,
retains geometry and publishes frame and accessibility snapshots. The shell owns
scheduling and failure recovery; interaction and rendering consume the results.
Failed retained layout invalidates its state for a later rebuild.

## Performance findings

These are work patterns, not measured speedups, and overlap the existing S08
performance backlog. `style/resolve/subtree.rs:282` restarts at the root for
each dirty path; prior dashboard measurements in the performance log found this
cost negligible. Measure 1/16/256 targets at depths 2/16/64, shared and
disjoint paths, visits, allocations and whole-frame p50/p95 with parity.
`frame.rs:735` aggregates semantic children for affected ancestors; measure
16/1,024/10,000 siblings and deep chains, bytes and allocations with semantic
parity. Full capture stores cloned node paths (`frame.rs:634,1222`), potentially
quadratic in depth; measure depths 16/128/512 versus wide trees, peak bytes and
capture latency. Historical runtime-key-path work is related; no optimization
is proposed without new measurement.

## Dead code and redundancy

Fresh and retained text-measure inputs are separately assembled in
`layout/lowering.rs:27` and `layout/retained.rs:295`. Share the input projection
only with field-by-field fresh/retained parity and probe-history tests.
Compiler and retained style inheritance also duplicate field projection;
preserve explicit masks and parent-update behavior in parity tests. Full and
dirty semantic projections independently combine references and popovers;
their distinct indexes are needed, but a shared policy could reduce drift.
These are confirmed duplication, not demonstrated current failures. Several
public methods have no repository callers; external consumers are unknown, so
they are candidates for API review rather than proven dead code.

## Logic and core mechanics

- **S08-LOGIC-01 — Border-width animation can retain stale geometry.**
  `style/types.rs:1585` recognizes border width as layout-affecting, but
  `animation_bucket` at :1605 classifies it paint-only. Taffy receives border
  widths (`layout/lowering.rs:205`), while shell animation dirty classification
  (`shell/component/animation.rs:613`) and retained-layout reuse
  (`layout/mod.rs:590`) can skip layout. Classify it consistently and compare
  intermediate and final transition/keyframe geometry and hit bounds with full
  layout. High confidence, source trace; runtime regression not yet run.
- **S08-LOGIC-02 — Icon axis declarations bypass their lowering.**
  `style/resolve/index.rs:480` classifies all `--` properties as custom and
  `declaration.rs:403` returns after storing them, so the dedicated icon-axis
  branches at :1257 are unreachable through normal declaration paths. The
  documented icon axes and shipped volume widget use these properties. Project
  the resolved values while preserving custom-property cascade; test literals,
  inheritance, variables, restyles and rendered glyphs. High confidence, source
  trace; renderer reproduction needed.
- **S08-LOGIC-03 — Custom-property chains have no explicit work bound.**
  `style/resolve/value.rs:19,119,218,399` recursively resolves references and
  scans the active stack for cycles. Long acyclic chains can consume deep stack
  and quadratic membership work. Bound or iteratively resolve them with a
  diagnostic; test long var/prop chains and nested fallbacks. Source-confirmed
  complexity; practical threshold unmeasured. Distinct from theme alias work.
- **S08-LOGIC-04 — Semantic diffs omit keyboard shortcuts.**
  `accessibility.rs:642` populates shortcut metadata, but `frame.rs:1320`
  compares other fields without it. A shortcut-only change can produce an empty
  diff despite a changed snapshot. Add a field and full/dirty snapshot tests;
  downstream assistive-technology impact requires further tracing. Confirmed
  source trace.
- **S08-LOGIC-05 — Transition shorthand drops supported property names.**
  `style/parse.rs:467` keeps a separate property list from
  `TransitionProperties::of_property` (`style/types.rs:1411`): for example,
  shorthand `padding-left` and `row-gap` select no properties, while longhand
  selection recognizes them. Share the mapping and test shorthand/longhand
  parity through real animations, including aliases and unknown properties.
  High confidence, source trace; animation regression needed.

Additional candidates require reproduction: attribute equality based on rendered
strings may retain an older JSON type (`attributes.rs:167,252`); cached style
indexes identify rule slices by pointer and length (`style/resolve/index.rs:169`),
so in-place edits may reuse an old index; non-finite numeric attributes may be
accepted before semantic snapshot validation; and reference scanning may treat
`var(` inside quoted values as syntax. Trace real callers and test before
classifying user impact.

## Existing backlog or audit overlap

Existing S08 layout/style/semantic measurement work covers the performance
patterns. Earlier blanket claims about pseudo-state matching, inherited values,
custom-property cycles and retained-layout failure recovery are not supported by
the current implementation. The border classification test itself currently
expects paint-only behavior (`style/tests/animation.rs:216`).

## Refuted suspicions

Unknown tags are rejected; popover parsing reports invalid values. Retained
layout failures invalidate state. Text measurement carries shaping context and
resource revisions. These observations do not establish correctness of every
branch of those systems.

## Tests and benchmarks needed

Run the regressions described above through the real component and renderer
paths, including incremental/full parity. Benchmark only with release workloads,
repeated samples and correctness assertions; no performance gain is claimed.

## File coverage

The 56 assigned paths are listed in [the inventory](../00-coverage.md). Each
was triaged in all three layers; detailed logic and test coverage remains partial
as stated above. External callers were sampled.
