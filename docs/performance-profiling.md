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

## Session capture with an activity log

```bash
./tools/profile-shell session
./tools/profile-report profiles/session/<timestamp>
```

Records CPU stacks for the whole session while the shell logs its input
(scroll, click, key, hover) and, with the built-in stage profiler switched on
automatically, every render-stage timing. Use the shell normally, then press
Ctrl-C. The report splits the session into activity segments, and for each
activity it gives CPU use per thread, MESH stage timings and renders per second,
and the hottest functions by self and inclusive time. It is also written to
`report.md` in the session directory. Key presses that produce text are logged
without the key.

Stage timings do not break restyle down per style rule by default: timing every
rule match adds a `clock_gettime` pair per rule and inflates the restyle being
measured. Set `MESH_PROFILE_STYLE_RULES=1` for a capture that needs the
per-rule breakdown, and do not compare its restyle times with other captures.

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

## Per-render-pass allocation counts

Use the opt-in counting allocator when the question is which surface render
allocates, rather than which call stack owns an allocation:

```bash
./tools/profile-shell alloc
```

Open the debug inspector and start profiling. The Overview and Surfaces views
then report allocation operations and allocated bytes for each completed
surface render pass. Recent per-pass samples and cumulative allocation,
deallocation, and reallocation counters are also published through the
`mesh.debug` profiling payload.

Counters are thread-local to the shell render thread, so concurrent backend
runtime allocations do not get charged to a surface. MESH's own timing-record
bookkeeping is suspended around its profiler writes so it is not charged to the
render being observed. The counting allocator is absent from normal builds and
is intentionally separate from `perf-tracy`; choose `alloc` for pass-level
counts or `live` for Tracy call stacks.

## Node-visit accounting

With built-in profiling enabled, each surface's latest invalidation snapshot
includes `node_visits`. `profile-shell session` also logs one count per pass per
paint; `profile-report` reports per-surface totals, means and maxima separately
from timings. Counts include repeated examinations, not unique dirty IDs.
Zero means that an instrumented pass did no work. An empty map means profiling
was disabled. Counts cover synchronous frame preparation on the calling thread;
Taffy's internal solver, raster workers and backend work are excluded.

| Counter | What it counts |
| --- | --- |
| `restyle_scan`, `style_resolve` | Restyle traversal entries and snapshot-path steps versus actual style resolutions, including diagnostics |
| `layout_build`, `layout_sync`, `layout_write` | Taffy construction/reconciliation, retained input synchronization, and geometry write visits |
| `layout_fit`, `layout_extents`, `layout_discovery` | Fit-size correction, bottom-up descendant extent aggregation (one visit per node), and fingerprint discovery for layout inputs |
| `animation_sample`, `animation_previous_style`, `animation_lookup` | Sampling visits, previous-style capture visits, and child-path steps |
| `accessibility_normalize`, `accessibility_project` | Full/memo normalization traversal and local/snapshot semantic projections |
| `validation`, `annotation` | Tree validation and runtime/scroll annotation visits, including snapshot-path steps |
| `retained_discovery`, `retained_sync`, `retained_lookup` | Candidate discovery, selected-node fingerprint refresh, and child-path steps |
| `display_entries`, `display_ancestors`, `display_build` | Display-entry traversal, dirty-ancestor discovery, and paint-subtree visits |
| `child_surface_requests` | Child-surface discovery visits; emitted as separate call samples after paint |

These counters measure the named functions, not every operation in their
subsystems. For example, semantic child-text aggregation and reference lookup
are not counted as normalization visits. Pruned/cache-hit nodes still count
when a traversal examines them. Child requests run outside the paint scope,
so their paint-snapshot count is zero; use their separate session-log samples.

`tools/check-performance` runs
`animation_frame_visits_registered_nodes_and_full_diff_finds_no_omissions` as
a structural gate: one animated leaf plus 1,024 clean siblings must sample one
node, read one previous style, perform no retained discovery, and synchronize
at most three retained records. The full-diff verifier checks the result.
This gate covers the animation prototype, not all-pass proportionality.
`hover_frame_visits_scale_with_the_hovered_item_not_the_tree` gates
interaction frames the same way: hovering between two tiles beside 16 or
1,024 clean siblings must give identical annotation, restyle-scan,
retained-lookup, retained-sync and semantic-projection counts, with no
retained discovery. A twin forced onto the full walks must paint identical
pixels and computed styles, and a rebuild must reproduce them.
`keyed_list_one_item_write_does_not_patch_clean_siblings` checks that an
in-place item write patches one node with both 8 and 256 list items and emits
no tree-build stage. The companion keyed-list tests compare forced-rebuild
pixels and verify structural fallbacks. Key validation and binding-target
discovery still scan their collections/tree; this is a property-patch gate,
not an all-pass proportionality claim.
The glass dock and spectrum profile tests print visits per measured paint
alongside their timings. `MESH_BENCH_FORCE_TEMPLATE=1` disables template
patching/selective template builds in the spectrum profile for comparison.
Use profiling-disabled runs for timing claims about
instrumentation overhead.

## Live glass replay

```bash
MESH_LIVE_CPU=6 nix develop -c ./tools/check-performance --live
```

Opt-in; it is not part of the default run and has no baseline entries. It runs
the ignored test `live_glass_replay` (3 fresh processes, `MESH_LIVE_RUNS`
overrides) in the release `mesh-core-shell` test binary. The test starts the
real `Shell` with a private copy of `config/` (the `glass` profile), a private
IPC socket and state directory, on the compositor named by `WAYLAND_DISPLAY`.
It skips, passing, without a reachable compositor. Backends are not spawned;
fixed service payloads are injected, and input enters as synthetic window
events. The workloads run in order: bar clock ticks (8 s idle window), dock
hover sweep, dashboard open, 120 spectrum updates, Settings open, Settings
Appearance scroll. Each prints `MESH_PERF metric=live_<workload>_<surface>_...`
lines: render mean/p50/p90 of `total_surface_render`, frame counts, per-pass
visit means, first present/render after each open request, and boot first
present. Raster is CPU unless the test binary is built with `--features gpu`.
Timings are for visibility; the only failure is structural: every live dock
hover frame must stay within the per-pass maxima of the same frame class
(hover change or animation only) in the CPU fixture `glass_dock_hover_animation_profile`.
Like the live sweep, the fixture takes tile bounds after the shelf's entrance
animation, enters the dock from outside on each run and paints at 16 ms pacing,
so its envelope includes hover script patches and the dock's own `:hover`
transition.
Replay surfaces appear on the compositor for about a minute under the shipped
module ids as layer namespaces (not unique, so stop a running desktop shell first); the shell then shuts down and its private files are removed.

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

## D9 workload gates

CI runs `nix develop -c ./tools/check-performance`. To run just the six D9
budgets locally:

```bash
nix develop -c ./tools/check-performance --workloads-only
```

The ceilings live in `config/performance-baseline.tsv`. Missing, duplicate or
invalid measurements fail the command. The D9 runner builds one release test
binary, then runs each workload serially in a fresh process to isolate resource
workers and caches. Setup stays outside the timed samples. Timing gates
use p90 rather than a single frame or the fastest sample.

| Metric | Workload | Ceiling |
| --- | --- | --- |
| `d9_idle_wakeups_per_minute` | Healthy watcher, three reload checks sharing maintenance; three modeled 120-second runs, plus a real scheduler sleep-deadline assertion | 30/min |
| `d9_hover_p90_ms` | 64 pointer crossings between two navigation controls, input through parent raster | 16.667 ms |
| `d9_service_update_p90_ms` | 64 changing audio percentages on the navigation bar, delivery through raster, with displayed-value checks | 16.667 ms |
| `d9_scroll_p90_ms` | Settings Appearance, 240 icon themes and 240 fonts; 60 finger frames and 30 momentum ticks | 16.667 ms |
| `d9_popover_open_p90_ms` | 12 theme popover openings from separate warmed navigation bars, input through first child raster | 100 ms |
| `d9_theme_switch_p90_ms` | 24 alternating shipped dark/light modes, invalidation through navigation raster; pixels must change | 100 ms |
| `d9_glass_dashboard_open_p90_ms` | 12 glass dashboard mounts with system, media and weather payloads, mount through first 1920×1080 raster | 100 ms |
| `d9_glass_dock_frame_p90_ms` | Glass dock with 10 pinned and 150 installed apps at 2880×180; 180 steps crossing tiles every sixth, requested frames only, including the `dock-in` entrance | 16.667 ms |
| `d9_glass_clock_tick_p90_ms` | 60 glass bar clock ticks at 2880×100, due script tasks through raster | 16.667 ms |

These are initial absolute responsiveness ceilings with no extra percentage
tolerance. They measure CPU raster at scale 1 (navigation 1920 pixels wide at
its measured height, Settings 920×900), without a compositor. The popover gate
includes child allocation and painting; theme switching excludes theme file
loading and persistent settings writes. Hover crossings may also disclose
navigation popovers. Scroll retains its one-pass, no repeated restyle and
retained-pixel-shift assertions. Frame pacing sleeps are outside scroll timing.
The glass gates use the violet-glass theme with glass-material; the dashboard
gate compiles its modules once outside the timed region. Clock ticks run at the
clock task's own deadline, because task sleeps re-queue from the wall clock.

The idle metric bounds shell reload-maintenance deadlines, not all OS thread
wakeups or backend service traffic. Native subscription and watcher idle tests
in the full suite cover those sources separately. These gates do not establish
GPU execution or Wayland presentation budgets; those remain separate renderer
work. Disable optional stage profiling when comparing timing results.


## GPU residency and workload gates

```bash
nix develop -c ./tools/check-gpu-performance
```

This builds one release renderer test binary and runs two exact tests serially:
`gpu_workload_visual_match_gate` and `gpu_workload_frame_cost_gate`. It requires
a DRM render node with GLES3 and dmabuf export; missing GPU support or an empty
test selection fails. The optional CI job uses a runner labeled `self-hosted`,
`linux`, `mesh-gpu` when repository variable `MESH_GPU_RUNNER` is `enabled`.
CPU CI does not substitute software timing for these hardware budgets.

Workloads use a 920×900 logical surface at scale 2 (1840×1800 pixels): 60 rounded,
shadowed Settings cards under a frosted panel, 40 text lines, and 256 image/icon
instances from 128 translucent PNG assets plus eight text lines. Frame costs
include complete UI scene preparation, worker handoff, retained GPU paint,
dmabuf copy/fence submission and GPU completion. They exclude compositor import
and scanout. Four warm-up frames precede three samples of 20 frames. Both the
slowest sample mean and individual-frame p90 must meet the ceilings in the test:

| Workload | Ceiling |
| --- | --- |
| Full Settings, full text, full images/icons | 16.67 ms |
| Frosted panel, small widget, one text line | 12 ms |

Test-only GLES hooks observe bitmap transfer calls. Warm frames must retain
resource identities and issue zero additional `glTexImage2D` data uploads or
`glTexSubImage2D` calls. These hooks are absent from production builds.

Visual gates read back the exported dmabuf slot and compare it against CPU raster
and direct GPU paint, then repeat under large and small partial damage. Recorded/direct GPU
output permits at most 0.1% of pixels differing by more than two levels, with a
maximum channel delta of three. CPU/GPU tolerances account for blur and coverage
rounding: Settings permits 10% / 40 levels; text and assets permit 1% / four
levels. Both comparisons must pass. Atlas tests separately check exact raster
pixels, shared snapshot identity, retirement and CPU fallback.

To compare the same workload with automatic Ganesh caching and atlas paging
turned off in the test harness:

```bash
MESH_REQUIRE_GPU=1 cargo test -p mesh-core-render --features renderer-gpu --release --lib -- \
  gpu_workload_automatic_residency_profile --ignored --nocapture --test-threads=1
```

This keeps scene preparation and GPU completion identical to the gated path;
it does not model compositor latency or frame pipelining.
