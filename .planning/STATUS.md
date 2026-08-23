# Status

**Updated:** 2026-08-23

## Now

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
`xdg_popup.reposition` is now represented as a version gate. Late callbacks
from timed-out frames or replaced roles are ignored instead of releasing a
newer pacing gate; the current surface generation snapshot remains available to
diagnostics.

Icon bitmap/SVG and font-glyph preparation still share a bounded render resource
broker, with revision-aware cache handoff and linked-file invalidation. Typed
icon/glyph caches and shell polling remain separate at the handoff boundary.
Text shaping remains synchronous and outside that broker.

Focused presentation and shell tests plus `cargo check --workspace` pass.
Existing shell dead-code/private-interface warnings remain. The focused
real-surface navigation raster fixture still has the known 1280px layout
overflow. The broad workspace lib test run remains blocked by the pre-existing
repository-settings fixture, which rejects its `revision` key as unknown.

## Next

Complete the remaining semantic presentation state diff and consume negotiated
capability versions for popup identity/reposition safety. Pointer-button
identity, IME/text-input-v3, connection recreation, and resource
preparation/text shaping remain separate follow-up work.
