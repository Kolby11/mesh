## Plan 03 Summary — Shell loop integration

**Status**: complete

### Changes
- `crates/core/shell/Cargo.toml`: Added `rustix = { version = "0.38", features = ["event"] }` dependency
- `crates/core/shell/src/shell/mod.rs`: Added `eventfd_fd: Option<std::os::unix::io::OwnedFd>` field to `Shell` struct; added `EventfdCreate(String)` variant to `ShellRunError`
- `crates/core/shell/src/shell/discovery.rs`: Initialized `eventfd_fd: None` in `Shell::new()`
- `crates/core/shell/src/shell/runtime/mod.rs`:
  - Removed `MAX_IDLE_SLEEP` constant and `.min(MAX_IDLE_SLEEP)` clamp from `next_runtime_sleep()`
  - Created eventfd via `rustix::event::eventfd(0, CLOEXEC | NONBLOCK)` at start of `run()`, before spawn calls
  - Replaced `std::thread::sleep(sleep_for)` with `supports_blocking_dispatch()` gate:
    - WaylandSurface → `PresentationEngine::wait_for_events(deadline, eventfd)` + `ProfilingStage::SchedulerIdle` profiling
    - DevWindow → `std::thread::sleep(deadline)` preserved
  - Loop order: drain → tick → render → present → block preserved

### Acceptance
- `rustix` dep added to shell Cargo.toml ✓
- `eventfd_fd: Option<OwnedFd>` field exists ✓
- `MAX_IDLE_SLEEP` fully removed (0 occurrences) ✓
- Deadline unclamped (single `saturating_duration_since`, no `.min()`) ✓
- `MIN_RUNTIME_SLEEP` preserved ✓
- Eventfd created before spawn calls ✓
- `supports_blocking_dispatch()` gate (1 occurrence) ✓
- `wait_for_events()` call (1 occurrence) ✓
- `ProfilingStage::SchedulerIdle` profiling record (1 occurrence) ✓
- `std::thread::sleep` preserved for dev-window path only (1 occurrence) ✓
- `EventfdCreate` variant in ShellRunError ✓
- `cargo check -p mesh-core-shell` passes ✓
