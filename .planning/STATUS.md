# Status

**Updated:** 2026-08-27

## Now

Section 15 finding #10 (one sub-bullet): removed the internal, undeclared
`"set-current"` command the shell sent directly to any module implementing
`mesh.theme` as a backend — confirmed dead in the shipped module set, but a
real contract violation. The other two sub-findings (settings republish,
discarded CoreRequests) are untouched. See
[`.planning/log/2026-08.md`](log/2026-08.md).

Disk note: `target/` hit 69G and filled the sandbox disk mid-session; freed
with `rm -rf target`. Watch disk usage in long sessions like this.

Section 15 finding #7: a dead file-watcher thread now tells the shell loop
(`ShellMessage::FileWatcherStopped`) instead of silently vanishing, so
polling falls back immediately instead of staying parked for 24h. Watch
coverage itself (new directories from live graph/import changes) is still
static-at-startup and remains open. See
[`.planning/log/2026-08.md`](log/2026-08.md).

Section 15 finding #6: one component's service-event handler failure no
longer aborts delivery to every other component (and, unhandled above
`Shell::run`, the whole shell process). Diagnosed per-component and skipped
instead. `tick`/`render`/reload failure isolation is still open follow-up
within the same finding. See [`.planning/log/2026-08.md`](log/2026-08.md).

Section 15 finding #8: `drain_requests` now caps processing at 4096
requests per batch and diagnoses/drops the remainder instead of hanging on a
self-emitting request cycle. Backstop only, not the full bounded-scheduler
redesign the finding describes. Findings #3 (generation tagging) and #9
(provider-failure state delivery) were checked and are already fully shipped
by prior work — no action needed there. See
[`.planning/log/2026-08.md`](log/2026-08.md).

Section 15 finding #2: enabling a backend-kind module now actually spawns it
live (`apply_set_module_enabled` was frontend-only), reusing the existing
provider-switch staged-activation path. Disable side needed no fix — already
guarded. See [`.planning/log/2026-08.md`](log/2026-08.md).

Started Section 15 finding #1 (profile activation atomicity). Fixed one
concrete gap: `commit_pending_profile_switch` now restores the durable
`active-profile` pointer if `commit_resource_snapshot` fails after
`paths.set_active` already advanced it, instead of leaving disk pointed at a
candidate the running shell rejected. The rest of the finding (candidate
interface snapshot use, entrypoint-aware root retention, rollback of
theme/locale/settings refresh failures after commit) remains open — see
[`.planning/log/2026-08.md`](log/2026-08.md) for the full assessment against
current source, which has moved substantially since the 2026-08-20 audit.

The backlog's Section 14 umbrella item ("build the transactional,
capability-aware presentation engine...") is closed and deleted from
`docs/BACKLOG.md`: all 12 audit findings were verified already shipped in the
current tree. See [`.planning/log/2026-08.md`](log/2026-08.md) for the dated
entry and the finding-by-finding evidence. Working through
`docs/BACKLOG.md` top to bottom next; Section 15 (shell core and
orchestration) is up next.

`WaylandClipboard::write_text` now kills and reaps its spawned helper on a
failed stdin write instead of leaking it unreaped; the rest of the Section 14
finding 10 (SHM buffer validation, attach/region error propagation, and
connection-loss surfacing) was already shipped by earlier work. See
[`.planning/log/2026-08.md`](log/2026-08.md) for the dated entry.

The active development graph now grants the Hyprland backend's optional
`socat`/`nc` socket-stream capabilities, so workspace changes use the socket2
event path instead of the 500 ms polling fallback. The workspace showcase uses
one animated shared indicator with a translucent trailing layer beneath the
workspace numbers, and the backend preserves each rapid socket2 workspace
transition for the shell.

Theme/locale switching now avoids two kinds of redundant preparation. Theme
publication computes its token delta once and skips individual `TokenChanged`
validation/fan-out when the exact event has no observer, while retaining the
aggregate revision event and conservative fallback semantics. Locale-only
settings transactions reuse the active immutable catalog snapshot; graph and
profile activation still prepare replacements when catalog sources can change.

Theme switching works end to end. After the contract compiled, every
`mesh.theme` command was still denied: the contract-authorized path checked a
bare `theme.control` capability that no manifest, capability registry entry,
or spec line uses, while logging the `service.theme.control` name it had not
consulted. Theme control now takes the generic `service.<name>.control` path.
Two test gaps hid it — the dispatch test ran without an installed graph, and
the graph theme helper never wrote its `module.json` to disk, so
`apply_set_theme` silently no-opped in every test that used it.

Theme switching works again. `mesh.theme` declared `fingerprint` as `integer?`
where the contract vocabulary wants `int?`, the contract failed to compile, and
`register_contract` discarded the error, so the interface was simply absent
from the catalog and every theme method rejected as an unknown channel.
Built-in contracts now register through a helper that logs and debug-asserts
instead of dropping the failure.

Themes ship as discoverable modules under `modules/themes` and contribute
through `mesh.provides.themes`, so all seven reach the graph catalog and the
Appearance page offers a real choice instead of only the active theme. The
catalog resolves a bare local id (`nord`) as well as the scoped identity
(`@mesh/nord:nord`) it keys on, so readable settings values still select.

Development-shell startup is clean: no errors, no warnings, and 29 log lines
instead of 90. The Luau sandbox charged each interpreter checkpoint as a
thousand instructions, which capped a callback at roughly a thousand loop
iterations and killed `@mesh/upower-power` and `@mesh/hyprland-wm` inside
`start()`; all five backend providers now come up. The settings loader no
longer reports its own `revision` stamp as an unknown key, and per-item startup
enumeration moved to `debug`.

A node with a running animation now stays in the retained dirty roots on every
pass. The animation pass compared against a baseline captured inside the same
pass, so the second paint of a frame reported a still-moving node as clean and
held the retained generation still while the tree changed — handing the display
list one generation for two different trees. A 60-second run is now free of
warnings and errors.

Note for anyone reading test counts here: `cargo test -p mesh-core-shell` is
flaky, giving 28-32 failures across runs of an unchanged tree. Compare failure
sets, not counts.

Development-shell startup diagnostics are clean in the code path: shipped
frontend modules, locale imports, settings metadata, backend initialization,
optional command capability checks, and core-owned provider status reporting
were repaired. The remaining smoke-test limitations are environment-owned
(no Wayland compositor and a read-only IPC socket directory).

Live navigation surfaces now synchronously resolve and rasterize bundled icons
on the first frame, preserve structured component props, and route revisioned
service effects. Navigation and settings surfaces opt out of an unreliable
system icon-theme precedence path in favor of their prepared Material pack.
Wayland pointer and surface identity lookups recover against live proxies and
emit boundary diagnostics for dropped events; actual compositor input remains
unverified in this environment because no Wayland compositor is available.

Node opacity and blend mode now lower as explicit isolated compositing groups
around the complete retained node subtree, including gradients, images, text,
shadows, and descendants. Primitive colors remain unmodified until the group
is composited, and retained sparse selection keeps each group atomic.

Retained display-list generations now include ordered paint command node/kind
topology, and child z-index sorting keeps authored order for equal-z siblings.
Pure paint-order changes therefore advance the display-list generation even
when primitive entry signatures are unchanged.

Retained render-object dirty categories and display-list paint signatures now
derive from one shared paint-input contract covering material, controls, text,
icons, opacity, and custom-property variables. Tag-aware content hashing keeps
irrelevant attributes out of generic entries while checked controls, text
attributes and typography, icon axes, and variables invalidate both contracts.

Frontend paint lowering now carries all four border edges and corner radii
through retained display commands, including asymmetric rounded border rings.

Backend command and event ingress now uses bounded per-provider queues with
shared JSON byte/depth validation. Luau provider events, queued side effects,
child processes, callback output, and storage writes participate in explicit
event-count and aggregate runtime resource budgets.

Effective service read and control grants are now separate: control-only
proxies can issue contract methods without receiving provider state, and shell
service fan-out applies the same read boundary.

Backend stream callbacks now reconcile changed poll intervals immediately after
each typed or legacy stream callback, before the next event or poll wait.

Backend Luau host and sandbox setup now returns a structured `HostSetup` error
instead of panicking, allowing the backend lifecycle to publish load failure
and complete cleanup.

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

Retained display lists and shell damage now carry the same cumulative affine
transform and exact ancestor clip stack through rotated paint, effect overflow,
blur regions, descendant reuse, and interaction-facing geometry.

Retained display lists now publish an immutable typed frame paint plan carrying
paint inputs, command topology, transforms, effect regions, replay spans, and
logical/device damage conversion.

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

Decoded assets, fonts, glyphs, text layouts, Skia images and shaders, icon
resolutions, PixelBuffers, and Wayland SHM pools now enforce explicit byte and
dimension budgets with deterministic rejection or LRU eviction.

Author-declared `promotable` now gates settings, IPC, and live surface role
changes through one shared role-transition authorization policy; the
author-only capability is excluded from user schemas and config ejection.

Settings-driven role reloads now stage an authorized role change and route it
through the same transactional transition supervisor as explicit promotion,
so child surfaces, focus state, compositor objects, and cached surface state
are invalidated together.

Manifest surface enums now use one canonical parser set across graph
diagnostics, settings, and runtime layout resolution; invalid values and
role-specific field contradictions are reported before runtime policy is used.

Presentation surface change detection now compares the lowered layer namespace
(including blur intent) and xdg decoration negotiation, so either creation-time
change takes the compositor-role recreation path instead of being treated as a
no-op.

Role-field metadata now comes from one shared contract consumed by manifest
diagnostics, settings validation, configuration ejection, and presentation
protocol lowering. Layer-only and window-only fields therefore stay aligned,
including keyboard mode, margins, blur, and decorations.

Configuration ejection now preserves structured localized surface titles and
uses an explicit effective-policy serializer to materialize derived window
identity such as `app_id`; emitted values are therefore distinguishable as
pinned effective overrides rather than fallback-only or still-derived fields.

Surface settings, shell reload, and presentation now share one revisioned
`SurfacePolicySnapshot` semantic diff and accepted policy generation covering
blur, decorations, padding, geometry, keyboard mode, and role transitions.

Surface policy compilation now exposes the declared contract, effective
revisioned policy with provenance and diagnostics, and a typed transition plan
covering rejection, measurement, live updates, reconfiguration, recreation,
children, focus, input regions, and presentation readiness.

Semantic layer-surface diffs compare the size the protocol actually carries,
so a spanning axis resolving its real output size is a measurement change
rather than a reconfigure. A spanning bar therefore no longer clears
`configured` waiting for a configure event the compositor will not send, which
had frozen the shipped navigation bar a few frames after startup — hover
restyles, service-driven icon changes, and the clock all resolved but never
reached a buffer.

The `Testing` presentation backend's deterministic protocol-state simulator
(configure/popup failure injection, close/dismiss/connection-loss, frame
pacing, buffer backpressure, scale/output generations, text-input-v3) was
already complete from prior sessions. Added the missing half: a focused live
compositor matrix (`crates/core/presentation/tests/live_compositor_matrix.rs`)
that drives the real `WaylandSurfaceBackend` through configure/present,
destroy-then-present, and orphan-popup rejection against whatever compositor
is on `WAYLAND_DISPLAY`, skipping cleanly where none exists.

## Blocked / open follow-ups

- The new live compositor matrix has only been verified to compile and skip
  cleanly; no Wayland compositor (headless or otherwise) exists in this
  sandbox to run it for real.
- Live multi-output membership beyond what the matrix now covers remains
  simulator-covered only.
- Connection recreation after a Wayland connection loss (separate follow-up).
