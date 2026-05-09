# Phase 14 Pattern Map

## Purpose

Map Phase 14 target files to the closest local analogs so profiling work extends existing shell debug and render-runtime patterns instead of introducing a parallel diagnostics path.

## File Pattern Map

| Target | Role | Closest Analog | Pattern To Reuse |
|--------|------|----------------|------------------|
| `crates/core/foundation/debug/src/lib.rs` | Shared debug snapshot contract | Current `DebugSnapshot` and `DebugOverlayState` definitions | Extend the existing crate boundary with new typed profiling payloads instead of adding a second snapshot transport. |
| `crates/core/shell/src/shell/types.rs` | Shell-owned request/control contract | Existing `CoreRequest::ToggleDebugOverlay` and `CycleDebugTab` variants | Add profiling control as another explicit shell request in the same enum and keep request handling typed. |
| `crates/core/shell/src/shell/runtime/request.rs` | Request application and debug state mutation | Existing `ToggleDebugOverlay` / `CycleDebugTab` match arms | Reuse the shell-owned request mutation path for profiling enable/reset semantics. |
| `crates/core/shell/src/shell/ipc.rs` | Running-shell control path | Existing `shell:debug_overlay` and `shell:debug_cycle_tab` commands | Add a profiling IPC command beside current debug commands rather than a new config channel. |
| `crates/tools/cli/src/main.rs` | Developer-facing debug CLI entrypoint | Existing `mesh-shell debug` command | Extend the current debug CLI surface instead of creating a standalone profiling CLI family. |
| `crates/core/shell/src/shell/runtime/mod.rs` | Main-loop shell stage ownership | Existing ordered event loop in `run()` | Use the main loop as the shell-wide stage root for runtime update and render-cycle timing. |
| `crates/core/shell/src/shell/runtime/wayland.rs` | Input handling stage seam | Existing event routing and shell-global shortcut handling | Measure input timing around the real Wayland dispatch path before component requests are drained. |
| `crates/core/shell/src/shell/runtime/render.rs` | Outer per-surface render/present span | Existing `render_components()` flow | Reuse the one-loop-per-surface structure to attach total render, present, and redraw accounting. |
| `crates/core/shell/src/shell/component/rendering.rs` | Build/style/layout stage seam | Existing `build_tree -> restyle_subtree -> compute_with_measurer` pipeline | Add fine-grained stage timers where the sub-stages actually execute instead of inferring them from outer spans. |
| `crates/core/shell/src/shell/component/shell_component.rs` | Surface paint-stage seam | Existing `paint()` method | Keep paint timing attached to the actual buffer paint path and content-measurement workflow. |
| `crates/core/shell/src/shell/runtime/debug.rs` | Snapshot rollup boundary | Existing `build_debug_snapshot()` implementation | Aggregate profiling runtime state into stable debug snapshot payloads here. |
| `crates/core/shell/src/shell/tests.rs` | Shell-owned regression proof | Existing debug shortcut, IPC, and debug snapshot tests | Extend these tests for profiling toggle, snapshot, and bounded collection contracts instead of inventing a separate harness. |

## Data Flow

1. Debug-only profiling control enters through `CoreRequest`, IPC, and CLI.
2. Shell-owned profiling state lives on `Shell` and/or a dedicated runtime helper module.
3. Wayland dispatch, request application, and component render/painters record shell-wide and per-surface stage timings.
4. `build_debug_snapshot()` converts that runtime state into a stable `DebugSnapshot` profiling payload.
5. Later phases can render the same payload without redefining the collection model.

## Constraints

- Do not create a profiler subsystem that bypasses `mesh-core-debug`.
- Do not put profiling ownership into frontend modules or renderer-global singletons.
- Do not infer `tree build`, `style/restyle`, or `layout` from coarse outer spans when the exact seams already exist in `component/rendering.rs`.
- Do not require Phase 14 to ship the inspector UI; this phase's contract is the runtime model and measurement hooks.
