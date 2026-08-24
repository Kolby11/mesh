# Status

**Updated:** 2026-08-24

## Now

Root and nested template expressions now share parser-derived scope validation.
Root expressions, keyed-loop keys, runtime props, and expression-body source
spans are checked before compilation.

Contribution roots now share primary-root interface validation: imports must be
declared by the contributing module, explicit ranges are checked against the
resolved graph contract/provider, and invalid contribution entries receive
scoped diagnostics without entering the host catalog.

Focus, pointer capture, press origin, gesture ownership, and scroll ownership
now publish through one staged `mesh-core-interaction` transaction. The shell
keeps key-based lookup caches for script/render compatibility, while ownership
changes, eligibility reconciliation, typed decisions, categorized dirty-node
output, and speculative rollback share one commit boundary.

Keyframe playback now preserves active progress through pause/resume, including
delay, direction, iteration boundaries, and finite completion state.

Activation revalidates live disabled/inert eligibility for descendants,
captured pointer targets, focused keyboard targets, and synthesized activation
routes before dispatching handlers.

The shared interaction/rendering policy carries visibility, disabled/inert
eligibility, target filtering, and one affine transform/clip contract through
hit testing, focus, scrolling, tooltips, events, paint bounds, and downstream
consumers.

Reduced-motion preferences now flow through one `MotionPolicy` snapshot that
clamps non-essential transitions, keyframes, scrolling, inertia, tooltips, and
surface motion at settings and frame boundaries.

CSS custom-property resolution, retained explicit/inherited style masks, and
the canonical generated element schema landed 2026-08-23 — see
[`.planning/log/2026-08.md`](log/2026-08.md) for the dated entries.

Visibility transition playback now starts from the previously displayed
discrete value and applies the visibility endpoint at the end of an exit.

Validated per-keyframe easing now flows from component and theme keyframes
through shell animation state into the core and tooltip samplers.

The public animation `box-shadow` parser now returns structured errors for
malformed values and rejects comma-separated shadow lists before construction.

Animation instances now use retained node identity, animation-list position,
and declaration generation for stable identity. The shell and core expose
explicit started, continued, replaced, cancelled, completed, and reversed
decisions so style changes do not inherit stale timelines.

`InteractionFrame` now carries the renderer-neutral interaction state,
revision, typed decisions, dirty outputs, immutable tree snapshot, and ordered
phase stamps from input/state through style invalidation, layout, animation,
paint, and semantics.

Frontend entrypoints and recursive component imports now resolve through the
canonical module-root path boundary. Absolute and escaping traversal paths are
rejected, symlinked source files and directories are rejected, and accepted
sources retain canonical identities for compilation and dependency tracking.

Service payloads remain in capability-filtered, per-context Rust-owned
snapshots. Interface proxies may resolve event-only contracts, but the shell
applies state payloads only with the resolved read policy and delivers event
payloads through the separate event policy.

Frontend source reloads now compile each module's primary root and active
contribution roots into one candidate catalog generation. The candidate is
published atomically, contribution paths remain watched, and a failed root
compilation leaves the last-known-good catalog in place.

Primary and contribution roots now share one reverse dependency graph for
catalog invalidation, so a dependency used only by a contribution reaches the
host surface that renders it.

## Next

Scope local component aliases by owner and canonical source, rejecting
collisions (`docs/BACKLOG.md`).

## Blocked / open follow-ups

- Live multi-output membership and compositor conformance matrix (deferred,
  simulator-covered only).
- Connection recreation after a Wayland connection loss (separate follow-up).
