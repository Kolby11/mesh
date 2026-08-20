# Section 9 — Interaction and motion: improvement audit

**Audited:** 2026-08-19  
**Scope:** `mesh-core-interaction` and `mesh-core-animation`: hit testing,
focus and target selection, pointer/keyboard/scroll behavior, interaction
state, transforms, transitions, easing, interpolation, shadows, keyframes,
and the shell/render integration around them.

This is an audit record, not a second task tracker. Open work belongs in
[`docs/BACKLOG.md`](../../../../docs/BACKLOG.md). No production code was
changed for this review.

## Logical instruction/process tree

The section is not just two independent utilities. It is a frame-spanning
decision pipeline whose output must remain consistent with style, layout,
rendering, accessibility, and script-visible state.

```text
element tree + computed style + retained layout + motion preferences + clock
  │
  ├─ Wayland/shell input normalization
  │    └─ pointer, keyboard, scroll, focus, and surface events
  │
  ├─ interaction geometry and eligibility
  │    ├─ visibility/display/clip filtering
  │    ├─ transform-aware coordinate conversion
  │    ├─ disabled and inert target filtering
  │    └─ focusability, pointer, gesture, tooltip, and scroll ownership
  │
  ├─ target/state resolution
  │    ├─ focus traversal and activation target
  │    ├─ hover/active/pressed/focus-visible state
  │    ├─ pointer capture and press-origin identity
  │    ├─ scroll container/scrollbar target and offset updates
  │    └─ keyboard routing and default-action decisions
  │
  ├─ typed invalidation classification
  │    ├─ interaction/style-only update
  │    ├─ paint or layer-effect repaint
  │    ├─ layout-affecting change
  │    ├─ semantic/accessibility update
  │    └─ tree rebuild or surface-level update
  │
  ├─ style and layout transaction
  │    └─ refreshed computed values and retained geometry
  │
  ├─ animation sampling
  │    ├─ transition snapshot and reversal/cancellation policy
  │    ├─ keyframe time, iteration, direction, and pause/resume
  │    ├─ segment easing and interpolation
  │    ├─ reduced-motion policy
  │    └─ animation-driven invalidation
  │
  ├─ render-object/display-list update
  │    └─ transformed paint bounds, shadows, and visual values
  │
  └─ Wayland presentation
       └─ frame commit and next-frame scheduling

feedback loops:
  handlers mutate script/runtime state → tree/style rebuild → same pipeline
  animation clock → repaint/layout invalidation → next sample/presentation
  focus/semantic changes → accessibility publication and keyboard ownership
  scroll/inertia → geometry changes → hit testing and paint update
```

### Required invariants

1. One shared visibility predicate controls painting, hit testing, focus,
   scrolling, tooltips, and semantic exposure.
2. One shared geometry contract describes layout coordinates, transforms,
   clipping, and inverse hit-test conversion. A node cannot be painted in one
   place and targeted in another.
3. Disabled, hidden, and inert nodes have one target-eligibility policy across
   pointer, keyboard, gesture, tooltip, scroll, and accessibility paths.
4. Focus, pointer capture, press origin, gesture ownership, and scroll owner
   are updated transactionally and produce explicit downstream invalidation.
5. Animation identity is stable across style updates, and pause/resume does
   not change elapsed time unexpectedly. Motion preferences apply to every
   non-essential animation source, including smooth scrolling and inertia.
6. Each frame exposes a coherent ordering from input resolution through state,
   style/layout, animation sampling, paint, and presentation.

## Verification

- `nix develop -c cargo check -p mesh-core-interaction -p
  mesh-core-animation` passed.
- `git diff --check` passed.
- `nix develop -c cargo test -p mesh-core-interaction --lib` passed: 24
  passed, 12 ignored, 0 failed.
- `nix develop -c cargo test -p mesh-core-animation --lib` passed: 19 passed,
  0 failed.
- The requested Luna xhigh process mapper, logical/order reviewer, direct
  code-error reviewer, and focused interaction/accessibility seam reviewer
  were launched. The logical/order report returned with the process tree and
  source findings; the other workers were stopped after they did not return a
  usable report within the review window. No worker edited files. The findings
  below were checked against the local source and tests.

## Confirmed findings

### 1. P1 — Interaction visibility differs from rendering visibility

`mesh-core-interaction` considers `display: none`, zero geometry, and the
`hidden` attribute in `interaction/src/lib.rs:117-125`. The renderer also
excludes `visibility: hidden` and `visibility: collapse` in
`frontend/render/src/display_list/build.rs:570-580`. Point-based focus lookup
in `interaction/src/focus.rs:53-80` does not apply the hidden predicate before
descending into children.

A node can therefore be absent from the display list while still being found
by focus or interaction queries. The same policy risk applies to scrolling,
tooltips, and semantic exposure.

**Improve it:** define a shared visibility/eligibility helper or snapshot
consumed by interaction and render. Add hidden-ancestor, `visibility`, zero
geometry, and semantic-exposure parity tests.

### 2. P1 — Hit testing does not match transformed paint bounds

Interaction applies only translation in `interaction/src/lib.rs:91-103` and
explicitly does not invert scale or rotation. The renderer applies scale when
calculating painted bounds in `frontend/render/src/display_list/paint_node.rs:68-80`.

Scaled content can consequently be visibly under the pointer while its
unscaled layout rectangle is tested. This also affects scrollbar and tooltip
targets, focus geometry, and `scroll_into_view`.

**Improve it:** introduce one affine transform/clip contract, use inverse
transforms for hit testing, and add nested translation/scale/rotation parity
tests against the same painted tree.

### 3. P1 — Disabled nodes can still receive pointer handlers

`interaction/src/hit_test.rs:192-224` checks visibility while locating pointer
handlers but does not check disabled state. Press targeting in
`interaction/src/hit_test.rs:230-269` likewise accepts a `click` handler
without the disabled check. `node_is_disabled()` exists in
`interaction/src/lib.rs:127-135`, but is used for focusability rather than as
the central target gate.

**Improve it:** centralize target eligibility and apply it consistently to
pointer/click, keyboard activation, gestures, scrollbars, tooltips, focus,
and semantic exposure. Test disabled ancestors and `aria-disabled` controls.

### 4. P1 — Paused keyframe animations jump forward after resume

`animation/keyframes.rs:103-119` freezes sampling at `paused_at`. The shell
bridge in `shell/component/animation.rs:346-370` preserves the original
`started_at` but clears `paused_at` when resuming. Elapsed time then includes
the pause interval, so an animation may jump or finish immediately after
resume.

**Improve it:** either shift `started_at` by the pause duration or make
pause/resume a stateful operation on `ActiveKeyframeAnimation`. Add tests for
pause at each iteration, resume after a long gap, reverse direction, and
finished-versus-paused state.

### 5. P1 — Reduced-motion ownership is specified but not implemented

`docs/spec/09-accessibility.md:78-87` assigns reduced-motion handling to the
animation engine, but no common motion preference or policy was found in the
core crates. Transitions, keyframes, smooth scrolling, inertia, tooltip fades,
and surface entrance motion therefore have no shared accessibility behavior.

**Improve it:** create a `MotionPolicy` snapshot with reduced-motion and
essential/non-essential classifications. Pass it into animation and scroll
scheduling, clamp or remove non-essential motion, and test runtime preference
changes without leaving stale animation invalidations behind.

### 6. P2 — Focus has multiple mutation paths that bypass event semantics

The normal focus path in `shell/component/input/focus.rs:5-32` fires blur/focus
handlers. Cross-surface transfer and auto-focus paths around lines 146-215 and
244-275 directly assign or clear focus, which can bypass those handlers and
canonical eligibility checks. `normalized_focused_key` validates that a key
exists, but does not fully revalidate hidden, disabled, or focusable state.

**Improve it:** make focus ownership a single transaction with explicit
`FocusChanged` output, blur/focus handler ordering, cross-surface transfer
semantics, return-focus behavior, and validation against the shared target
eligibility predicate.

### 7. P2 — Visibility transitions become hidden at the beginning

`animation/transition.rs:223-225` makes `selective_from()` use the desired
visibility instead of the previous visibility. Interpolation at
`transition.rs:313-320` treats visibility as a discrete value. For a visible
→ hidden transition, both endpoints can therefore become hidden immediately,
so the element disappears at the start rather than at the transition end.

**Improve it:** preserve the previous discrete value in the transition
snapshot and implement explicit discrete-property timing for visibility and
similar properties.

### 8. P2 — Per-keyframe easing is dropped across the shell bridge

`animation/keyframes.rs:18-26` supports segment-local easing, but
`shell/component/animation.rs:413-429` lowers every render stop with
`easing: None`. A keyframe-local timing function consequently cannot affect
its following segment.

**Improve it:** preserve validated easing from component parsing through shell
lowering, or explicitly reject the syntax until the complete path exists. Add
multi-stop easing tests at the component and animation boundaries.

### 9. P2 — Interaction policy is split between the interaction crate and shell

The interaction crate primarily supplies tree queries, while focus ownership,
pointer capture, smooth scrolling, inertia, and invalidation are distributed
through `shell/component.rs:13-17` and
`shell/component/interaction_state.rs:287-381`. This makes policy drift likely:
different callers can use different visibility, focus, capture, and invalidation
rules.

**Improve it:** define a renderer-neutral `InteractionFrame` or state-machine
transaction in `mesh-core-interaction`; keep Wayland polling, script handler
execution, and shell surface ownership in the shell. Return typed decisions and
dirty outputs rather than having each caller infer them.

### 10. P2 — The public `box-shadow` parser can panic unconditionally

`animation/box_shadow.rs:72-76` exports `parse_box_shadow` but still contains
`unimplemented!()`. `BoxShadow` is already part of `AnimatableStyle` and
transition interpolation, so malformed or merely valid public input can reach
a panic boundary.

**Improve it:** implement the parser with structured errors and coverage for
multiple shadows, or make the function private and reject the unsupported
syntax before it reaches the public animation API.

### 11. P2 — Animation identity is too weak for concurrent or changing rules

The shell key in `shell/component/animation.rs:329` is derived from
`node-key::animation-name`. Two instances with the same name or a changed
duration/keyframe definition can collide with the existing animation and
inherit stale timing/state.

**Improve it:** use a stable animation instance ID derived from node identity,
declaration generation, and list position; define explicit replacement,
cancellation, and reversal semantics for rapid hover/focus/style changes.

## Better feature direction

The logic review found that the current code flow is not the best long-term
boundary. A stronger design would make the following frame contract explicit:

```text
InteractionFrame {
  input_revision,
  tree_revision,
  visibility_and_geometry_snapshot,
  focus_owner,
  pointer_capture_owner,
  gesture_and_scroll_owners,
  resolved_targets,
  state_changes,
  semantic_changes,
  invalidation_set,
}
```

Animation would consume the same tree/style revision plus a clock and
`MotionPolicy`, then return visual samples and typed invalidation. A phase stamp
such as `InputResolved → StateUpdated → StyleInvalidated → LayoutReady →
AnimationSampled → PaintReady` would make ordering observable in diagnostics and
tests. This would prevent render, interaction, and accessibility from silently
using different snapshots while retaining shell ownership of Wayland polling
and Luau handler execution.

## Recommended implementation order

1. Centralize visibility, disabled/inert eligibility, and target filtering;
   add interaction/render/accessibility parity tests.
2. Define the shared transformed geometry contract and fix scale/rotation hit
   testing, scrolling, tooltips, and focus geometry.
3. Consolidate focus, pointer capture, press origin, gesture ownership, and
   scroll ownership into one interaction transaction with typed invalidation.
4. Fix keyframe pause/resume, strengthen animation instance identity, and
   define cancellation/reversal behavior.
5. Add `MotionPolicy` and apply it to keyframes, transitions, smooth scrolling,
   inertia, and other non-essential motion.
6. Correct discrete visibility transitions and preserve per-keyframe easing.
7. Implement or gate the public box-shadow parser, then add the regression
   matrix below.

## Regression matrix

- Hidden ancestors, `visibility: hidden/collapse`, zero geometry, and semantic
  exposure produce the same result in paint, hit testing, focus, scroll, and
  tooltip queries.
- Translation, scale, rotation, nested transforms, clipping, and transformed
  scrollbars use identical painted and interactive bounds.
- Disabled and `aria-disabled` nodes cannot be pointer-activated or keyboard
  activated, including disabled ancestors and pointer capture transitions.
- Press, drag, release, removal, and re-entry preserve press-origin and capture
  semantics; keyboard events use the same canonical focus owner.
- Cross-surface focus transfer, auto-focus, blur/focus ordering, return focus,
  and focus invalidation all use one path.
- Pause/resume across iteration boundaries preserves progress; cancellation,
  reversal, replacement, and duplicate animation names have deterministic
  results.
- Reduced-motion changes affect existing and newly scheduled transitions,
  keyframes, smooth scroll, inertia, tooltip, and surface motion.
- Visibility transitions remain visible until their discrete end point, and
  every keyframe segment honors its declared easing.
- Box-shadow parsing returns structured errors rather than panicking and
  interpolates supported multi-shadow values consistently.
