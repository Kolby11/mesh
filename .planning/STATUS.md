# Status

**Updated:** 2026-08-25

## Now

Backend command and event ingress now uses bounded per-provider queues with
shared JSON byte/depth validation. Luau provider events, queued side effects,
child processes, callback output, and storage writes participate in explicit
event-count and aggregate runtime resource budgets.

Effective service read and control grants are now separate: control-only
proxies can issue contract methods without receiving provider state, and shell
service fan-out applies the same read boundary.

Backend stream callbacks now reconcile changed poll intervals immediately after
each typed or legacy stream callback, before the next event or poll wait.

Backend source loading now runs in a startup-staged host phase: mutating
service, process, event, logging, and durable-storage handles reject calls
until the explicit `start(self)` lifecycle entrypoint begins.

Each backend runtime generation now preserves one Lua `self` table together
with its storage proxy and provider-owned event handles across lifecycle
callbacks. Provider interface event channels remain subscription-only for
consumers, while backend event publication is available through declared
provider-owned `self.EventName:fire(...)` handles. Subscriber callback failures
are isolated per callback, and host delivery continues through the same
failure boundary.

A revisioned `FrontendFrame` boundary now publishes immutable tree,
catalog/runtime/service revisions, invalidation, diagnostics, paint metadata,
and effects together after frontend paint.

Compiled frontend roots now publish immutable content revisions, and the normal
typed-effect adapter path rejects effects whose catalog/runtime revisions are
missing or stale. Live script event dispatch supplies the current pair before
lowering effects into shell requests.

Typed diagnostic categories and byte source spans now survive component AST,
frontend compiler, shell catalog/runtime, LSP, and debug serialization paths.
Runtime interface and storage failures retain distinct categories, and
compiler diagnostics retain their owning source file and span.

Frontend local component aliases now remain scoped to their canonical owner
source and target identity. Compiler output no longer publishes an unscoped
alias index; same-owner collisions are rejected using typed canonical import
targets, while shell/catalog lookup paths use the scoped records.

Component script imports now keep local-component and frontend-module aliases in
one typed namespace while parsing. Cross-kind collisions are rejected with an
import diagnostic before template lowering, including explicit imports mixed
with `require()` bindings.

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
snapshots. Each accepted shell snapshot carries provider identity and a
monotonic generation into frontend contexts, where stale snapshots are
discarded before proxy reads or reactive state updates. Interface proxies may
resolve event-only contracts, but the shell applies state payloads only with
the resolved read policy and delivers event payloads through the separate
event policy.

Frontend source reloads now compile each module's primary root and active
contribution roots into one candidate catalog generation. The candidate is
published atomically, contribution paths remain watched, and a failed root
compilation leaves the last-known-good catalog in place.

Primary and contribution roots now share one reverse dependency graph for
catalog invalidation, so a dependency used only by a contribution reaches the
host surface that renders it.

Shared full_moon metadata now drives Luau symbol discovery for component
compiler/editor consumers, frontend event-subscription graph diagnostics, and
LSP backend shape completion. Line-oriented symbol inference is no longer a
source of truth.

## Next

Return recoverable host-installation errors instead of panicking during backend
setup (`docs/BACKLOG.md`).

## Blocked / open follow-ups

- Live multi-output membership and compositor conformance matrix (deferred,
  simulator-covered only).
- Connection recreation after a Wayland connection loss (separate follow-up).
