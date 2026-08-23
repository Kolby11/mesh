# Status

**Updated:** 2026-08-23

## Now

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

Icon bitmap/SVG and font-glyph preparation still share a bounded render resource
broker, with revision-aware cache handoff and linked-file invalidation. Typed
icon/glyph caches and shell polling remain separate at the handoff boundary.
Text shaping remains synchronous and outside that broker.

Focused presentation, Wayland routing, and child-surface tests plus
`cargo check --workspace` pass. The focused popover group still has one
pre-existing legacy dismissed-popover visibility failure. Existing shell
dead-code/private-interface warnings remain. The focused real-surface
navigation raster fixture still has the known 1280px layout overflow. The
broad workspace lib test run remains blocked by the pre-existing
repository-settings fixture, which rejects its `revision` key as unknown.
The complete elements library suite also retains eight existing theme/style
expectation failures unrelated to pointer input.

## Next

Complete the remaining semantic presentation state diff and actual
`zwp_text_input_v3` object lifecycle/event wiring for preedit, commit,
surrounding text, and deletion; connection recreation and resource
preparation/text shaping remain separate follow-up work.
