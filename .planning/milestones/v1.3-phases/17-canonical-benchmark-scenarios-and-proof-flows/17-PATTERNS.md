# Phase 17: Pattern Map

**Generated:** 2026-05-09
**Status:** Ready for planning

## Scope

Phase 17 extends the existing debug-only profiling inspector with canonical benchmark scenario data, launch/proof flows, a compact benchmark UI, and final backend-to-frontend correlation proof.

## Files to Modify

| File | Role | Closest Analog | Notes |
|------|------|----------------|-------|
| `crates/core/foundation/debug/src/lib.rs` | Typed debug/profiling contract | Existing `ProfilingSnapshot`, `ProfilingSurfaceSnapshot`, `ProfilingBackendSnapshot`, `DebugInspectorView` | Add benchmark scenario/result types beside current debug snapshot types. Keep labels as stable string methods. |
| `crates/core/shell/src/shell/runtime/debug.rs` | Debug payload assembly | Existing `profiling_snapshot_json`, `profiling_surface_snapshot_json`, `profiling_backend_snapshot_json` | Serialize benchmark data into `mesh.debug` beside `profiling`. Preserve disabled-mode null/silent behavior. |
| `crates/core/shell/src/shell/runtime/profiling.rs` | Rolling sample source | Existing shell/surface/backend accumulators | Reuse summaries; avoid persistence or trace storage. Add helper only if needed for latest-run extraction. |
| `crates/core/frontend/host/src/lib.rs` | `CoreRequest` contract | Existing `ToggleDebugProfiling`, `ToggleSurface`, `ActivatePopover` requests | Add benchmark run request only if scenario launch cannot be represented by existing shell events alone. |
| `crates/core/shell/src/shell/service.rs` | Frontend event to request mapping | Existing `shell.toggle-debug-profiling` mapping | Map `shell.run-debug-benchmark` to a debug-scoped request if launch actions are added. |
| `crates/core/shell/src/shell/runtime/request.rs` | Request execution | Existing `ToggleDebugProfiling`, `ToggleSurface`, `ActivatePopover`, service command handling | Keep benchmark execution explicit and generic. Record shell profiling through existing request timing path. |
| `crates/core/shell/src/shell/ipc.rs` | Optional IPC parity | Existing `shell:debug_profiling` and `shell:toggle_surface:<id>` parsing | Add `shell:debug_benchmark:<scenario_id>` only if needed. |
| `modules/frontend/debug-inspector/src/main.mesh` | Inspector state and handlers | Existing profiling state sync, view switching, `onToggleProfiling` | Read benchmark payload safely, provide fallback row values, publish run events. |
| `modules/frontend/debug-inspector/src/components/benchmark-view.mesh` | Benchmark UI | Existing scaffold cards and other inspector view row/card components | Replace scaffold cards with five stable compact scenario rows per UI-SPEC. |
| `crates/core/shell/src/shell/tests.rs` | Shell contract tests | Existing `profiling_*` and `debug_snapshot_*` tests | Add benchmark ids, debug payload JSON, disabled behavior, request mapping, backend/surface correlation tests. |
| `crates/core/shell/src/shell/component/tests.rs` | Real-surface UI tests | Existing `debug_inspector_*` tests | Assert benchmark rows, controls, fallback states, and populated result state render on real `.mesh` surface. |

## Reusable Patterns

### Typed Contract Pattern

Existing debug structs live in `crates/core/foundation/debug/src/lib.rs` and expose labels through enum methods. Phase 17 should follow that shape:

- Rust enum for stable scenario ids/statuses.
- Structs for snapshot/result rows.
- `label()` / `as_str()` style helpers for JSON serialization.
- No UI-specific string assembly in core types except stable labels and ids.

### Debug JSON Pattern

`crates/core/shell/src/shell/runtime/debug.rs` uses small helper functions:

- `debug_service_payload`
- `profiling_snapshot_json`
- `profiling_scope_snapshot_json`
- `profiling_surface_snapshot_json`
- `profiling_backend_snapshot_json`

Benchmark serialization should add parallel helpers such as `benchmark_snapshot_json` and `benchmark_scenario_json` rather than inlining JSON assembly into `build_debug_snapshot`.

### Inspector `.mesh` Pattern

The debug inspector parent owns service payload normalization in `onRender()`, then passes primitive text/hidden props into child views. Phase 17 should keep that pattern:

- Parent reads `debug_service.benchmarks`.
- Parent populates fixed variables for each of five rows.
- Child `BenchmarkView` receives explicit props and renders rows.
- Child does not call `require("@mesh/debug")` directly.

### Test Pattern

Shell tests use local helpers and direct assertions on `build_debug_snapshot()` or `latest_service_state`. Component tests use `real_frontend_module_component("@mesh/debug-inspector", debug_catalog())`, feed `ServiceEvent::Updated { service: "mesh.debug", source_module: "@mesh/core-debug", payload: json!(...) }`, paint, and inspect rendered text.

## Constraints for Executors

- Do not add benchmark persistence, trace capture, replay, export, or external telemetry.
- Do not make profiling start automatically when the inspector or benchmark view opens.
- Do not add audio-specific business logic to Rust core.
- Do not redesign existing overview/surfaces/backend views beyond small shared support needed by benchmark rows.
- Keep the UI at 320px inspector width using theme tokens only.
