# Runtime architecture direction: what a from-scratch design would change

**Status:** accepted direction, 2026-10-10; partial implementation through
2026-10-13. Binding patches cover direct service fields, keyed loop bodies,
and template calls. Shipped frontend `render()` hooks are migrated to derived
expressions and tracked effects; backend tasks and the remaining architecture
tracks are still open. Backlog lines are under
"Runtime architecture direction" in
[`docs/BACKLOG.md`](../../../docs/BACKLOG.md). This note builds on
[the authoring-model review](2026-10-10-authoring-model.md) and
[script timers and typed shell ops](../../log/2026-10.md).

## What stays

The module, contract, capability and profile architecture stays, together
with Luau, separate backend providers, retained rendering, and the
"core owns mechanisms, modules own experiences" boundary
([00](../../../docs/spec/00-philosophy.md)). Everything below changes the
runtime model *under* scripts, not the platform model.

## D1. Change tracking at write time (compile-time fine-grained reactivity)

**Baseline at the review:** the runtime reconstructed what changed after a script ran: it scans
globals, converts state to JSON, runs `render()` for every instance, re-evaluates
templates and diffs the tree. Memoization, narrow invalidation, fingerprints and
the retained diff exist largely to make that reconstruction cheaper.

**Direction** (the Svelte 5 / Solid model):

- The `.mesh` compiler rewrites top-level variables that templates read into
  signals. Authors keep writing `volume = 42`; the write is tracked.
- Each `{expr}` becomes an effect bound to one node property, so a write updates
  only the properties that depend on it. There is no tree rebuild and no diff.
- Service proxy fields are signals. Reading `audio.percent` inside an expression
  subscribes to that field only.
- The script body runs once as setup. `render()` is retired. Derived state is an
  expression or `computed(fn)`; side effects use `effect(fn)` with automatic
  cleanup.
- Keyed `{#each}` keeps list reconciliation. Collections used as list sources
  are deep-proxied stores; other state is scalar or replaced wholesale.

**Migration:** compiled bindings can coexist with the current template
evaluation and retained diff, which remains the fallback for constructs the
compiler cannot bind directly. Convert shipped modules component by component.

**Shipped D1 implementation (2026-10-13):** scalar writes are tracked through
runtime proxies; recorded element bindings patch direct service fields, keyed
loop bodies, and template calls. Structural readers retain reconciliation.
Frontend setup runs once, automatic `render()` scheduling is retired with a
migration diagnostic, and shipped hooks use derived expressions or `effect(fn)`.
Effects track public scalar and service-field inputs, settle under callback
budgets, and clean up on rerun, cancellation, reload, or unmount. Compound public
inputs remain conservative. The clock updates public time at minute boundaries
through its task scheduler. Current evidence is in the performance log.

**Open:** whether signals are Rust-owned values (backlog item R) or Luau-side
cells. Also how deep-store mutation is tracked without the table reconstruction
costs already rejected for storage reads (see the rejected-experiments table).

## D2. Async as coroutines with structured lifetime

- Each component gets one task scheduler built on Luau coroutines:
  `task.delay(ms, fn)`, `task.spawn`, `task.sleep`.
- Service methods, exec, sockets and file reads yield instead of returning
  through events or callbacks.
- Tasks are owned by the component instance and cancelled on unmount or
  reload.
- This subsumes `mesh.timer`, the `shell.*-handler` channels, and the
  set-then-reconcile command pattern.
- Budgets apply per resumption. The frame loop wakes on the earliest task
  deadline, as it does today for scheduled handlers.

**Shipped D2 implementation (2026-10-13):** frontend and backend contexts own
a deadline queue; `ticket:await()` parks frontend tasks on service calls.
Backend `start`, `on_poll` and command handlers run as coroutines, and exec,
socket and file reads suspend on a bounded host worker pool (64 process-wide).
Readiness waits for `start`; polls never overlap; suspended commands reply
late and honour cancellation. D8 (2026-10-18) also runs stream hooks and `stop`
as coroutines. Later stream callbacks remain in the bounded queue until the
current callback settles; stop cancels previous tasks and has a two-second
shutdown deadline.

## D3. Capabilities checked at acquisition (object capabilities)

- `require("mesh.audio")` returns a capability object only when the grant
  exists. Calls on it are plain calls with no per-call string authorization.
- Shell operations become a capability object too (`mesh.shell`).
- `mesh.events.publish` is kept only for explicit broadcast between modules,
  and is never used for RPC.
- Typed arguments are validated once, at the call boundary.

## D4. Typed values end to end

- Compiled contracts (S03-DEAD-001) generate payload types, validators and
  Luau type definitions (`.d.luau`). The LSP and the Luau analyzer then
  type-check scripts and template expressions.
- Providers send typed field patches instead of whole JSON snapshots, and
  validation happens once, at the provider boundary.
- Frontends see typed, range-checked fields, with a defined value for an absent
  optional service.
- JSON is kept for persistence and IPC only, not the frame path.

## D5. Headless interaction primitives in core

Popover, menu, tooltip, hover intent, focus scope, dismissal, and roving focus
become core element behaviors that modules compose (the Radix / React Aria
model), subject to the element admission test in 00 §4. This subsumes the
hover-intent backlog item and the per-module bridges.

**Shipped D5 implementation (2026-10-13):** popover disclosure.
`<popover open-on="hover|click">` gets hover intent, capture hold, trigger
keyboard toggling, Escape with focus return, outside-press and focus-leave
dismissal from core. The controller proposes changes through a simple
`open={variable}` write-back plus `openchange`, and triggers get `:expanded`,
derived accessible `expanded` and tooltip suppression. Shipped popovers no
longer forward events to their triggers.

**Shipped remainder (2026-10-20):** dialog/sheet and explicit focus scopes,
modal Tab trapping, focus entry/return, Escape/outside dismissal, and menu
prefix search, first/last navigation and nested-menu entry/return. Closed
dialogs, sheets and menus are absent from layout, painting and semantics.
The keyboard spec and elements guide define the authoring contract; October's
log records validation. This direction note remains for the other open items.

## D6. Threaded scene commit and GPU raster

- The UI thread produces an immutable scene/layer snapshot per frame and
  commits it to a render thread (the Chromium compositor-commit / Flutter
  layer-tree model).
- Raster runs on the GPU. Scrolling becomes layer translation, and blur and
  opacity become layer effects.
- Presentation stays damage-driven, so idle cost does not rise.
- Extends the existing GPU lanes
  ([GPU backend](2026-07-15-gpu-rendering-backend.md),
  [persistent scene renderer](2026-09-20-persistent-scene-renderer.md)) and
  SH-11's immutable `PaintContext`. It would make much of the scroll raster cache
  and in-surface blur work unnecessary, so re-check those plans against this
  before investing further.

## D7. Styles compiled to typed form with incremental matching

- Theme tokens are resolved to typed values once per theme load.
- Selectors are matched through dependency indexes, so a class or state change
  is a bitset flip that re-styles only matching nodes.
- Pick the supported CSS subset partly for incremental matchability.
- Extends the existing "typed style declarations" and interning items.

## D8. Event-driven backends by default

- First-class async host APIs for D-Bus signals, PipeWire, netlink, inotify and
  Wayland protocols, built on D2's coroutines.
- Polling and monitor subprocesses become fallbacks.
- Extends the existing D-Bus signal subscription item.

**Shipped D8 implementation (2026-10-18):** D-Bus signal filters, PipeWire
Node/Metadata invalidations, route netlink groups and wlr foreign-toplevel
observation join the existing inotify/socket streams. All use generation-owned,
bounded handles, cancellation and readiness/error events. The three shipped
power/network/audio providers prefer native events when granted and preserve
monitor/poll fallbacks. Protocol interpretation and domain controls stay in Luau.
The API contract is in [backend host events](../../../docs/modules/backend-events.md);
measurements and verification are in the performance log. This establishes native
observation, not arbitrary D-Bus methods, PipeWire control or every Wayland protocol.

## D9. Performance budgets per workload

- Each workload has a gate in CI: idle wakeups per minute, hover frame, service
  poll update, scroll frame, popover open, and theme switch.
- Each direction item above lands with its gate's before and after
  measurements.

## Suggested order

1. **D3 + D4.** They cost little and are boundary-only. They also unblock D1
   and D2.
2. **D2.** The task scheduler is self-contained and removes the event-RPC
   patterns.
3. **D1.** Prototype on one surface, the navigation bar. Compare against the
   retained-diff path with `navigation_frame_cost_profile`. Then expand.
4. **D5**, alongside D1, as module code is rewritten.
5. **D6 / D7** on the existing GPU and style lanes. Re-scope the scroll
   raster cache and blur plans first.
6. **D8** as providers are touched. **D9** from now on.

## D6 scene handoff contract

`mesh-core-render::scene_thread` provides the preparatory commit boundary.
`SceneSnapshot` pins an immutable `FramePaintPlan`, a matching typed resource
snapshot, scale, and surface incarnation. A renderer is constructed, used, and
destroyed on one worker thread and may own a GPU context that is not `Send`.
Only owned snapshots and transferable presentation outputs cross the boundary.
Output destruction must be safe on the UI thread and after renderer shutdown.

The queue admits a configured number of surfaces and holds at most one pending
scene and one completed output per surface, with one render active on the worker.
New commits coalesce pending work. Superseded and removed surface outputs cannot
become current. Removed incarnations retain their admission slot until the
renderer retires their resources. Worker sleep uses a condition variable, not
polling. Shutdown discards unpublished work and joins the renderer owner.

Presentation acknowledgment is explicit. A first frame, unacknowledged earlier
commit, changed scale/extent/resource revision, or skipped intermediate frame
requires full topology and surface replay. A partial-damage plan cannot be used
as if the skipped frame had been presented. Resource and output leases retire
outside the queue lock so cleanup can re-enter resource ownership mechanisms.

**Shipped integration (2026-10-20):** the UI prepares a complete Skia picture
with pinned immutable raster images, icon variants, glyph pages, backdrop and
tooltip settings. Layout and glyph preparation use the same UI font database;
the worker never resolves assets or loads an installed font catalog. This
prepared-raster adapter replaces the need to freeze all host font sources in
the production worker. `FrozenFontDatabase` remains an optional lower-level
adapter. Immutable `PaintContext` values replace shell paint-setting mutations.

The scene worker owns retained GPU pixels, at most three exported textures per
surface and a shared 512 MiB target budget. Partial damage clips picture replay;
export slots accumulate pending damage while compositor-held. Descriptor-only
outputs and incarnation-specific leases cross to Wayland; buffer release,
supersession, removal and shutdown never call GL on the UI thread. Presentation
currently waits on completion, so frame-N/frame-N+1 pipelining remains separate.
Parity, lifetime, queue, buffer backpressure and live-compositor checks accompany
the implementation. Measurements are in the performance log; small damage
frames pay complete UI recording overhead. No universal speedup is claimed.

### D7 token lowering contract

Reference-bearing theme tokens cache their resolved scalar values on the
immutable snapshot. Clones share the cache; every style-bearing mutable accessor
detaches it. Root-specific errors and CSS fallbacks retain existing semantics.
Missing arbitrary names are not retained. The UI additionally shares parsed
color/number projections in per-thread caches bounded to 16 theme revisions and
512 token references per revision. Local variables and component props bypass
those projections when they override the token. Remaining property-specific
lowering and selector dependency work stay under D7; token microbenchmarks do
not establish an end-to-end theme-switch improvement.

### D6 frozen font resource adapter

`FrozenFontDatabase` supplies owned byte sources for the scene handoff. It
preserves face metadata and provides explicit ID remapping; layout and painting
must consume the same prepared database. Source reservations stay attached to
cloned databases, including worker-thread clones. Cancellation or byte-budget
failure drops the entire candidate. Production scene integration uses prepared glyph images rather than copying
the full installed catalog. Consumers that use this adapter must still select
a host font-catalog residency policy rather than assuming every installed font
fits within one resource budget.
