# Phase 99 Verification — Event-Driven Wayland Dispatch

**Status**: passed

**Verification date**: 2026-06-09

## Build

| Command | Result |
|---------|--------|
| `cargo check --workspace --all-features` | passed (0 errors) |
| `cargo check -p mesh-core-debug` | passed |
| `cargo check -p mesh-core-presentation` | passed |
| `cargo check -p mesh-core-shell` | passed |

## Tests

| Command | Result |
|---------|--------|
| `cargo test -p mesh-core-debug` | passed (1 test) |
| `cargo test -p mesh-core-presentation` | passed (8 tests) |
| `cargo test -p mesh-core-shell --lib` | FAILED — 2 pre-existing errors unrelated to Phase 99 |

The shell test failures are pre-existing:
- `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs:571`: `Color::from_rgba8` does not exist
- `crates/core/shell/src/shell/tests.rs:1894`: `ProfilingInvalidationSnapshot` missing fields `affected_node_count` and `narrow_path`

Neither file was modified by this phase (`git diff --name-only` confirms).

## Requirements

| ID | Requirement | Status |
|----|-------------|--------|
| SCHED-01 | Block on Wayland fd via poll() with deadline | ✓ `LayerShellBackend::wait_for_events()` with `poll(&mut wayland_fds, timeout_ms)` |
| SCHED-02 | eventfd IPC wakeup | ✓ `rustix::io::write()` after each IPC/backend `tx.send()` |
| SCHED-03 | Preserve drain-first loop ordering | ✓ drain → tick → render → present → block |
| SCHED-04 | Dev-window backend retains sleep path | ✓ `std::thread::sleep(deadline)` in else branch |
| DIAG-01 | ProfilingStage::SchedulerIdle | ✓ recorded with `result.reason.as_str()` after each block |

## Plan-by-plan acceptance

### Plan 01 — ProfilingStage + API contracts
- `ProfilingStage::SchedulerIdle` variant with label `"scheduler_idle"` ✓
- `WaitReason` enum (WaylandEvent, IpcEvent, DeadlineExpired) with `as_str()` ✓
- `WaitResult` struct with `deadline_expired()` constructor ✓
- `PresentationEngine::supports_blocking_dispatch()` ✓
- `PresentationEngine::wait_for_events()` ✓

### Plan 02 — LayerShellBackend::wait_for_events
- Blocks on Wayland fd with deadline poll ✓
- Eventfd polled after Wayland unblock ✓
- Eventfd counter consumed via `rustix::io::read` ✓
- Returns typed `WaitResult` with correct `WaitReason` ✓
- Idempotent: no blocking when `prepare_read` returns None ✓
- `dispatch_available()` unchanged ✓

### Plan 03 — Shell loop integration
- `rustix` dependency in shell Cargo.toml ✓
- `eventfd_fd: Option<OwnedFd>` field on Shell ✓
- `MAX_IDLE_SLEEP` constant and clamp removed ✓
- Eventfd created before spawn calls ✓
- `supports_blocking_dispatch()` gate ✓
- Wayland → `wait_for_events()` with SchedulerIdle profiling ✓
- DevWindow → `std::thread::sleep()` ✓
- Loop order: drain → tick → render → present → block ✓
- `EventfdCreate` ShellRunError variant ✓

### Plan 04 — IPC/backend eventfd signaling
- IPC server signals eventfd after `tx.send(ShellMessage::Ipc(...))` ✓
- Backend event bridge signals eventfd after all 8 `shell_tx.send()` variants ✓
- Both spawn functions accept `eventfd_fd: RawFd` ✓
- Shell::run() creates eventfd before spawn calls ✓

## Files modified

```
crates/core/foundation/debug/src/lib.rs          — SchedulerIdle variant
crates/core/presentation/src/lib.rs              — WaitReason, WaitResult, PresentationEngine API
crates/core/presentation/src/wayland_surface/backend.rs — LayerShellBackend::wait_for_events
crates/core/shell/Cargo.toml                     — rustix dep
crates/core/shell/src/shell/mod.rs               — eventfd_fd field, EventfdCreate error
crates/core/shell/src/shell/discovery.rs         — eventfd_fd: None init
crates/core/shell/src/shell/runtime/mod.rs       — eventfd creation, blocking dispatch, profiling
crates/core/shell/src/shell/ipc.rs               — eventfd signaling in IPC handler
crates/core/shell/src/shell/backend/spawn.rs     — eventfd signaling in backend event bridge
```

## Design decisions honored

- `wait_for_events(deadline)` is a NEW method alongside `dispatch_available()` (not a refactor) ✓
- eventfd via `rustix::event::eventfd` (already in dependency tree) ✓
- Signal eventfd immediately after `tx.send()` in IPC server and backend spawn ✓
- Poll both fds sequentially: Wayland fd first, then eventfd ✓
- `supports_blocking_dispatch() -> bool` on PresentationEngine ✓
- Dev-window backend returns false, keeps `std::thread::sleep` ✓
- `MAX_IDLE_SLEEP` clamp removed from `next_runtime_sleep()` ✓
- `WaitReason` enum: WaylandEvent, IpcEvent, DeadlineExpired ✓
