# Section 09 — Interaction and motion

## Scope and coverage

Reviewed all 18 assigned files under `crates/core/ui/interaction/` and
`crates/core/ui/animation/`, their tests, and keyboard/motion specifications.
Element, shell, frontend, rendering, Wayland, accessibility, and settings
callers were searched. **18/18 assigned files inspected; no follow-up remains.**

## Process tree

```text
raw Wayland/input or programmatic state
  -> frame/tree generation and visibility/disabled checks
  -> hit test, focus, capture, keyboard/scroll dispatch
  -> typed interaction state/events
  -> transition/keyframe timeline and reduced-motion policy
  -> style/layout/paint invalidation and presentation
  -> pause/resume/cancel/replacement/removal cleanup
```

## Performance findings

### S09-PERF-001 — Hit testing and event dispatch can traverse the tree repeatedly

- **Source:** `interaction/src` and element event helpers
  `crates/core/ui/elements/src/events.rs:576-760`.
- **Current behavior:** visibility, target, ancestry, transformed containment,
  focusability, and handler routing use separate traversals.
- **Why it matters:** pointer motion and keyboard navigation at interaction
  frequency can consume frame budget on large trees.
- **Recommended improvement:** build a generation-bound spatial/interaction
  index once per frame and reuse it for hit testing and dispatch.
- **Measurement:** 100/1,000/10,000 nodes, transformed/clipped trees, 60/240 Hz
  motion; measure traversals, allocations, p95 dispatch latency and frame gap.
- **Confidence:** medium hypothesis. **Status:** new; not a rejected traversal
  experiment without new evidence.

### S09-PERF-002 — Animation timelines repeatedly scan rules and allocate values

- **Source:** `animation/src`, transition/keyframe parsing and shell bridge.
- **Current behavior:** animation selection, easing, keyframe interpolation,
  and state updates are performed per timeline/node rather than from a compiled
  rule artifact.
- **Why it matters:** many simultaneous transitions can create per-frame parse,
  lookup, or allocation pressure.
- **Recommended improvement:** compile keyframes/easing once per style revision,
  store compact timeline state, and batch updates by frame generation.
- **Measurement:** 1/10/100/1,000 concurrent animations with 1/5/20 stops and
  varied duration/delay/easing; record allocations, CPU, missed frames, and
  cancellation cleanup.
- **Confidence:** medium. **Status:** new hypothesis.

### S09-PERF-003 — Reduced-motion and visibility changes may invalidate broad scopes

- **Source:** interaction policy and animation invalidation consumers.
- **Current behavior:** policy/visibility changes can cancel or rebuild many
  timelines even when only a subset is affected.
- **Why it matters:** accessibility preference changes should not cause an
  avoidable animation/layout storm.
- **Recommended improvement:** bind timelines to typed policy/resource revisions
  and cancel only dependent entries.
- **Measurement:** 100/1,000 nodes toggling reduced motion, hidden, or surface
  removal; measure cancelled timelines, style/layout work, damage, and frame gap.
- **Confidence:** medium. **Status:** new.

## Dead code and redundancy

### S09-DEAD-001 — Interaction policy is duplicated between core and shell

- **Source:** `interaction/src/policy.rs` and shell/component keybind/policy
  consumers.
- **Current behavior:** focus, activation, keyboard, and navigation policy is
  represented in both a reusable interaction package and shell handlers.
- **Why it matters:** a key may be reserved or routed differently depending on
  entry point, making accessibility and automation behavior inconsistent.
- **Recommended improvement:** keep generic state machines in interaction core;
  inject shell policy as typed configuration and delete wrappers after call-site
  migration.
- **Test:** repository-wide policy call graph and parity tests for mouse,
  keyboard, automation, and touch/pointer paths.
- **Confidence:** high duplication; **Status:** older audit/backlog overlap.

### S09-DEAD-002 — Transition and keyframe easing representations overlap

- **Source:** `animation/src` and component/theme style easing types.
- **Current behavior:** parser/lowering and runtime timeline code retain related
  easing/stop representations, with bridge conversions.
- **Why it matters:** per-keyframe easing or comma-list semantics can be lost at
  a conversion boundary.
- **Recommended improvement:** use one typed compiled timeline representation;
  keep CSS parser types at the source boundary only.
- **Test:** multi-entry transitions, distinct delays/durations/easings, keyframe
  defaults, and round-trip projection.
- **Confidence:** medium-high. **Status:** older audit/Section 07 overlap.

## Logic and core mechanics

### S09-LOGIC-001 — Interaction visibility must match rendering visibility

- **Source:** interaction hit-test visibility helpers and element/render style
  visibility state.
- **Current behavior:** interaction paths use their own hidden/display/clip
  checks rather than consuming the exact rendered frame snapshot.
- **Why it matters:** invisible or off-surface nodes can receive input, while
  visible transformed content may not be targetable.
- **Recommended improvement:** make hit testing consume the committed frame's
  visibility, transform, clip, and generation index.
- **Test:** `display:none`, opacity/visibility, clipping, popovers, surface
  replacement, and stale frame events.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S09-LOGIC-002 — Hit testing must use transformed paint bounds

- **Source:** element event transforms at `elements/src/events.rs:629-760`
  and animation transform consumers.
- **Current behavior:** transformed paint and pointer containment can use
  different coordinate/bounds logic.
- **Why it matters:** rotated/scaled/translated controls receive clicks outside
  their painted shape or miss painted targets.
- **Recommended improvement:** derive inverse hit transforms and clip bounds
  from the same affine paint state used by rendering.
- **Test:** rotation, scale, translation, nested transforms, clipping, fractional
  scale, and pointer boundaries.
- **Confidence:** high. **Status:** older audit.

### S09-LOGIC-003 — Disabled nodes and pointer capture need explicit transitions

- **Source:** `elements/src/events.rs` target checks and interaction policy.
- **Current behavior:** disabled filtering, press/release, drag-out, and capture
  are distributed across helpers, so release/removal can route to stale nodes.
- **Why it matters:** disabled controls may activate and drags can deliver to a
  removed/replaced component.
- **Recommended improvement:** model pointer contact/capture/release as a
  generation-aware state machine with cleanup on node/surface removal.
- **Test:** disabled target, drag outside, release after removal, multi-button,
  surface close, and stale generation.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S09-LOGIC-004 — Animation pause/resume and replacement must preserve timeline semantics

- **Source:** animation timeline state and keyframe transition bridge.
- **Current behavior:** pause/resume, visibility, and rule replacement can use
  wall-clock time or reinitialize progress without preserving an explicit
  timeline generation.
- **Why it matters:** paused animations can jump, reverse, or apply stale values
  after resume/replacement.
- **Recommended improvement:** store monotonic timeline time, playback state,
  rule fingerprint, and generation; define cancellation/restart behavior.
- **Test:** pause at multiple progress points, delay, repeat, reverse, rule edit,
  reduced motion, node removal, and provider/profile switch.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S09-LOGIC-005 — Reduced-motion ownership and enforcement are not one contract

- **Source:** interaction policy, animation config, settings/accessibility
  consumers, and shell bridge.
- **Current behavior:** preference declaration, animation reduction, and input
  policy are split; some callers can bypass the setting.
- **Why it matters:** users who require reduced motion may still receive
  transitions/keyframes or inconsistent focus movement.
- **Recommended improvement:** publish a typed motion policy revision and apply
  it at timeline creation and update, while preserving instantaneous final
  state and accessibility focus semantics.
- **Test:** policy on/off during active animation, profile reload, component
  override, and every animation type.
- **Confidence:** high. **Status:** older audit/backlog overlap.

## Existing backlog or audit overlap

The prior motion audit covers visibility/hit bounds, disabled/capture, pause,
reduced motion, focus paths, transition timing, shell policy, box-shadow parser,
and animation identity. Current CSS transition work has improved independent
comma-list timelines; that fix is not repeated. New findings are shared index
and compiled timeline measurements.

## Refuted suspicions

- The current transition implementation retains independent comma-list timing;
  “all entries share one duration/easing” is not a current finding.
- No rejected SmallVec/traversal/cache experiment is repeated without a new
  workload and measurement gate.

## Tests and benchmarks needed

- Visibility/transform/disabled/capture/focus, reduced-motion, timeline
  pause/resume/replacement, easing, cancellation, and stale-generation tests.
- Interaction and animation benchmarks with node/timeline counts, event rate,
  allocations, tree traversals, p95 dispatch and frame-gap measurements.

## File coverage

**Assigned:** 18/18 under `crates/core/ui/interaction/` and
`crates/core/ui/animation/`, with their tests and motion/keyboard contract
documents. **Inspected:** 18/18. Element/shell/Wayland consumers were searched
but belong to Sections 08, 14, and 15. **Files still needing review:** none.
