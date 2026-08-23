# Status

**Updated:** 2026-08-23

## Now

The component parser now validates a strict top-level `.mesh` block sequence,
retains ordered byte-span metadata and attributes, requires `<template>`,
rejects duplicates/unknown content/unsupported script languages, and reports
inline `<i18n>` as an explicit migration error. Its brace-aware template lexer
and parser validates Luau expressions and control-flow nesting, rejects empty,
unterminated, malformed, and mismatched braces, and preserves absolute spans
for expressions, attributes, conditions, loops, and control-flow blocks.
The post-parse semantic pass links `<props>` declarations to exact and embedded
`prop()` style references and common CSS value domains, while local child
component inputs are checked for public visibility and static types. Undefined,
empty, or incompatible style props and unknown, private, or invalid child props
fail before lowering; LSP retains a tooling parse path for incomplete refs.
Component AST blocks and template nodes now retain absolute byte spans, parser
errors expose those spans, compiler validation errors preserve the offending
node range, and CLI/LSP diagnostics render the same source ownership (including
UTF-16 LSP positions). Focused component, frontend, and LSP diagnostics tests
plus workspace checking pass. The broad LSP suite retains its existing
locale-completion expectation failure.

The deterministic presentation backend now models compositor preferred-scale
updates for live surfaces: valid 0.5x..4x changes are observable through the
same scale/full-redraw seam as Wayland, while missing and destroyed surfaces
reject or clear stale scale state. This covers the simulator's scaling phase;
it now also retains multi-output membership, removes only the output named by an
unordered leave/destroy event, keeps the most recent surviving output as the
geometry choice, and resets output state when the surface disappears. Live
multi-output membership and compositor conformance remain open.

The deterministic popup lifecycle simulator now enforces the live identity
contract: unsupported promotion, regular-surface id collisions, missing or
nested parents, and reparenting are rejected before retained popup state is
mutated. Parent-close simulation now dismisses popup descendants before
publishing the parent's `Closed` event. Live compositor conformance and
connection recreation remain open.

Visible unknown testing targets now return `SurfaceMissing` for both pixel
presentation and state-only commits, matching the live backend and preserving
shell retry semantics instead of recording delivery against a nonexistent
surface.

Presentation now keeps frame-callback pacing separate from SHM buffer-release
backpressure. When all reusable buffers are compositor-owned, the live backend
retains an explicit release gate, refreshes it from dispatched
`wl_buffer.release` events, and the shell waits for that event instead of
retrying on the frame-callback timeout. The deterministic backend and shell
tests exercise the two gates independently.

Presentation now resolves dynamic layer-shell zero dimensions against the
surface's actual output before buffer attach and surface-state preparation.
The resolved logical extent is shared by paint-facing queries, input regions,
window geometry, viewport destination, and commit state, so a spanning bar or
rail cannot be painted at output size and presented through a one-pixel
logical destination.

Presentation surface configuration now computes a role-aware semantic diff:
layer surfaces ignore inert toplevel fields, windows ignore inert layer
placement fields, and live window size/title/identity, input padding, and
effective keyboard changes remain explicit. The presentation seam no longer
reconfigures a compositor object for fields its active role cannot consume.

Display paint now carries parsed `font-style` into the text renderer. Cosmic
text shaping, layout caching, ellipsis truncation, selection geometry, and
editable-input rendering use the same normal/italic style, and the style is
also part of text batch compatibility.

The deterministic presentation backend now clears its retained text-input
surrounding-text snapshot whenever the owning surface, popup, parent tree, or
connection is torn down, while preserving state published for other surfaces.
Its lifecycle simulation now matches the real Wayland teardown boundary.

The deterministic backend now also simulates one-seat text-input-v3 lifecycle:
enter accepts only live surfaces, preedit/commit/deletion payloads remain
pending until `done`, and one atomic `TextInputEdit` is published at that
boundary. Leave and object teardown cancel pending protocol state, while
published surrounding-text state remains valid through leave and clears with
the owning object.

Wayland keyboard commits now preserve the compositor's complete Unicode text
payload through presentation, shell routing, keyboard repeat, and the
component input boundary. Focused inputs apply one accepted multi-scalar
commit and dispatch one change boundary; the developer backend's existing
single-character path remains compatible.
The component input boundary now also carries UTF-8 byte-range deletion;
focused inputs retain a scalar-safe byte cursor, support surrounding deletion
and keyboard Delete, and dispatch one change boundary for each effective edit.

Wayland presentation now classifies compositor I/O/EOF failures as typed
`ConnectionLost` errors. The first loss clears retained compositor surface
identities, protocol-owned input handles, focus/repeat/touch/gesture ownership,
and pending stale input before publishing deterministic per-surface `Lost`
lifecycle events. The shell drains those events before returning the typed
failure, invalidating accepted configuration while keeping components mounted;
the testing backend injects the same one-shot lifecycle.

Wayland presentation also validates BGRA source/canvas lengths before copying,
returns typed copy and SHM attach failures, and restores the selected buffer's
copied pending damage when a copy or attach transaction fails. Visible presents
for missing compositor surfaces return `SurfaceMissing`; the shell retains the
damage, clears the accepted-config cache, and retries surface creation instead
of acknowledging an unshown frame. Surface configuration now returns typed
creation failures, prepares a replacement role before destroying the last-good
role, and updates shell config caches only after acceptance.
Compositor-closed layers and dismissed popups now emit typed lifecycle events;
their auxiliary protocol objects and identity mappings go through one
idempotent teardown helper, and the shell invalidates/recreates closed targets.
Input protocol handles and ownership now live in per-seat state: pointer,
keyboard, touch, gesture, and focus-grab callbacks resolve their originating
seat, repeat processing runs independently, and queued events retain internal
seat ownership until the backend boundary. Surface teardown, seat capability
removal, and full seat removal cancel only the affected seat's ownership while
preserving other seats targeting the same surface.
Normalized Wayland Escape (`Esc`) is also classified as non-repeating, closing
the key-name mismatch between event normalization and repeat suppression.
Surface configuration now uses a typed semantic diff instead of a partial
fingerprint: live changes, layer geometry that needs a fresh configure, and
creation-time identity changes that require replacement are distinct. Layer
namespace/blur and window decoration changes now take the safe replacement
path rather than being silently applied to the wrong live role.
Opaque, blur, input-region, and window-geometry updates now share a pending
surface-state transaction. Pixel-free render passes issue a state-only
`wl_surface.commit`, while buffer attach failures leave those updates pending;
shell region caches are invalidated whenever a configure can replace or remap
the compositor object.

Live presentation entries now carry monotonic object, configure, frame, buffer,
and output generations. Role creation reserves a unique object generation,
accepted compositor configures advance the per-object configure generation,
each SHM slot receives a non-reused buffer identity, and each
`wl_surface.frame` request carries an exact object/frame/buffer token. Output
enter/leave, output geometry updates, and output destruction advance the output
generation, invalidate output-dependent presentation state, and force a full
redraw; stale leave/destroy notifications cannot clear a newer membership.
The Wayland connection also publishes a typed, connection-local negotiated
capability snapshot with clamped protocol versions and an explicit generation;
`xdg_popup.reposition` is now represented as a version gate. Popup updates
reject non-popup id collisions, reparenting, and parent object replacement,
gate reposition requests on xdg-shell v3, issue nonzero reposition tokens, and
correlate returned configure tokens. Newly created v3 popups use reactive
positioners and retain the compositor-resolved popup position; older xdg-shell
versions still allow initial popup creation, while unsupported reposition
requests return typed diagnostics. Late callbacks from timed-out frames or
replaced roles are ignored instead of releasing a newer pacing gate; the
current surface generation snapshot remains available for diagnostics.

Click-grab popups now carry the exact Wayland seat protocol identity and
button-press serial from input normalization through shell routing and popup
creation. The shell stores that credential only for the current dispatch
generation, consumes it once for initial popup creation, and strips it from the
accepted placement cache so reposition/resize passes cannot replay a stale
serial. The presentation backend resolves the requested seat directly and
emits a warning when a requested grab has no live matching seat/serial, instead
of silently selecting the global activation-seat hint.

Wayland pointer presses and releases now retain their Linux button code through
presentation, shell routing, and component input. Primary-button activation and
click-grab authorization remain explicit; secondary and other buttons cannot
silently activate MESH controls, and generated click events expose the button
code.

Wayland presentation now binds one `zwp_text_input_v3` object per seat when the
compositor advertises the protocol. Enter/leave lifecycle is tracked per
surface, preedit/commit/deletion events are buffered through each compositor
`done` boundary, and the shell publishes a validated, UTF-8 byte-indexed
surrounding-text snapshot for the focused input. The resulting transaction
reaches the component boundary atomically, preserving one commit/delete change
boundary. Components retain preedit separately from committed input state and
project it inline at the UTF-8 cursor for rendering without firing a change
handler. The retained display list carries validated UTF-8 preedit ranges and
the painter draws the composition underline and caret at the compositor's
preedit cursor without decorating masked password input. Surface teardown now
clears the entered text-input surface, pending `done` transaction, and applied
surrounding state before the compositor identity disappears; the deterministic
testing backend drops queued input for destroyed surfaces while preserving
other seats/surfaces. The deterministic backend also validates and retains the
published surrounding-text snapshot, including explicit clear transitions, so
shell-side text-input publication is observable without a live compositor.

Popup targets now invalidate their cached creation size when presentation
reports `SurfaceMissing`, including state-only commits, and force a full retry
when the missing result came from surface-state submission. Child popup
reconciliation treats that invalidated gate as configure-needed even when the
placement is unchanged, while retained child paint state cannot suppress the
retry present.

Successful parent reconfiguration now invalidates child compositor caches as
well. If a parent role replacement destroys its popup or window descendants,
the next reconciliation recreates those child objects and forces a retained
content present instead of trusting the old child target cache.

Rejected child `configure_popup`, including a compositor-rejected
reposition, now destroys the stale popup role and requests a paint retry. The
next normal child reconciliation recreates the popup with the requested
placement while preserving its normal entrance transition.

Resource discovery, selected icon/font-pack parsing, and asset preparation now
run on cancellable workers. Immutable glyph maps and font bytes cross the
resource boundary as prepared handles; semantic icon lookup and icon/glyph
raster work use revision-aware bounded broker queues with deterministic missing
placeholders. The committed resource candidate now also carries a serializable
effective explanation with host/module provenance, pack chains, asset handles,
fallback attempts, and structured diagnostics. Runtime debug output, CLI
resource views and config doctor, and LSP resource completion consume that same
model; LSP deliberately reports discovered metadata without claiming prepared
render assets. Text shaping remains synchronous and outside that broker.

Focused presentation, Wayland routing, shell text-transaction, and preedit
rendering tests plus `cargo check --workspace`, formatting, and `git diff
--check` pass. The full
presentation library passes (95 active, 11 ignored). The broad shell library
run compiles and reaches 667 passed tests, but retains 47 existing fixture and
runtime failures and 130 ignored tests; none are text-input failures. Existing
shell dead-code/private-interface warnings remain. Connection recreation and a
live compositor/lifecycle matrix remain separate follow-up work.

## Next

The next open backlog item is in the UI element core audit: use one stateful
input dispatcher with pointer capture, press-origin identity, activation
semantics, focus eligibility, and invalidation output; the remaining
presentation/resource follow-ups stay separate.
