## Plan 01 Summary — ProfilingStage + API contracts

**Status**: complete

### Changes
- `crates/core/foundation/debug/src/lib.rs`: Added `SchedulerIdle` variant to `ProfilingStage` enum (after `RuntimeUpdateHandling`) with label `"scheduler_idle"`
- `crates/core/presentation/src/lib.rs`: Added `WaitReason` enum (WaylandEvent, IpcEvent, DeadlineExpired), `WaitResult` struct, `supports_blocking_dispatch()` method, `wait_for_events()` method on `PresentationEngine`

### Acceptance
- `ProfilingStage::SchedulerIdle` compiles with label `"scheduler_idle"` ✓
- `WaitReason` three variants each with `as_str()` ✓
- `WaitResult` with `deadline_expired()` constructor ✓
- `supports_blocking_dispatch()` returns true for WaylandSurface, false for DevWindow ✓
- `wait_for_events()` delegates to backend; DevWindow returns DeadlineExpired ✓
- `cargo check -p mesh-core-debug` passes ✓
- `cargo check -p mesh-core-presentation` passes ✓
- `cargo test -p mesh-core-debug` passes ✓
- `cargo test -p mesh-core-presentation` passes (8 tests) ✓
