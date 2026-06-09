# Architecture Patterns

**Domain:** Event-Driven Wayland Frame Scheduler
**Researched:** 2026-06-09
**Confidence:** HIGH

## Recommended Architecture

```
┌──────────────────────────────────────────────────────────────┐
│ Shell::run() main loop                                       │
│                                                              │
│  while !shutting_down {                                      │
│    // Phase 1: Compute deadline                              │
│    let deadline = next_runtime_sleep(backlog);                │
│                                                              │
│    // Phase 2: Non-Wayland work                              │
│    reload_theme(); reload_locale(); reload_modules();         │
│                                                              │
│    // Phase 3: Drain pending Wayland events (non-blocking)    │
│    dispatch_wayland();  ← polls + processes events            │
│    handle_shell_messages();                                   │
│    tick_components(); drain_requests(); render_components();   │
│                                                              │
│    // Phase 4: Block on Wayland fd OR deadline (NEW)          │
│    presentation_engine.wait_for_events(deadline);              │
│    // ^ replaces: std::thread::sleep(sleep_for)               │
│    // ^ wakes on: frame callback, input, compositor events    │
│  }                                                           │
└──────────────────────────────────────────────────────────────┘
```

### Component Boundaries

| Component | Responsibility | Communicates With |
|-----------|---------------|-------------------|
| `Shell::next_runtime_sleep()` | Compute deadline from shell-message backlog, pending events, render needs, reload timers, throttled commands, surface transitions | N/A (pure computation) |
| `PresentationEngine::wait_for_events()` | Block on Wayland connection fd until timeout or data arrives | `LayerShellBackend::wait_for_events()` |
| `LayerShellBackend::wait_for_events()` | `flush()` → `dispatch_pending()` → `prepare_read()` → `poll(fd, timeout)` → `read()` → `dispatch_pending()` | Wayland connection fd, `EventQueue` |
| `LayerShellBackend::present_with_damage()` (existing, modified) | Compute opaque region from retained display list, create `wl_region`, call `set_opaque_region` before commit | `SurfaceEntry`, `WlCompositor`, retained display list |
| `SurfaceEntry::compute_opaque_region()` (new) | Union of fully-opaque background rects from the retained display list for this surface | Retained display list (via `mesh_core_render`) |
| `CompositorHandler::frame()` (existing) | Handle `wl_callback.done` → clear `frame_pending`, clear `frame_pending_since` | `SurfaceEntry` |

### Data Flow

```
1. Shell computes deadline from all work sources
2. Shell performs all non-blocking work (dispatch, tick, render)
3. Shell calls present() → backend attaches buffer, requests frame callback,
   sets opaque region, commits
4. Shell calls wait_for_events(deadline) → blocks on Wayland fd
5. Wayland compositor sends frame callback (or input event, or any other event)
   → fd becomes readable → poll returns
6. Backend reads events, dispatches them (frame callback clears frame_pending)
7. Shell loop continues, processes new events
```

## Patterns to Follow

### Pattern 1: Deadline-Driven Blocking Dispatch

**What:** Replace `std::thread::sleep()` with `poll()` on the Wayland connection fd, using the already-computed deadline as the poll timeout.

**When:** At the bottom of the shell loop, after all non-blocking work is done and before the next iteration.

**Why:** The connection fd becomes readable whenever the compositor sends *any* event — frame callbacks, input, configure, output changes. Blocking on the fd means the shell sleeps until there's actually something to do, rather than waking up on a fixed 16ms timer to poll for work that may not exist. This eliminates idle CPU burn while preserving responsiveness.

**Integration:** The existing `dispatch_available()` already has the full prepare_read/poll/read/dispatch loop. The only change is passing a non-zero timeout to `poll()`.

**Example:**
```rust
// In LayerShellBackend (new method)
fn wait_for_events(&mut self, timeout: Duration) -> Result<(), PresentationError> {
    self.event_queue.flush()?;
    self.event_queue.dispatch_pending(&mut self.state)?;

    let Some(read_guard) = self.event_queue.prepare_read() else {
        // Events already pending — don't block, let caller process them
        return Ok(());
    };

    let fd = read_guard.connection_fd();
    let mut fds = [PollFd::new(&fd, PollFlags::IN | PollFlags::ERR | PollFlags::HUP)];
    let timeout_ms = timeout.as_millis().min(i32::MAX as u64) as i32;

    match poll(&mut fds, timeout_ms) {
        Ok(0) | Err(rustix::io::Errno::INTR) => {
            // Timeout or interrupted — nothing to read
            return Ok(());
        }
        Ok(_) => {
            match read_guard.read() {
                Ok(0) => {} // EOF — compositor disconnected
                Ok(_) => {} // Data read
                Err(WaylandError::Io(err)) if err.kind() == ErrorKind::WouldBlock => {}
                Err(err) => return Err(...)
            }
        }
        Err(err) => return Err(...)
    }

    self.event_queue.dispatch_pending(&mut self.state)?;
    Ok(())
}
```

### Pattern 2: Opaque Region from Retained Display List

**What:** Before `wl_surface.commit()`, compute the union of fully-opaque background rects from the retained display list, create a `wl_region`, and call `wl_surface.set_opaque_region()`.

**When:** In `SurfaceEntry::attach_shm_buffer()` (or a new method called just before `layer_surface.commit()`).

**Why:** Telling the compositor which regions are fully opaque lets it skip compositing occluded surfaces underneath, reducing GPU work and improving frame pacing. This is a standard Wayland optimization and is expected by compositors like Sway/Hyprland for shell surfaces.

**Example:**
```rust
fn set_opaque_region_for_opaque_rects(
    &self,
    compositor: &wl_compositor::WlCompositor,
    qh: &QueueHandle<State>,
    opaque_rects: &[DamageRect],
) {
    if opaque_rects.is_empty() {
        self.layer_surface.wl_surface().set_opaque_region(None);
        return;
    }
    let region = compositor.create_region(qh, ());
    for rect in opaque_rects {
        region.add(rect.x as i32, rect.y as i32,
                   rect.width as i32, rect.height as i32);
    }
    self.layer_surface.wl_surface().set_opaque_region(Some(&region));
    region.destroy(); // Copy semantics — safe to destroy immediately
}
```

### Pattern 3: Frame Callback as Render Permit (Already Wired)

**What:** The existing code already requests `wl_surface.frame()` in `attach_shm_buffer()` and handles the callback in `CompositorHandler::frame()`. The scheduler just needs to *block on* these callbacks by waiting on the connection fd.

**What changes:** The `components_have_ready_render_work()` check in `next_runtime_sleep()` already returns `Duration::ZERO` when no frame callback is pending. The scheduler adds the blocking wait *after* all work so the shell doesn't spin when there's nothing to render.

## Anti-Patterns to Avoid

### Anti-Pattern 1: Blocking Before Processing All Non-Wayland Work

**What:** Calling `wait_for_events()` before draining shell messages, ticking components, or rendering.

**Why bad:** Shell messages from IPC arrive through `mpsc::channel`, not the Wayland fd. If you block on Wayland before draining the channel, you introduce latency proportional to the poll timeout for IPC-triggered work (theme reloads, module reloads, backend state changes).

**Instead:** Always drain all local channels, render, and present *before* blocking on Wayland. The loop order should be: drain shell messages → tick components → render → present → block on Wayland.

### Anti-Pattern 2: Recreating wl_region Objects Every Frame Without Destroying

**What:** Creating `wl_region` objects each frame and leaking them instead of destroying after `set_opaque_region`.

**Why bad:** Leaks Wayland protocol objects — the compositor tracks them until the client disconnects. Over hours of runtime, this wastes compositor memory and can trigger resource limits.

**Instead:** `wl_surface.set_opaque_region` has copy semantics per the spec — destroy the `WlRegion` immediately after the call.

### Anti-Pattern 3: Using `thread::sleep` as Fallback for Non-Wayland Backends

**What:** Adding `if deadline.is_zero() { thread::sleep(deadline) }` as a fallback when no Wayland fd is available.

**Why bad:** The dev-window backend already has its own event loop (minifb's internal polling). Adding `thread::sleep` to it would make the dev window unresponsive.

**Instead:** The `PresentationEngine::wait_for_events()` should be a no-op for `Backend::DevWindow`. Only the `Backend::WaylandSurface` variant uses fd-blocking.

## Scalability Considerations

| Concern | At 1 surface | At 10 surfaces | At 50 surfaces |
|---------|-------------|----------------|---------------|
| fd blocking | Single fd poll — trivial | Still single fd (all surfaces share one Wayland connection) — no change | Same — one connection, one fd |
| Frame callbacks | 1 callback per frame per surface | 10 callbacks per frame; all arrive on same fd, dispatched in batch | 50 callbacks; still dispatched in batch — no per-surface polling cost |
| Opaque region computation | O(num opaque rects per surface) — typically <10 | 10 × ~5 rects = 50 region operations | 50 × ~5 rects = 250 region operations; still trivial (<1ms) |
| Deadline computation | O(1) per surface check | O(10) — unchanged from current `next_runtime_sleep()` | O(50) — still negligible |

## Sources

- [MESH codebase] `crates/core/presentation/src/wayland_surface/backend.rs` — `dispatch_available()` pattern, `attach_shm_buffer()` frame callback request, `frame_pending` tracking — HIGH confidence
- [MESH codebase] `crates/core/presentation/src/wayland_surface/handlers.rs` — `CompositorHandler::frame()` handling — HIGH confidence
- [MESH codebase] `crates/core/shell/src/shell/runtime/mod.rs` — `next_runtime_sleep()`, `run()` loop structure — HIGH confidence
- [docs.rs/wayland-client/0.31.14] — `wl_surface.set_opaque_region()`, `WlCompositor.create_region()`, `EventQueue.prepare_read()` — HIGH confidence
- [Wayland protocol spec] — `wl_surface::set_opaque_region` copy semantics — HIGH confidence
