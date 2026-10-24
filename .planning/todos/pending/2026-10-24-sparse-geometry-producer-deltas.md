# Sparse geometry producer deltas

Linked from the P2 geometry item in `docs/BACKLOG.md`.

## Required evidence boundary

`layout/sparse.rs::write` checks every child of a dirty ancestor because Taffy
can reposition or resize an otherwise clean sibling. Taffy 0.10.1's public
`TaffyTree::compute_layout_with_measure` does not expose the nodes whose
unrounded layouts it wrote; its `TaffyView` and `as_layout_tree` adapter are
private. A dirty script-node set is not a geometry-change set.

`refresh_overflow_paths` re-unions siblings because a shrinking child may have
owned the parent's right or bottom extent. Current scroll metrics store the
union's width/height, not each child's prior contribution. Reconstructing that
contribution from current geometry and a tolerance would not establish an exact
invalidation boundary.

## Design constraint

Narrowing write-back requires a producer-owned list of written/changed Taffy
rects, including changes caused by siblings, available space, wrapping,
alignment, flex distribution and visibility. That list must survive the
rounding/projection step and carry exact before/after geometry. Use a supported
layout adapter or an explicit upstream hook; do not infer untouched siblings
from child index or script dirtiness.

Overflow updates then need authoritative old/new propagated bounds and a
bounded child-union index, maintained at full/structural layout boundaries.
Changing children updates their leaves; parent shrinkage recomputes the union
from that index. Style changes affecting clipping, padding, fixed/promoted
boundaries and visibility must invalidate/update the same state. Missing
history or mismatched topology keeps the full safe traversal.

## Exit evidence

Compare every rect, scroll metric, offset clamp, retained command and pixel
against full recomputation for 16 and 1,024 siblings. Include early/middle/last
child growth/shrink, wrap, flex grow/shrink, alignment, intrinsic text,
fractional coordinates, clipping and nested fixed/promoted content. Record
producer visits as well as downstream visits; moving the sibling scan into an
uncounted pass does not meet the gate. Keep the current fallback until these
proofs and repeated release measurements pass.
