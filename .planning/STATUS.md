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

Complete per-seat input ownership and cancellation for ordinary surface/seat
teardown, then add object/configure/frame generations and the remaining
semantic presentation diff. Resource preparation and text shaping remain
separate follow-up work.
