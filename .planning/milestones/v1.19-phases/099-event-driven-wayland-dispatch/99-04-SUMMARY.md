## Plan 04 Summary — IPC/backend eventfd signaling

**Status**: complete

### Changes
- `crates/core/shell/src/shell/ipc.rs`:
  - Added `eventfd_fd: RawFd` parameter to `spawn_ipc_server()` and `handle_ipc_client()`
  - Signal eventfd via `rustix::io::write(&evfd, &1u64.to_ne_bytes())` immediately after `tx.send(ShellMessage::Ipc(...))`
  - Updated all 6 test invocations to pass throwaway eventfd via helper `test_eventfd()`
- `crates/core/shell/src/shell/backend/spawn.rs`:
  - Added `eventfd_fd: RawFd` parameter to `spawn_backend_modules()` and `spawn_backend_candidate()`
  - Signal eventfd via `rustix::io::write()` after each of 8 `shell_tx.send()` calls in the event bridge loop
  - All 8 `BackendServiceEvent` variants covered
- `crates/core/shell/src/shell/runtime/mod.rs`:
  - Eventfd created before spawn calls in `run()`
  - `eventfd_raw` derived from stored `OwnedFd` and passed to both spawn functions

### Acceptance
- IPC eventfd signal: 1 occurrence, immediately after `tx.send(ShellMessage::Ipc` ✓
- Backend eventfd signals: 8 occurrences (one per `shell_tx.send`) ✓
- All signals write value 1 (u64 to_ne_bytes) ✓
- `BorrowedFd::borrow_raw` wrapped in `unsafe` with local binding for lifetime ✓
- `spawn_backend_modules` and `spawn_ipc_server` accept `eventfd_fd: RawFd` ✓
- Shell::run() creates eventfd before spawn calls and passes raw fd ✓
- `cargo check -p mesh-core-shell` passes ✓
