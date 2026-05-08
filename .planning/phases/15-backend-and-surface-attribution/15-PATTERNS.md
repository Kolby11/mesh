# Phase 15 Pattern Map

## Purpose

Map Phase 15 target files to the closest local analogs so backend/provider attribution extends the Phase 14 rolling profiler and existing backend runtime seams instead of creating a second diagnostics path.

## File Pattern Map

| Target | Role | Closest Analog | Pattern To Reuse |
|--------|------|----------------|------------------|
| `crates/core/foundation/debug/src/lib.rs` | Shared profiling contract extension | Existing `ProfilingSnapshot`, `ProfilingScopeSnapshot`, and `ProfilingSurfaceSnapshot` types | Extend the same typed debug payload with backend profiling structures instead of hiding provider attribution in stringly sample metadata. |
| `crates/core/shell/src/shell/runtime/profiling.rs` | Shell-owned collector/storage | Existing shell-wide and per-surface bounded accumulators | Add backend accumulators using the same fixed-count retention and snapshot-assembly pattern as Phase 14. |
| `crates/core/shell/src/shell/runtime/debug.rs` | Snapshot rollup boundary | Existing `build_debug_snapshot()` profiling emission | Serialize backend profiling beside shell and surface snapshots in the same debug-only payload. |
| `crates/core/shell/src/shell/backend/spawn.rs` | Provider/service identity bridge | Existing backend event forwarding for `Update`, `CommandResult`, and lifecycle events | Reuse the bridge as the place where backend-originated events already carry interface and provider identity into the shell. |
| `crates/core/shell/src/shell/runtime/mod.rs` | Message-drain runtime seam | Existing `ShellMessage` receive loop and profiling timing around runtime updates | Reuse the shell message drain when attributing backend update traffic as accepted shell work. |
| `crates/core/shell/src/shell/runtime/request.rs` | Backend command path | Existing `dispatch_service_command(...)` and Phase 14 runtime-update timing | Attach backend command-handling profiling at the same shell-owned dispatch seam that already knows interface, command, and active provider. |
| `crates/core/shell/src/shell/runtime/service_state.rs` | Publish/delivery fanout seam | Existing `broadcast_service_event(...)` and `record_latest_service_state(...)` flow | Measure state publish/delivery where accepted provider updates are validated, cached, and fanned out to components. |
| `crates/core/shell/src/shell/runtime/render.rs` | Surface rollup preservation | Existing surface stage aggregation and redraw accounting | Preserve stable per-surface totals and `module_id`/`surface_id` attribution while backend snapshots are added. |
| `crates/core/shell/src/shell/tests.rs` | Regression proof | Existing profiling snapshot and backend lifecycle tests | Extend shell tests rather than introducing a separate harness so attribution behavior stays covered next to the debug contract. |

## Data Flow

1. Backend runtime events cross into the shell through `backend/spawn.rs`.
2. The shell collector records backend stage samples tagged by interface and provider ID.
3. Service command dispatch records backend command-handling samples on the active provider.
4. Accepted service updates record publish/delivery work when they are cached and fanned out to frontend components.
5. Surface-local render timings continue to accumulate in the existing per-surface collector.
6. `build_debug_snapshot()` emits shell, per-surface, and backend summaries in one debug-only profiling payload.

## Constraints

- Do not overload `backend_runtimes` lifecycle entries with rolling timing data.
- Do not create an unbounded backend trace store separate from `runtime/profiling.rs`.
- Do not record backend samples for stale or rejected provider updates.
- Do not promise end-to-end backend-to-frontend interaction correlation that belongs to Phase 17.
- Do not weaken the existing per-surface snapshot contract while adding backend attribution.
