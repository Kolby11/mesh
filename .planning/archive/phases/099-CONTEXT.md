# Phase 99: Event-Driven Wayland Dispatch - Context

**Gathered:** 2026-06-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Replace `std::thread::sleep` at the bottom of `Shell::run()` with a blocking `poll()` on the Wayland connection fd using the deadline already computed by `next_runtime_sleep()`. Add eventfd IPC wakeup so backend/command messages wake the blocking poll without adding latency. Preserve drain-first loop ordering, keep dev-window backend on its existing sleep path, and record scheduler idle behavior in profiling infrastructure.

This phase delivers: zero idle CPU burn, frame-callback-driven wakeup, eventfd-triggered IPC wakeup, dev-backend compatibility, and scheduler profiling — all without new crate dependencies.
</domain>

<decisions>
## Implementation Decisions

### Blocking Dispatch Design
- Add `wait_for_events(deadline: Duration)` method to `PresentationEngine` that delegates to `LayerShellBackend` — keeps dispatch ownership in presentation crate
- Reuse `next_runtime_sleep()` output directly — it already computes the correct deadline from all wakeup sources (message backlog, pending Wayland events, render needs, reload timers, command throttle, surface transitions); only remove the `MAX_IDLE_SLEEP` (16ms) clamp
- Add `wait_for_events(deadline)` as a new method alongside `dispatch_available()` — isolates the blocking change; `dispatch_available` stays non-blocking for Wayland dispatch during the hot-work portion of the loop

### IPC Wakeup Mechanism
- Use eventfd via `rustix::event::eventfd` — already in dependency tree (rustix 0.38), Linux-only which matches Wayland-only constraint
- Signal eventfd immediately after `tx.send()` in IPC server and backend spawn paths — simple, guaranteed wake; the existing 256-message drain cap prevents spamming
- Poll both fds in sequence: poll Wayland fd with deadline first, then check/clear eventfd — avoids epoll complexity; eventfd read+consume is near-instant

### Dev-Backend Path & Profiling
- `PresentationEngine` exposes `supports_blocking_dispatch() -> bool` — returns `true` for WaylandSurface, `false` for DevWindow; shell branches on this at loop bottom
- Dev-window keeps `std::thread::sleep` — minifb is CPU-polled with no fd to block on
- Add `ProfilingStage::SchedulerIdle` variant to the enum — separate stage for scheduler visibility
- Capture block duration + wake reason (WaylandEvent, IpcEvent, DeadlineExpired) — actionable for compositor compatibility testing

### OpenCode's Discretion
- Exact placement of eventfd read/clear relative to message drain
- Internal method signatures and error propagation patterns
- How `wait_for_events` interacts with the existing `flush_wayland` / `pump` sequence
</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Shell::next_runtime_sleep()` at `mod.rs:47-102` — already computes deadline from all wakeup sources, only needs MAX_IDLE_SLEEP removal
- `LayerShellBackend::dispatch_available()` at `backend.rs:795-864` — already implements prepare_read/poll(0)/read/dispatch loop; only needs timeout parameterization
- `CompositorHandler::frame()` at `handlers.rs:22-37` — frame callbacks already clear `frame_pending` flag
- `ShellMessage` mpsc channel — IPC drain at `mod.rs:169-182` already occurs before render/present/block
- `ProfilingStage` enum at `debug/src/lib.rs:357-373` — 13 existing stages, adding one more is well-established pattern
- `PresentationEngine` interface — already abstracts `WaylandSurface` vs `DevWindow` backends

### Established Patterns
- PresentationEngine trait methods delegate to backend — `wait_for_events` follows same pattern
- Shell loop order is drain → tick → render → present → sleep — blocking goes after present, not before drain
- rustix `poll()` and `eventfd` are already used in the codebase (via rustix 0.38)
- Profiling stages use `ProfilingSnapshot::snapshot(stage)` — minimal overhead, zero-alloc

### Integration Points
- `Shell::run()` loop at `mod.rs:162-196` — the `std::thread::sleep(sleep_for)` at line 194 is the replacement target
- `PresentationEngine` trait — new `wait_for_events` and `supports_blocking_dispatch` methods
- `spawn_ipc_server()` at `ipc.rs:11` and `spawn_backend_modules()` — need eventfd signaling after `tx.send()`
- `ProfilingStage` enum — new `SchedulerIdle` variant
</code_context>

<specifics>
## Specific Ideas

No specific user requirements beyond the research findings — implementation follows ROADMAP success criteria and research recommendations.
</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.
</deferred>
