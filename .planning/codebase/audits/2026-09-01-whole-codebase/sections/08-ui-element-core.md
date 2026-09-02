# Section 08 — UI element core

## Scope and coverage

Reviewed all 51 assigned files under `crates/core/ui/elements/`, including
contracts, retained nodes, styles, layout, events, semantics, LRU caches, and
tests, plus the element/accessibility documentation and callers. **51/51
assigned files inspected; no follow-up remains.**

## Process tree

```text
.mesh/compiler attributes and element contracts
  -> WidgetNode identity/tree and runtime state
  -> style matching/cascade and computed style
  -> retained Taffy lowering, measurement, geometry
  -> pointer/keyboard/focus/scroll event routing
  -> accessibility semantic projection
  -> render/Wayland/automation consumers
  -> dirty propagation, frame snapshot, recovery
```

## Performance findings

### S08-PERF-001 — Retained layout/style invalidation can broaden beyond the changed node

- **Source:** `elements/src/layout/mod.rs:243-300`, lowering/synchronization
  code, and style resolution callers.
- **Current behavior:** dirty synchronization and inherited/style changes can
  walk retained subtrees and rerun layout/style work for descendants even when
  the dependency is narrower.
- **Why it matters:** large panels and lists can turn a hover, token, or text
  change into frame-sized recomputation.
- **Recommended improvement:** track typed dependency/invalidation scopes and
  preserve last-known-good geometry for unaffected branches.
- **Measurement:** 100/1,000/10,000 nodes with leaf hover, inherited color,
  width, and text edits; measure nodes traversed, Taffy calls, allocations,
  frame gap, and damage.
- **Confidence:** medium hypothesis; **Status:** new measurement item.

### S08-PERF-002 — Text measurement cache keys and revisions need bounded-load evidence

- **Source:** `layout/mod.rs:30-220` and layout tests around
  `text_measure_revisions`.
- **Current behavior:** cache entries include shaping inputs/revisions, but
  retained layout may still perform repeated measurement on generation changes.
- **Why it matters:** text-heavy surfaces can spend a large fraction of frame
  time in shaping and cache misses.
- **Recommended improvement:** measure per-resource revision invalidation and
  use byte-bounded cache admission/eviction where beneficial.
- **Measurement:** 1/10/100k glyphs, 1/10 fonts, locale/theme/scale changes;
  record hit rate, bytes, shaping CPU, allocations, and p95 frame time.
- **Confidence:** medium. **Status:** hypothesis; do not repeat rejected cache
  changes without this workload.

### S08-PERF-003 — Semantic and layout snapshots duplicate tree traversal

- **Source:** `elements/src/frame.rs:562-733`, accessibility projection, and
  render/interaction callers.
- **Current behavior:** frame layout, semantic, and state comparisons each walk
  the retained tree and clone selected values.
- **Why it matters:** accessibility publication and repaint scheduling can add
  avoidable per-frame work on large trees.
- **Recommended improvement:** produce one generation-stamped frame index with
  node geometry/state/semantic dependencies and derive consumer deltas from it.
- **Measurement:** 100/1,000/10,000-node trees with no-op, leaf-state, and
  full-tree changes; count traversals/clones and measure p95 frame work.
- **Confidence:** medium hypothesis. **Status:** new.

## Dead code and redundancy

### S08-DEAD-001 — Element vocabulary is duplicated across contracts, types, and source tags

- **Source:** `elements/src` contract/type tables and compiler source-tag/event
  mappings.
- **Current behavior:** tag names, attributes, event names, pseudo-states, and
  runtime behavior are represented in multiple tables.
- **Why it matters:** a newly declared element can be accepted by one layer but
  not receive style, events, accessibility, or runtime behavior.
- **Recommended improvement:** generate/derive tables from one typed element
  contract and retain compatibility aliases only with explicit diagnostics.
- **Test:** generated parity over every shipped element, attribute, event,
  state, style hook, and source tag.
- **Confidence:** high duplication. **Status:** older audit/backlog overlap.

### S08-DEAD-002 — Multiple focus/semantic state representations overlap

- **Source:** `elements/src/events.rs`, `pseudo_state.rs`, `frame.rs`, and
  accessibility projection.
- **Current behavior:** interaction focus, accessibility focus, pseudo-state
  flags, and semantic fields each maintain related state.
- **Why it matters:** a state update can reach routing but not styling or the
  accessibility tree, leaving divergent observable facts.
- **Recommended improvement:** define one core state snapshot with explicit
  derived views; remove or isolate compatibility fields after consumer review.
- **Test:** focus/hover/disabled/checked/hidden transitions across style,
  events, semantics, automation, and render.
- **Confidence:** high parallel authority; **Status:** older audit.

## Logic and core mechanics

### S08-LOGIC-001 — Accessibility semantics are incomplete and not post-child normalized

- **Source:** compiler semantic construction, `elements/src/frame.rs:621-733`,
  render accessibility adapter, and element attributes.
- **Current behavior:** role/ARIA aliases, hidden/display filtering, descriptions,
  relationships, and descendant text naming are not uniformly normalized from
  the same post-child tree.
- **Why it matters:** controls can be unnamed or expose hidden nodes, violating
  accessibility and automation requirements.
- **Recommended improvement:** derive one semantic snapshot after children and
  visibility are known, with typed role/name/description/state precedence.
- **Test:** buttons with descendant text, icon-only controls, hidden/disabled
  descendants, ARIA aliases, locale changes, and AccessKit publication.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S08-LOGIC-002 — Pseudo-state declaration, mutation, and matching tables disagree

- **Source:** `elements/src/pseudo_state.rs:277-300`, style resolver state and
  matching modules, compiler annotation callers.
- **Current behavior:** some declared states are indexed but not matched or
  initialized by runtime annotation (`required`, `selected`, `pressed`, etc.).
- **Why it matters:** authored selectors silently do nothing and invalidation
  misses state-dependent styles.
- **Recommended improvement:** generate one typed state vocabulary and explicitly
  map applicability, mutation, matching, and invalidation per element.
- **Test:** full pseudo-state matrix through compiler, runtime events, computed
  style, and repaint.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S08-LOGIC-003 — Failed Taffy/layout updates must not advertise valid stale geometry

- **Source:** `elements/src/layout/mod.rs:121-300`, layout lowering and retained
  write-back.
- **Current behavior:** failure paths can retain prior/zero geometry while the
  tree's validity/dirty state is not a single explicit contract.
- **Why it matters:** hit testing, accessibility bounds, and rendering can use
  geometry that was never produced by the current generation.
- **Recommended improvement:** use a transaction result containing generation,
  valid/invalid status, and last-known-good geometry; block consumers from
  treating failed results as current.
- **Test:** invalid style/dimension, Taffy failure injection, recovery, and
  concurrent tree replacement.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S08-LOGIC-004 — Pointer event routing and capture need one state machine

- **Source:** `elements/src/events.rs:168-760`.
- **Current behavior:** pointer target selection, disabled checks, press/release,
  ancestry, and capture semantics are spread across helpers and can disagree
  with hit testing/transformed bounds.
- **Why it matters:** dragging, disabled controls, transformed nodes, and
  pointer release outside a node can activate the wrong handler.
- **Recommended improvement:** model pointer contact/capture/hover as explicit
  transitions over one frame snapshot and dispatch only after target validity.
- **Test:** transformed/ clipped nodes, disabled descendants, drag-out/release,
  multi-button, surface removal, and stale tree generation.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S08-LOGIC-005 — Custom-property inheritance/cycle handling is not a single cascade

- **Source:** style resolve value/matching modules and compiler inherited-style
  matching.
- **Current behavior:** explicit defaults, inherited values, per-node scratch
  maps, and variable recursion are handled in separate paths; cycles need a
  bounded guard.
- **Why it matters:** descendants can receive wrong tokens or recurse on a
  malformed stylesheet.
- **Recommended improvement:** carry an inherited typed environment through the
  style cascade, detect cycles/depth limits, and retain source diagnostics.
- **Test:** parent/descendant custom properties, fallback chains, cycles, deep
  nesting, and theme/component precedence.
- **Confidence:** high. **Status:** older audit/backlog overlap.

## Existing backlog or audit overlap

The older element audit covers accessibility projection, descendant naming,
pseudo-states, inherited matching, layout failure, pointer routing, vocabulary
drift, custom-property cycles, focus, text cache keys, and popovers. Those remain
overlap unless explicitly marked as fixed by current tests. New findings are
the requested workload measurements and the need for a shared frame index.

## Refuted suspicions

- Existing layout tests cover shaping inputs and measurement revisions; a blanket
  “text cache key is incomplete” claim is not repeated without a new failing
  case.
- No broad SmallVec/cache/traversal optimization from the rejected table is
  repeated; all performance suggestions specify fresh workload measurements.

## Tests and benchmarks needed

- Semantic/accessibility, pseudo-state, layout failure, event capture, focus,
  custom-property, identity, and element-contract parity matrices.
- Retained-tree benchmarks with node counts, edit shapes, shaping inputs,
  allocations, traversals, invalidated nodes, damaged pixels, and p95 frame gap.

## File coverage

**Assigned:** 51/51 files under `crates/core/ui/elements/`, its tests, and the
element/accessibility documentation assigned by the section map. **Inspected:**
51/51. Compiler/render/Wayland callers were searched but belong to Sections 10,
12, and 14. **Files still needing review:** none.
