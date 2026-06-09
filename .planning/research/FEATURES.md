# Feature Landscape

**Domain:** Event-Driven Wayland Frame Scheduler for MESH
**Researched:** 2026-06-09
**Confidence:** HIGH

## Table Stakes

Features users expect. Missing = product feels incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Block on Wayland connection fd instead of fixed sleep | No idle CPU burn; shell sleeps until compositor sends data | Low-Medium | Existing `dispatch_available()` already implements the full prepare_read/poll/read/dispatch loop — only the poll timeout changes from 0ms to the computed deadline |
| wl_surface::frame callback as render permit | Standard Wayland throttling; compositors expect clients to wait for frame before repainting | Low | Already implemented: `wl_surface.frame()` called in `attach_shm_buffer()`, `CompositorHandler::frame()` clears `frame_pending` |
| wl_surface::set_opaque_region from present path | Compositor compositing optimization; expected for well-behaved shell surfaces | Low-Medium | Requires creating `wl_region` objects from `WlCompositor` (already bound); computing opaque rects from retained display list background fills |
| Deadline calculation (already implemented) | Determines how long to block — must consider all wakeup sources | Low | `next_runtime_sleep()` already computes deadline from shell messages, pending events, render needs, reload timers, throttled commands, and surface transitions |
| Frame callback timeout fallback | Prevents indefinite blocking if compositor drops a frame callback | Low | Already implemented: `MAX_FRAME_CALLBACK_WAIT = 50ms` in `SurfaceEntry::waiting_for_frame_callback()` |

## Differentiators

Features that set product apart. Not expected, but valued.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Opaque region from retained display list background analysis | Tells compositor which pixels are fully opaque without shell authors needing to annotate anything — automatic optimization | Medium | Requires walking the retained display list to find background fills with alpha=1.0 and computing their union. Only applies to fully-opaque backgrounds, not semi-transparent shells |
| Coalesced event dispatch in blocking path | When the fd wakes, drain all pending events before returning to shell work — avoids multiple wake/drain cycles per frame | Low | Already the pattern in `dispatch_available()` — just parameterize the timeout but keep the batch-drain loop |

## Anti-Features

Features to explicitly NOT build.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| tokio-based async event loop | The shell loop is synchronous by design for deterministic rendering order, profile attribution, and frame-commit sequencing. Adding an async runtime would complicate ownership and introduce scheduling unpredictability | Blocking `poll()` with deadline timeout — achieves the same effect (sleep until event) without an async runtime |
| calloop integration | SCT 0.19 already has calloop as an optional feature; the current codebase does not activate it. Migrating to calloop would require rewriting the entire dispatch architecture (`EventQueue` → `calloop::EventLoop` + `WaylandSource`) | Continue using `rustix::event::poll` directly — simpler, already tested, one less dependency |
| Per-surface fd blocking | Each MESH surface shares the same Wayland connection; there is only one fd. Polling per-surface would require epoll and doesn't add value | Block on the single connection fd; all surfaces' events arrive on the same fd |
| Compositor-specific opaque region hints (Hyprland-only, Sway-only) | Violates Wayland protocol neutrality; would break on KWin/Mutter | Standard `wl_surface::set_opaque_region` — works on all compliant compositors |
| Polling timer for deadline enforcement | Adding a timerfd or similar would mean blocking on *two* fds (Wayland + timer), requiring epoll. Deadlines shorter than the next Wayland event are rare (the 16ms cap ensures responsiveness) | Deadline as poll timeout — if no Wayland event arrives, the shell wakes at the deadline naturally |

## Feature Dependencies

```
Block on Wayland fd ←→ Deadline calculation (next_runtime_sleep)
    ↓
Frame callback handling (already implemented)
    ↓
Opaque region computation ←→ Retained display list access
    ↓
wl_surface::set_opaque_region ←→ WlCompositor::create_region (already bound)
```

Key dependency: The opaque region computation depends on access to the retained display list from the paint/present path. The presentation engine receives `PixelBuffer` but not display-list structure. Either:
- Pass opaque-rect metadata alongside the pixel buffer in `present_with_damage()`
- Or have the shell pre-compute opaque rects and pass them through the presentation interface

## MVP Recommendation

Prioritize:
1. **Blocking dispatch on Wayland fd** — replaces `std::thread::sleep()`, immediately eliminates idle CPU burn
2. **wl_surface::set_opaque_region** — sends opaque region hints; wired from the present path
3. **Opaque rect computation from retained display list** — automatic, no author annotations needed

Defer:
- Compositor-specific optimizations (damage region negotiation, wp_presentation feedback)
- Multi-fd epoll integration — only needed if MESH adds timerfd or additional socket sources
- Frame callback statistics/diagnostics beyond existing profiling infrastructure

## Sources

- [MESH codebase] `crates/core/shell/src/shell/runtime/mod.rs` — full shell loop, `next_runtime_sleep()`, `dispatch_wayland()`, `render_components()` — HIGH confidence
- [MESH codebase] `crates/core/presentation/src/wayland_surface/backend.rs` — `dispatch_available()`, `attach_shm_buffer()`, `present_with_damage()`, `frame_pending` — HIGH confidence
- [MESH codebase] `crates/core/presentation/src/wayland_surface/handlers.rs` — `CompositorHandler::frame()`, `LayerShellHandler::configure()` — HIGH confidence
- [docs.rs/wayland-client/0.31.14] — `wl_surface.set_opaque_region()` API, `WlCompositor.create_region()` — HIGH confidence
- [Wayland protocol spec] — `wl_surface::set_opaque_region` copy semantics — HIGH confidence
