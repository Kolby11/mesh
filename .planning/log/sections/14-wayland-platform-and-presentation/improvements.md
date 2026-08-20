# Section 14 — Wayland platform and presentation audit

**Audited:** 2026-08-20  
**Packages:** `mesh-core-wayland`, `mesh-core-presentation`  
**Scope:** platform-neutral surface/input contracts, layer-shell and xdg roles,
popup promotion, configure/ack lifecycle, SHM buffers, damage and regions,
scaling, input/focus events, frame scheduling, and the shell/presentation seam.
No production code was changed.

Four Luna xhigh passes were used: the requested whole-process instruction-tree
pass, independent logic/order and direct code-error passes, and an additional
Wayland protocol/lifecycle and test-fidelity review. The findings below were
checked against the source, the locally installed SCTK/protocol definitions,
and the focused presentation suite. SCTK already acknowledges layer, window,
and popup configure serials before invoking MESH's handlers; configure/ack
ordering itself is not a finding.

## Logical process tree

```text
module/profile settings + CSS/component measurement + render damage
  -> shell surface intent
       role, placement, content size, padded paint extent, keyboard policy,
       window identity, visibility, opaque/blur regions, popup placement
  -> PresentationEngine backend selection
       Wayland layer/xdg backend | development window | testing backend
  -> configure top-level surface
       -> clamp layer intent against the surface's known output
       -> compare the last presentation fingerprint
       -> create or replace the role object
            layer: wl_surface -> zwlr_layer_surface_v1 -> initial commit
            window: wl_surface -> xdg_toplevel -> initial commit
       -> bind optional fractional-scale + viewport objects
       -> apply role-specific requests and wait for configure
  -> configure promoted popup
       -> validate/resolve layer or xdg-window parent
       -> build xdg_positioner
       -> create xdg_popup, optionally request a click grab
       -> initial commit and wait for configure
       -> later placement changes use xdg_popup.reposition
  -> compositor configure callback
       -> SCTK acknowledges serial
       -> adopt layer/window/popup size and window states
       -> mark the entry configured and request a full redraw where needed
  -> paint and present
       -> logical damage -> physical coverage
       -> select or allocate a released SHM buffer
       -> copy that buffer's accumulated dirty regions
       -> stage blur, input, opaque, and window-geometry state
       -> damage_buffer + buffer scale/viewport + attach
       -> request frame callback + wl_surface.commit
  -> compositor feedback
       -> frame callback releases the pacing gate
       -> wl_buffer release makes an SHM slot reusable
       -> output enter/leave updates output association
       -> pointer/keyboard/touch/gesture events route by wl_surface identity
       -> popup_done / window close / layer closed enter lifecycle handling
  -> shell scheduling
       -> Presented consumes damage
       -> NotReady retains damage and retries
       -> finish_frame flushes one batched Wayland frame
       -> wait on Wayland fd + shell eventfd
```

The boundary should enforce these invariants:

1. A configuration is acknowledged to the shell only after presentation has
   accepted a valid create/reconfigure/recreate plan; a failed replacement
   leaves the last-known-good surface intact or reports a terminal lifecycle
   failure.
2. A visible present to an absent surface is never reported as delivered.
3. Every creation-time field has an explicit recreate/reject rule, and every
   live field has one authoritative semantic diff.
4. No buffer is attached before the initial configure, and configure, popup
   reposition, frame callback, buffer release, and object generations cannot be
   confused with older operations.
5. Content size, padded logical extent, physical buffer extent, damage, input
   region, opaque region, blur region, and window geometry belong to one
   validated frame snapshot.
6. Destroy, compositor close, popup dismissal, hide/recreate, and role changes
   all run the same idempotent cleanup and notify the shell.
7. Optional protocol behavior is capability- and version-gated; availability
   of `xdg_wm_base` alone does not imply popup reposition support.
8. Click-grab popup behavior either carries the triggering seat/serial and
   succeeds or produces a visible downgrade diagnostic.

## Severity-ranked findings

### 1. P1 — Failed configuration is cached as success and later missing presents
discard the frame

`WaylandSurfaceBackend::configure` can destroy the old role before creating its
replacement, then log and return when creation fails
(`crates/core/presentation/src/wayland_surface/backend/surfaces.rs:130-159`).
Neither it nor `PresentationEngine::configure` returns a result
(`crates/core/presentation/src/lib.rs:232-243`), so the shell unconditionally
stores the failed config as `last_surface_config`
(`crates/core/shell/src/shell/runtime/render/mod.rs:360-361`). A visible present
to that absent surface then returns `PresentStatus::Presented`
(`backend/present.rs:59-61`), and the shell consumes the damage as if it reached
the compositor (`shell/runtime/render/mod.rs:981-988`).

**Failure:** A startup window, settings-driven role change, failed replacement,
or compositor-closed layer can become permanently blank: the old object is gone,
the failed intent is cached, and no retry or health event is produced.

**Improvement:** Make configure return a typed result and prepare the replacement
before committing the role transition. Cache only an accepted generation.
Return a distinct `SurfaceMissing`/`SurfaceLost` outcome for a visible present,
retain its damage, and publish a lifecycle diagnostic. Test failed window
creation, failed role replacement, and present-after-removal.

### 2. P1 — Compositor close/dismiss paths bypass lifecycle notification and
complete teardown

Layer `closed` removes only the backend entry
(`wayland_surface/handlers.rs:134-144`), without destroying popup descendants,
releasing optional per-surface objects, or notifying the shell. Popup `done`
does the same before queuing its id (`handlers.rs:771-777`). Explicit destroy,
by contrast, releases viewport, fractional-scale, and blur objects and destroys
popup children (`backend/surfaces.rs:451-503`). `State::remove_surface` itself
only removes the two identity maps (`wayland_surface/state.rs:323-337`).

**Failure:** The shell can keep warm config/region state for a compositor object
that no longer exists; auxiliary protocol objects and stale input ownership can
survive the abbreviated path, and a visible closed layer may never be recreated.

**Improvement:** Route explicit destroy, role replacement, layer `closed`, popup
`done`, parent destruction, and connection loss through one idempotent teardown
supervisor. It should destroy descendants, cancel input/focus/repeat state,
release auxiliary objects, remove indexes, and emit a typed lifecycle event.

### 3. P1 — Creation-time config fields and post-recreation region state can
diverge from the live compositor object

`surface_config_fingerprint` omits `blur`, `namespace`, and
`window.decorations` (`backend/config.rs:192-216`). Namespace/blur identity is
applied only when a layer object is created (`backend/surfaces.rs:241-249`), and
window decoration negotiation is also creation-time (`:222-225`). The shell
does notice these config changes, but presentation can classify them as a no-op.

Separately, the shell's opaque/blur cache keys only on display-list generation,
surface size, and content size (`shell/types.rs:28-49`). Role transitions and
window hide/recreate clear config and geometry but not `last_region_state`
(`shell/runtime/request.rs:1444-1471`, `:1800-1809`), so the new object may
never receive otherwise unchanged opaque or blur regions
(`shell/runtime/render/mod.rs:795-826`).

**Improvement:** Replace the incomplete hash with a typed semantic diff such as
`LiveUpdate`, `Configure`, `Recreate`, or `Reject`. Give each presentation
object a generation and key all applied compositor state on it, or move the
region caches into presentation so recreation makes them dirty mechanically.
This confirms the Section 13 config-diff finding at its protocol enforcement
point.

### 4. P1 — Documented click-grab popups always open without the compositor grab

Both popup construction paths set `grab_serial: None`
(`shell/runtime/render/child.rs:218-239` and
`shell/runtime/request.rs:740-755`). The backend calls `xdg_popup.grab` only
when both a seat and serial are present, otherwise it opens without a grab
(`presentation/.../backend/surfaces.rs:376-386`).

**Failure:** `grab="click"` cannot provide its documented outside-click
dismissal and keyboard ownership; it silently degrades to the hover-bridge
behavior.

**Improvement:** Preserve the triggering seat and pointer press serial through
input normalization, component request creation, and popup configuration. Make
the serial one-shot and generation-bound, reject stale serials, and diagnose a
requested grab that cannot be honored. Test click, keyboard, outside-click, and
stale-serial paths.

### 5. P1 — Popup update assumes protocol v3 and does not validate popup identity
or parentage

SCTK binds `xdg_wm_base` at the compositor's advertised version, including v1
and v2, while `xdg_popup.reposition` exists only since v3. MESH calls it
unconditionally (`backend/surfaces.rs:434-448`), which can send an unsupported
request and disconnect on an older compositor.

The update fast path also treats any existing `surface_id` as an existing popup,
mutates its padding, and returns success without checking its role or parent
(`backend/surfaces.rs:312-315`). A collision with a layer/window is silently
accepted; a popup reparent request leaves `PopupRole.parent_id` and the real xdg
parent unchanged (`:405-410`).

**Improvement:** Record a capability matrix including negotiated protocol
versions. Gate reposition on xdg-popup v3 and recreate safely on v1/v2. Require
the existing entry to be a popup with the same live parent; reject collisions
and recreate or explicitly reject reparenting. For v3+, track reposition tokens
and parent configure/size inputs; use reactive positioners when supported.

### 6. P1 — Dynamic layer size resolution is not the size used by present

For spanning top/bottom or left/right layer surfaces, the size query resolves a
zero configure dimension against the actual output
(`backend/config.rs:335-367`). The layer-shell protocol explicitly permits a
configure dimension of zero so the client can choose it. The shell can therefore
paint an output-sized buffer, but `present_with_damage` uses raw
`entry.width/height` (`backend/present.rs:80-87`), which may still be the
one-pixel creation stand-in, as the logical viewport/commit size. The attach path
then writes that raw logical size back into the entry (`backend/entry.rs:487-493`).

**Failure:** A valid zero-dimension configure can make the shell paint a full
bar or rail but present it through a 1-pixel logical destination, producing a
collapsed or rescaled surface.

**Improvement:** Resolve one authoritative logical extent before paint and use
that same value for buffer validation, viewport destination, regions, attach,
and subsequent queries. Add live/simulated zero-width top-bar and zero-height
rail configures, with and without a known output.

### 7. P1 — Region-only changes have no guaranteed commit

`update_opaque_region` stages `wl_surface` state but does not commit it
(`backend/present.rs:215-245`). Blur changes remain pending until
`present_with_damage` (`:112-156`). The shell records region cache state before
collecting damage, then skips present entirely when a visible frame has no pixel
damage (`shell/runtime/render/mod.rs:795-826`, `:956-970`).

**Failure:** An idle surface can retain stale compositor opacity or blur until
some unrelated future repaint. The cache already says the update was sent, so
recreation or another no-damage frame may not retry it.

**Improvement:** Treat surface-state changes as commit work independent of pixel
damage. Stage regions in presentation and issue one configured state-only commit,
or make the frame plan carry protocol-state damage that forces a commit. Verify
opaque and blur changes with an empty paint-damage list.

### 8. P1 — SHM exhaustion and frame-callback timeout can become a hot retry loop

When every SHM slot is compositor-owned, presentation correctly returns
`NotReady` (`backend/entry.rs:386-400`, `backend/present.rs:102-110`). The shell
retains damage and requests paint again. After the 50ms frame-callback hint
expires, `waiting_for_frame_callback` becomes false even if no buffer was
released (`backend/entry.rs:496-501`), making the still-dirty component ready
again (`shell/runtime/mod.rs:178-195`). Repeated paint/`NotReady` cycles can then
run without a release event.

The callback state is also a boolean/timestamp. A late callback from before a
hide/show can clear the pacing state for a newer frame
(`wayland_surface/handlers.rs:50-65`, `backend/entry.rs:323-331`).

**Improvement:** Separate `AwaitingFrameCallback` from `BlockedOnBufferRelease`,
wake the latter on an actual release/Wayland event, and track callback/object
generations. Keep a bounded recovery deadline for broken compositors without
turning it into an immediate render retry. Add three-busy-buffer, occlusion,
late-callback, and hide/show regressions.

### 9. P2 — Surface and seat removal leave stale input ownership

The central remove path does not clear pointer focus, keyboard focus/repeat,
gesture ownership, or active touch-id mappings (`wayland_surface/state.rs:333-337`).
Keyboard and touch follow-up events can still emit the removed surface id
(`wayland_surface/handlers.rs:624-679`, `:1043-1075`). Removing a keyboard seat
also releases the object without clearing keyboard focus or repeat state
(`handlers.rs:293-317`).

**Improvement:** Make teardown cancel every owned input transaction and emit
the necessary leave/cancel result before removing identity. Store input state
per seat rather than in the current single-seat globals. Test removal during a
key repeat, touch sequence, pointer capture, and gesture.

### 10. P2 — Several presentation failures are silently converted into stale
or apparently successful output

The SHM copy helpers silently return or skip rows when `PixelBuffer::data` is
short (`backend/damage.rs:5-27`, `:184-207`), even though `PixelBuffer` storage
and dimensions are publicly mutable. The buffer is still attached and reported
as presented, so a reused SHM slot can expose stale pixels. The attach result is
also discarded with `.ok()` (`backend/entry.rs:487`). Blur-region creation can
fail and still clear its dirty bit (`backend/present.rs:125-155`). Normal
`pump()` and `poll_events()` discard Wayland dispatch errors
(`backend/events.rs:3-21`).

`WaylandClipboard` has the same cleanup shape at the platform edge: a stdin
write error returns before `child.wait()` (`platform/wayland/src/lib.rs:139-158`),
which can leave the helper running or unreaped.

**Improvement:** Validate checked buffer length/stride/scale relationships before
copy, propagate attach and region errors without clearing pending state, surface
connection loss as a lifecycle failure, and reap/terminate the clipboard child
on every exit path. This buffer validation folds into the Section 12 bounded,
storage-safe `PixelBuffer` work.

### 11. P2 — The testing backend is a recorder, not a presentation lifecycle
model

The testing backend records config without entering an unconfigured state,
accepts popup configurations without checking capability/parent/identity, treats
unknown visible surfaces as presentable, never models frame callbacks or SHM
release, and reports no frame wait (`crates/core/presentation/src/lib.rs:232-242`,
`:340-351`, `:440-466`, `:571-578`). Tests can manually toggle one configure
flag, but most shell tests exercise semantics materially weaker than the real
backend.

**Improvement:** Replace it with a deterministic protocol-state simulator that
can inject create failure, configure, close/dismiss, frame callback, buffer
release/backpressure, output, scale, and connection-loss events. Keep a small
live compositor matrix for behavior the simulator cannot prove.

### 12. P3 — Input events expose avoidable correctness and feature gaps

Escape is normalized to `"Esc"` (`wayland_surface/handlers.rs:722-738`) while
the non-repeat filter recognizes only `"escape"`
(`wayland_surface/state.rs:383-397`), so Escape can repeat. Pointer dispatch
drops every button except primary (`handlers.rs:386-413`), and keyboard text
delivery keeps only the first Unicode scalar from the key event (`:641-660`).

**Improvement:** Fix the normalized Escape regression immediately. Evolve the
event contract to carry button identity and add an IME/text-input-v3 path for
composition, multi-character commits, and accessibility-grade text entry.

## Unconstrained feature direction

The stronger feature is a transactional, capability-aware presentation engine,
not a larger collection of setters:

```text
Validated SurfaceIntent
  role + placement + content/padded/physical extents + regions + input intent
        |
        v
PresentationPlan
  Noop | StateCommit | Configure | Reposition | Recreate | Reject
        |
        v
Per-surface state machine
  Absent -> Created -> AwaitingConfigure -> Ready
                 |             |             |
                 |             |             +-> FramePending
                 |             +-> RepositionPending
                 +-> Hidden / Closed / Dismissed / Lost
        |
        v
CommittedSurfaceSnapshot
  object generation + role + configure generation + logical/physical extent
  + applied regions + buffer generation + output membership + capabilities
        |
        v
Typed lifecycle feedback to shell
  Configured | Presented | Backpressured | Closed | Dismissed | Lost | Rejected
```

This makes several better features natural rather than special cases:

- real versioned capability reporting for layer-shell, xdg popup reposition,
  fractional scale, viewporter, blur, activation, and focus grabs;
- atomic role changes and last-known-good recovery;
- reactive/reparent-safe popups with correlated click serials;
- correct multi-output membership and output-targeted surfaces;
- per-seat input ownership, full pointer buttons, and IME text input;
- direct SHM paint or future dmabuf/GPU buffers behind the same frame contract;
- deterministic lifecycle simulation plus portable live-compositor conformance.

`mesh-core-wayland::CompositorCapabilities` is currently only a stub-oriented
name/version/string interface (`crates/core/platform/wayland/src/lib.rs:166-195`).
It should become the typed capability snapshot consumed by the planner, while
concrete Wayland objects remain private to `mesh-core-presentation`.

## Recommended implementation order

1. Make configure/present/lost errors typed; stop reporting missing visible
   surfaces as presented and preserve the last-known-good role on replacement.
2. Centralize idempotent teardown and lifecycle events, including input state,
   children, optional protocol objects, and connection loss.
3. Introduce object/configure/frame/buffer generations and the typed semantic
   config diff; invalidate all applied regions on object generation changes.
4. Gate popup behavior by negotiated version, validate popup identity/parent,
   and carry click serials; then add reactive/reposition transaction handling.
5. Unify logical/physical extent resolution and state-only region commits; add
   checked buffer validation and attach/error propagation.
6. Separate frame pacing from buffer-release backpressure and make output/seat
   ownership explicit.
7. Build the deterministic lifecycle simulator and live compositor matrix,
   then extend buttons, IME, multi-seat, and output-targeting features.

## Regression matrix

| Area | Regression |
| --- | --- |
| Creation | Failed create/recreate leaves the old role or reports `Lost`; shell does not cache success |
| Missing | Visible present to an absent id retains damage and cannot return `Presented` |
| Close | Layer `closed` and popup `done` run full child/auxiliary/input teardown and notify shell |
| Config | Blur, namespace, and decorations cause the declared recreate/reject action |
| Regions | Role or hide/show recreation reapplies opaque/input/blur state once |
| State commit | Opaque/blur-only change commits with no pixel damage |
| Popup grab | Fresh click seat/serial produces a compositor grab; stale/missing serial diagnoses downgrade |
| Popup version | xdg v1/v2 never receives `reposition`; v3+ correlates the reposition/configure sequence |
| Popup identity | Layer/window id collisions are rejected; parent changes recreate or reject explicitly |
| Dynamic size | Zero-width top bar and zero-height rail use one resolved extent from layout through commit |
| SHM | Three busy slots block on release without repaint spin and preserve all pending damage |
| Callback | Late callback from an older frame/object cannot clear a newer frame gate |
| Input | Destroy during key repeat/touch/gesture cancels ownership; Escape does not repeat |
| Buffer | Short storage, overflow, scale/extent mismatch, and attach failure return typed errors |
| Output | Enter/leave/destroy re-evaluates the correct surface output without stale membership |
| Simulator | Testing backend enforces create -> configure -> present -> release and hide/recreate ordering |
| Live | wlroots/Hyprland/KWin cover close, popup, fractional scale, occlusion, and multi-output paths |

## Verification

- `nix develop -c cargo test -p mesh-core-presentation --lib`: 64 passed,
  12 ignored.
- Source/protocol validation confirmed `xdg_popup.reposition` is a version 3
  request and confirmed SCTK's configure acknowledgment ordering.
- No performance claim is made; no production code was changed.
