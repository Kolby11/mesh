## Plan 02 Summary — LayerShellBackend::wait_for_events

**Status**: complete

### Changes
- `crates/core/presentation/src/wayland_surface/backend.rs`: Added `LayerShellBackend::wait_for_events(deadline, eventfd_fd)` method implementing:
  - Flush + drain pending events (non-blocking)
  - `prepare_read()` with early return if events already pending
  - Blocking `poll()` on Wayland fd with deadline timeout (i32::MAX truncated)
  - Non-blocking `poll(0)` on eventfd to detect IPC/backend signals
  - `rustix::io::read()` to consume eventfd counter
  - `read_guard.read()` to ingest ready Wayland data
  - Typed `WaitResult` return with correct `WaitReason`
  - Final `dispatch_pending()` + `release_expired_surface_focus_grab()`

### Acceptance
- `dispatch_available()` unchanged (still 1 occurrence) ✓
- `prepare_read` count 4 (dispatch_available + wait_for_events with Some/None branches) ✓
- `eventfd_fd` referenced ≥3 times ✓
- All three `WaitReason` variants used in method body ✓
- `timeout.as_millis()` overflow-safe truncation ✓
- `rustix::io::read` for eventfd counter consumption ✓
- `cargo check -p mesh-core-presentation` passes ✓
- `cargo test -p mesh-core-presentation` passes (8 tests) ✓
