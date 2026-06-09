# Stack Research

**Domain:** Event-Driven Wayland Frame Scheduler
**Researched:** 2026-06-09
**Confidence:** HIGH

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| smithay-client-toolkit | 0.19.2 (already in use) | Wayland protocol binding (compositor, layer-shell, seat, SHM) | Already the platform foundation; SCT's `CompositorHandler::frame()` dispatches frame callbacks that the scheduler will block on |
| wayland-client | 0.31.14 (already in use) | Low-level Wayland protocol objects (`wl_surface`, `wl_region`, `EventQueue`) | Provides `WlCompositor::create_region()` for opaque region objects and `EventQueue::prepare_read()` for fd-based blocking |
| wayland-backend | 0.3.15 (already in use, transitive) | Connection fd and event loop raw I/O | `ReadEventsGuard::connection_fd()` returns the raw fd used by `rustix::event::poll` for deadline-blocking |
| rustix | 0.38 (already in use) | `event::poll()` with timeout | Already used in `dispatch_available()` for non-blocking poll; same call accepts a `Duration` for blocking |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| wayland-protocols | 0.32.12 (transitive via SCT) | wlr-layer-shell, xdg-activation protocol bindings | Required by layer-shell backend; no direct scheduler changes here |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| None new | Existing profiling infrastructure covers scheduler changes | `mesh_core_debug::ProfilingStage` already tracks shell stages; add `FrameWait` stage for blocking-duration measurement |

## Installation

```bash
# No new Cargo.toml entries needed. All dependencies already present:

# Already in crates/core/presentation/Cargo.toml:
# smithay-client-toolkit = { version = "0.19", default-features = false, features = ["xkbcommon"] }
# wayland-client = "0.31"
# rustix = { version = "0.38", features = ["event", "fs", "mm", "shm"] }
```

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| `rustix::event::poll()` on `EventQueue` connection fd | calloop (optional SCT feature, 0.13.x) | Only if you need to multiplex with non-Wayland fd sources (IPC socket, timers). MESH already has `mpsc::unbounded_channel` for IPC; calloop adds dependency complexity without solving a current problem |
| Deadline passed to `poll()` directly | tokio async wrapper around EventQueue | Would require converting the synchronous shell loop to async, pulling in a reactor, and paying runtime overhead. The shell loop is intentionally synchronous and single-threaded |
| `EventQueue::dispatch_pending()` in a spin loop | `std::thread::park()` / condition variable | `park()` can't be signaled by Wayland fd readiness; need `poll` or equivalent. Spin loops burn CPU |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| calloop (`calloop` 0.13, `calloop-wayland-source` 0.3) | SCT 0.19.2 has calloop as an *optional* feature; the current codebase does not activate it. Adding calloop would require changing the entire event-dispatch architecture (replace `EventQueue` with `calloop::EventLoop` + `WaylandSource`) for marginal benefit — the single-fd use case is trivially handled by `poll()` | Existing `rustix::event::poll` on `EventQueue::prepare_read()` connection fd |
| tokio / async-std / smol | The shell loop (`Shell::run()`) is synchronous, single-threaded, and must remain so for deterministic rendering order, profile attribution, and frame-commit sequencing. Adding an async runtime would complicate ownership and introduce scheduling unpredictability. | Synchronous `poll()` blocking with computed deadline |
| `std::thread::sleep()` | This is the current implementation and the *thing we are replacing*. `thread::sleep` cannot be woken by Wayland frame callbacks or input events — it wastes CPU cycles polling on every wakeup and adds unnecessary latency. | `poll()` blocking on the Wayland connection fd |
| New polling-timer crate (e.g., `timerfd`) | Not needed. The deadline computation is a pure in-memory operation; the `poll()` timeout is set to `min(computed_deadline, 16ms)` and automatically returns early if Wayland data arrives. | `poll()` timeout parameter |

## Stack Patterns by Variant

**If Wayland compositor is available (production path):**
- Use `LayerShellBackend::wait_for_events(timeout)` — blocks on Wayland fd via `rustix::event::poll` with computed deadline
- `wl_surface::frame` callbacks dispatched automatically through SCT's `CompositorHandler::frame()`
- `set_opaque_region` computed from retained display list opaque background rects and sent before each `wl_surface.commit()`

**If using dev-window backend (development path):**
- `DevWindowBackend` continues using its existing event-driven loop (minifb internal polling)
- `surface_waiting_for_frame_callback` returns `false` — no deadline blocking needed
- No `set_opaque_region` call (dev window not a real Wayland surface)

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| smithay-client-toolkit 0.19.2 | wayland-client 0.31.14 | Locked — both are workspace dependencies with no version conflicts |
| wayland-client 0.31.14 | wayland-backend 0.3.15 | Locked — `EventQueue::prepare_read()` returns `ReadEventsGuard` from `wayland-backend` |
| rustix 0.38 | All above | Locked — used for `event::poll()`, `event::PollFd`, `event::PollFlags` |

## Key API Points

### 1. Blocking Dispatch Pattern
The existing `dispatch_available()` method in `LayerShellBackend` already implements the correct pattern but with `poll(&mut fds, 0)` (non-blocking). The change is:
```rust
// Current: non-blocking poll
poll(&mut fds, 0)

// New: deadline-blocking poll
poll(&mut fds, timeout.as_millis().min(i32::MAX as u64) as i32)
```

### 2. wl_region Creation
`WlCompositor::create_region()` is available at `wayland-client` 0.31.14 (wayland protocol `wl_compositor` version 1+). Access via SCT's `CompositorState::wl_compositor()` which returns `&WlCompositor`.
```rust
let compositor = state.compositor_state.wl_compositor();
let region = compositor.create_region(&qh, ());
region.add(rect.x as i32, rect.y as i32, rect.width as i32, rect.height as i32);
wl_surface.set_opaque_region(Some(&region));
// Region can be destroyed immediately — opaque region has copy semantics
region.destroy();
```

### 3. Frame Callback Already Wired
`wl_surface.frame(qh, wl_surface.clone())` is already called in `SurfaceEntry::attach_shm_buffer()` (line 267 of `backend.rs`). The `CompositorHandler::frame()` handler (line 22-37 of `handlers.rs`) clears `frame_pending` and `frame_pending_since`. No new protocol bindings needed.

## Sources

- [docs.rs/smithay-client-toolkit/0.19.2] — `CompositorState`, `CompositorHandler::frame()`, `SurfaceData` — HIGH confidence (official crate docs)
- [docs.rs/wayland-client/0.31.14] — `WlCompositor::create_region()`, `WlSurface::set_opaque_region()`, `EventQueue::prepare_read()`, `wl_region` — HIGH confidence (official crate docs)
- [docs.rs/rustix/0.38] — `event::poll()` with timeout — HIGH confidence (official crate docs)
- [MESH codebase] — `crates/core/presentation/src/wayland_surface/backend.rs` (existing `dispatch_available()`, `attach_shm_buffer()`, `frame_pending` tracking), `handlers.rs` (frame callback handler), `state.rs` (State struct) — HIGH confidence (verified against live code)
- [MESH codebase] — `crates/core/shell/src/shell/runtime/mod.rs` (`next_runtime_sleep()` deadline calculation, `run()` loop with `std::thread::sleep`) — HIGH confidence (verified against live code)

---
*Stack research for: Event-Driven Wayland Frame Scheduler*
*Researched: 2026-06-09*
