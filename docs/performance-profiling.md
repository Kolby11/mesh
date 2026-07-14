# Profiling the MESH shell

MESH provides an optimized Cargo profile with symbol and source-line data and a
launcher for CPU and allocation profiling. Captures are written under the
ignored `profiles/` directory.

The first profiling build needs several GiB of free space. The launcher checks
for 4 GiB before Cargo starts; set `CARGO_TARGET_DIR` to a larger filesystem or
remove stale build artifacts when the check fails.

Enter the development environment and stop any existing MESH shell before each
capture:

```bash
nix develop
./target/debug/mesh-shell ipc shell:shutdown  # when a dev shell is running
```

## CPU flamegraph

```bash
./tools/profile-shell cpu
```

Interact with the profiled shell and exercise one focused workflow. Press
Ctrl-C after 10-30 seconds. Open the timestamped SVG in `profiles/cpu/`.
Wide frames consume more sampled CPU time. Read from the bottom upward to see
which caller led to a hot function. For hardware counters and annotated source,
run a manual `perf record` capture and open its `perf.data` in Hotspot.

Linux may deny sampling when `kernel.perf_event_paranoid` is restrictive. The
script reports that error rather than changing system security settings.

## Live CPU and memory

```bash
./tools/profile-shell live
```

The command builds MESH with the opt-in `perf-tracy` feature, starts the Tracy
profiler, and runs the shell. Select the discovered localhost MESH client in
Tracy. The timeline shows the instrumented build, restyle, layout, retained-tree,
display-list, paint, presentation, input, handler, and Lua synchronization
spans. Open Tracy's Memory view for live allocation rate, active allocations,
allocation call stacks, and memory usage over time.

The Tracy client uses on-demand, localhost-only mode. Profiling code and the
allocation wrapper are absent from normal builds. Allocation call stacks are
limited to 16 frames to keep live overhead bounded.

## Allocation heat map

```bash
./tools/profile-shell memory
```

After exercising one workflow, press Ctrl-C and open the generated capture:

```bash
heaptrack_gui profiles/memory/<capture>.gz
```

Use the flamegraph for allocation call stacks, the bottom-up view for allocation
hotspots, and the consumed/peak/leaked columns to distinguish temporary churn
from retained memory. Heaptrack adds substantial overhead, so compare allocation
counts and bytes rather than frame latency from this run.

## Repeatable captures

Profile one workload at a time: idle, pointer movement, scrolling, text update,
popover open/close, animation, theme reload, or resize. Keep duration and input
roughly constant between before/after captures. Use the built-in shell profiler
alongside system captures when stage attribution is needed:

```bash
./target/profiling/mesh-shell debug profiling
```

The system profiler identifies hot native call stacks; the built-in profiler
attributes work to MESH render stages and surfaces.

The debug inspector's Benchmark view exposes the stable canonical profile IDs
and their stage/target guidance:

| Profile ID | Target | Primary evidence |
| --- | --- | --- |
| `idle` | shell scheduler | scheduler idle duration and redraw count |
| `pointer_update` | navigation-bar audio controls | input/runtime handling, then layout/paint |
| `text_update` | settings text controls | input/runtime handling, then tree build/text shaping |
| `scroll` | settings surface | input handling, then layout/paint |
| `icon_grid` | debug inspector | icon raster/paint traversal, then paint |
| `animation` | navigation bar | restyle/runtime handling, then paint |
| `theme_reload` | active theme + navigation bar | tree build/restyle, then layout/render |
| `resize` | navigation bar | layout, then paint/present |

Start a fresh profiling session for each profile so aggregated stage summaries
contain only that workload. The older hover, surface-open/close, keyboard, and
backend-update scenarios remain available for interaction regressions.
