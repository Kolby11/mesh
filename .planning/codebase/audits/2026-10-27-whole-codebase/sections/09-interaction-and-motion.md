# Section 09 — Interaction and motion

## Scope and coverage

All 19 assigned files were inspected across the three layers. Logic and
redundancy reviewed production paths; performance used systematic scans and
focused review, with some utility branches triaged. Inline tests and neighboring
shell, element and renderer callers were sampled. No tests or benchmarks ran.

## Process tree

Input is checked against widget eligibility, transforms and clips; focus,
capture, gestures and scroll decisions enter interaction transactions and
frames. Reconciliation removes ineligible owners. Style and layout changes feed
snapshots, while transitions and keyframes sample a clock and motion policy;
the shell schedules subsequent samples and cleanup.

## Performance findings

All are unmeasured and overlap existing S09 traversal/timeline work:

- `animation/src/keyframes.rs:172` rebuilds step boundaries between pairs of
  offsets for each deadline check. Measure 1/10/100 timelines, 2/10/100 stops
  and 1/30/120 steps, wake-check p50/p95, allocations and frame latency;
  preserve never-late scheduling, directions and cancellation. Any cached data
  must be bounded.
- `interaction/src/focus.rs:35` and `scope.rs:48,115` can repeat subtree
  searches per focus or menu target. Measure 16/256/1,024 items, modal on/off
  and nested groups, counting visits and allocations with focus-order parity.
- `scroll/overflow_cache.rs:63` repeatedly walks dirty path prefixes; scroll
  reveal clones offsets and resolves ancestor transforms (`scroll.rs:132`).
  Measure depths 2/16/64 and 1/16/256 shared/disjoint dirty paths, plus
  1/8/64 nested scrollers and 16/1,024 unrelated offsets. Check full annotation,
  transformed reveal and clamping parity.

Existing fused input paths and compiled keyframes supersede parts of older
blanket whole-tree and timeline claims. Do not repeat the rejected small-key
popover search fusion without new evidence.

## Dead code and redundancy

`animation/src/box_shadow.rs` has a public shadow representation and parser
whose repository consumers are its tests, while live transitions use the
elements representation and another interpolation implementation. Their parse
contracts differ; review canonical ownership with compatibility and parity
tests before removal. External consumers are unknown. Scrollbar geometry is
also independently computed by interaction and renderer; test actual paint
and hit/drag boundaries at fractional scales and tiny tracks before unifying.
Public transform helpers lack repository callers and their documentation
describes older translation-only behavior; review the API and update its
contract, retaining tests for rotations and skew if kept.

## Logic and core mechanics

- **S09-LOGIC-01 — Transformed scroll reveal mixes coordinate spaces.**
  `interaction/src/scroll.rs:143` computes screen-space bounding boxes, then
  adds their distances directly to logical scroll offsets at :173. Scaling or
  rotation changes the relationship between those spaces. Convert through the
  relevant transform and test nested scaled/rotated containers, reprojected
  target visibility, clamping and untransformed parity. High confidence source
  trace; end-to-end reproduction needed.
- **S09-LOGIC-02 — Scrollbar hit testing ignores sibling paint order.**
  `scroll.rs:389` traverses children in reverse insertion order while other
  scroll hit testing uses reverse paint order at :349. Overlapping scrollbars
  with different z-indices can select a visually lower sibling. Test overlapping
  and nonoverlapping siblings against renderer order. High confidence source
  trace; renderer integration regression needed.
- **S09-LOGIC-03 — Reverse stepped keyframes may miss a boundary change.**
  `animation/src/keyframes.rs:172` schedules step boundaries and excludes the
  current boundary on the next check. For decreasing directed progress,
  `easing.rs:68` can change immediately after the boundary, while the next wake
  advances to a later boundary. Test reverse and alternate `steps()` just
  before, at and after boundaries through the held-animation scheduler; ensure
  the next deadline is never late. High confidence mathematical/source trace;
  runtime reproduction needed.

## Existing backlog or audit overlap

Existing S09 performance work covers traversal and timeline cost; the interim
audit already tracks offscreen keyboard focus and easing keyword curves. The
coordinate-space and stacking issues above are distinct. Earlier blanket claims
of translation-only hit testing, absent motion policy, broken pause/resume and
unimplemented shadow parsing do not describe current implementations.

## Refuted suspicions

Legacy and fused hit-test APIs have production consumers. The separate public
shadow parser is implemented; it is not a panic stub. These observations do not
prove all edge cases or public API consumers are covered.

## Tests and benchmarks needed

Run the specified geometry, scheduling and rendering regressions with actual
callers; test correctness at boundaries and under simultaneous interaction.
Run performance workloads in release with repeated samples and parity checks;
no speedup is asserted.

## File coverage

All 19 paths appear in [the inventory](../00-coverage.md). Production paths were
reviewed, with test bodies and external callers sampled as described above.
