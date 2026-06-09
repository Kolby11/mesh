# Domain Pitfalls

**Domain:** Event-Driven Wayland Frame Scheduler
**Researched:** 2026-06-09
**Confidence:** HIGH

## Critical Pitfalls

Mistakes that cause rewrites or major issues.

### Pitfall 1: Blocking Before Draining the Shell Message Channel

**What goes wrong:** The shell blocks on the Wayland fd (`poll()`) before draining the `mpsc::unbounded_channel` from IPC and backends. Shell messages (theme reloads, module reloads, backend state) sit in the channel until the Wayland timeout expires.

**Why it happens:** `wait_for_events()` is called at the bottom of the loop after `render_components()`, but if reordered to the top (trying to "wait first, then work"), IPC latency spikes to the poll timeout.

**Consequences:** Theme hot-reloads and IPC commands experience up to 16ms of added latency. Remote-control responsiveness degrades. Backend state updates pile up.

**Prevention:** Keep the existing loop order — drain shell messages, tick components, render, present, *then* block. Do not move `wait_for_events()` before `handle_shell_message()`.

**Detection:** Profile `shell_message_backlog_likely` — if it's ever true after a blocking wait, the order is wrong.

### Pitfall 2: wl_region Leak on set_opaque_region

**What goes wrong:** Each frame creates a new `WlRegion`, calls `set_opaque_region()`, but never destroys the region. The compositor tracks every region object until disconnect.

**Why it happens:** The `wl_surface.set_opaque_region` docs say "the wl_region object can be destroyed immediately" (copy semantics) — but if this isn't read carefully, it's easy to miss the destroy step.

**Consequences:** Over hours of uptime at 60fps, 216,000 leaked region objects per hour. Compositors may reject new objects, crash, or slow down significantly.

**Prevention:** Always `region.destroy()` after `set_opaque_region()`. Add a debug-only leak check that asserts `wl_region` objects are destroyed within the same frame.

**Detection:** Monitor Wayland object counts in compositor debug tools. A monotonically increasing `wl_region` count is a leak.

### Pitfall 3: Setting Opaque Region on Semi-Transparent Surfaces

**What goes wrong:** A surface uses theme-defined alpha (e.g., `rgba(0, 0, 0, 0.85)` for a panel background). The opaque region computation sees the background fill and marks it opaque, telling the compositor it can skip compositing surfaces behind it.

**Why it happens:** The opaque region computation incorrectly uses the fill color's alpha channel as a binary "opaque or not" check without considering that 0.85 alpha means 15% of background bleeds through.

**Consequences:** Compositor skips rendering surfaces behind the "opaque" region — users see stale framebuffer contents or rendering artifacts where transparent surfaces should show underlying content.

**Prevention:** Only mark regions as opaque when the background fill alpha is exactly 1.0 (255). Any alpha < 1.0 means the region is NOT opaque. Theme tokens with alpha channels (semi-transparent panels, glass effects) must not produce opaque region hints.

**Detection:** Visual inspection: if a semi-transparent panel shows correct content behind it on Sway but garbage/black on Hyprland, the opaque region is wrong.

## Moderate Pitfalls

### Pitfall 4: Frame Callback Not Requested on Every Commit

**What goes wrong:** `wl_surface.frame()` is only called in `attach_shm_buffer()` — which is only called when there's a buffer to present. If a commit happens without a buffer (e.g., a configure-only commit or a hide), no frame callback is requested, and the shell blocks for the full timeout.

**Why it happens:** The current codebase already handles this: `hide()` calls `wl_surface.attach(None, 0, 0)` then `commit()` without requesting a frame callback.

**Consequences:** Surface hide transitions have up to 16ms of unnecessary latency. Polish issue, not correctness.

**Prevention:** After a hide commit, either: (a) skip the blocking wait entirely, or (b) request a frame callback even on null-buffer commits. Option (a) is simpler and preferred.

### Pitfall 5: Dev-Window Backend Blocking

**What goes wrong:** `PresentationEngine::wait_for_events()` is implemented for `Backend::DevWindow` by calling `thread::sleep()`, making the dev window unresponsive.

**Why it happens:** The dev-window backend (minifb) doesn't expose a Wayland fd.

**Consequences:** Dev window freezes or has 16ms input latency.

**Prevention:** Match on `Backend::DevWindow` and return immediately (no-op). Minifb's internal update loop already handles its own event polling.

## Minor Pitfalls

### Pitfall 6: poll() Timeout Truncation

**What goes wrong:** `Duration::as_millis()` returns `u128`. Truncating to `i32` for `rustix::event::poll()` silently wraps on very large values.

**Why it happens:** The deadline is capped at `MAX_IDLE_SLEEP = 16ms`, well within i32 range. But a future change removing the cap could overflow.

**Consequences:** None currently (16ms << i32::MAX). Future risk only.

**Prevention:** `timeout.as_millis().min(i32::MAX as u64) as i32`.

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Scheduling: fd blocking | Blocking order (pitfall 1) | Keep drain-then-wait order |
| Scheduling: deadline calculation | Removing MAX_IDLE_SLEEP cap | Keep the 16ms cap — it's safety, not legacy |
| Opaque region: display list walking | Alpha threshold (pitfall 3) | Only mark alpha=255 as opaque; test with alpha=254 background |
| Opaque region: wl_region lifecycle | Region leak (pitfall 2) | Mandatory `destroy()` after `set_opaque_region()` |
| Integration: dev-window backend | Blocking on wrong backend (pitfall 5) | DevWindow returns immediately from `wait_for_events()` |

## Sources

- [MESH codebase] `crates/core/presentation/src/wayland_surface/backend.rs` — `hide()`, `attach_shm_buffer()`, `dispatch_available()`, `MAX_FRAME_CALLBACK_WAIT` — HIGH confidence
- [MESH codebase] `crates/core/shell/src/shell/runtime/mod.rs` — `run()` loop order, `next_runtime_sleep()`, `MAX_IDLE_SLEEP` — HIGH confidence
- [docs.rs/wayland-client/0.31.14] — `wl_surface.set_opaque_region()` copy semantics — HIGH confidence
- [Wayland protocol spec] — `wl_region` lifecycle — HIGH confidence
- [Community wisdom] — Common Wayland client bugs: region leaks, opaque region on transparent surfaces — MEDIUM confidence
