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

## Surface render timing

`total_surface_render` sums a surface's own synchronous preparation, CPU raster
replay and presentation work. Other surfaces' preparation and presentation,
raster-worker queueing and the wait for the shared raster batch are excluded.
Child surfaces start their own timers at subtree painting; parent preparation
and child-request reconciliation are not charged to them. GPU worker execution
and compositor display latency are asynchronous and excluded from this total.
The `paint` samples tagged `deferred_raster` report individual CPU replays;
`present_commit` reports the presentation call.

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
| `retained_discovery`, `retained_sync`, `retained_lookup` | Test-only discovery, selected-node fingerprint refresh, and child-path steps |
| `display_entries`, `display_ancestors`, `display_build` | Display-entry traversal, dirty-ancestor discovery, and paint-subtree visits |
| `child_surface_requests` | Child-surface indexing, dependency/path lookup and affected subtree extent visits |

These counters measure the named functions, not every operation in their
subsystems. For example, semantic child-text aggregation and reference lookup
are not counted as normalization visits. Pruned/cache-hit nodes still count
when a traversal examines them. Child requests are refreshed before the paint
snapshot publishes, so these visits belong to that paint. Repeated request
reads reuse the cache; a standalone discovery fallback emits a separate sample.

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
pixels and verify structural fallbacks. Binding targets retain typed setters
and indexed paths. Plain item-field keys reuse validation when their dependencies
are unchanged. `keyed_list_item_writes_compare_only_written_items` checks that a
list read directly from a script member compares and key-checks only the items
its writes changed, at 16 and 1,024 items, with forced-rebuild pixel parity;
a list read through any other expression still compares every item.
`geometry_patch_frames_scale_with_the_change` gates geometry-changing patches
(text growth, width, last-item height, and a change beside a separate
1,024-node column): display entries (release), display builds, retained and
layout synchronization and keyed compares stay equal at 16 and 1,024 siblings,
with rect, pixel and retained-tree parity. Layout write-back still visits every
child of a re-laid-out parent, the same order as Taffy's own flex pass.
The W3 gates compare 16 and 1,024 clean siblings:
`display_ancestors_visit_only_retained_paths`,
`transition_and_lifecycle_frames_visit_only_changed_nodes`,
`child_surface_requests_visit_only_dirty_dependencies`,
`closed_child_surface_requests_skip_clean_descendants`,
`retained_layout_validates_only_structure_changes`, and
`semantic_reindex_reuses_validated_structure`. They check path-proportional
ancestor work, scoped transition snapshots/sampling, cached child requests,
and zero retained structural validation; semantic reindexing is compared with
a complete capture, and actual invalid tags still fail validation.
Scene preparation is gated by `scene_preparation_records_only_changed_nodes`:
16 and 1,024 clean siblings reuse their node pictures while one changed node
records one new picture, with full-scene and direct-raster pixel parity.
`scene_cache_invalidates_external_paint_inputs`,
`scene_cache_preserves_effects_scroll_and_tooltip_inputs`, and
`scene_node_cache_bounds_shared_resources_and_rejects_inflight_results` cover
cache inputs, live scopes, resource ownership/admission and eviction.
`SceneCanvas::preparation_metrics()` reports recorded nodes/operations, reused
nodes and live commands. Reused picture references still require complete-scene
composition; these counters do not claim that traversal or GPU replay is free.
The ignored release `scene_preparation_profile` compares the production cache
with the original uncached path. `tools/check-gpu-performance` continues to gate
exported pixels, completed-frame costs and zero warm bitmap uploads on hardware.

The glass dock and spectrum profile tests print visits per measured paint
alongside their timings. `MESH_BENCH_FORCE_TEMPLATE=1` disables template
patching/selective template builds in the spectrum profile for comparison.
Use profiling-disabled runs for timing claims about
instrumentation overhead.

## Child popup first-open attribution

Start a release shell with `RUST_LOG=info,mesh_profiling=debug`, then enable
profiling with `target/release/mesh-shell debug profiling` before opening the
popup. `MESH_RENDER_GPU=0` forces CPU/SHM with the same GPU-enabled binary.
Use fresh processes to distinguish first-use resource work from warm reopens.

Child surface samples with `attribution:child_` triggers split canvas setup,
backdrop context, component painting, and damage/blur updates. These are nested
attribution samples; exclude them from sums of ordinary stage totals. Child
paint logs additionally carry `child_stage` for root snapshot, display-list
update, command selection, canvas session, command replay and finalization.
`child_command_class` splits replay into primitive, text, icon, control and
scrollbar commands; text shaping and icon raster times are nested subsets.
Detailed child logs identify the component and node key, while outer samples
identify the actual child surface. Timers and command attribution are disabled
when built-in profiling is off.

Match input to the target's first non-null Wayland buffer commit for live open
latency. CPU child replay is immediate; GPU child replay records commands, and
its timer excludes asynchronous worker raster and later compositor commit.
Use stack sampling alongside these spans when an icon-command time greatly
exceeds the narrower icon-raster counter. Repeat with profiling disabled before
claiming production latency or an optimization speedup.

## Focused live GPU popup validation

```bash
nix develop -c cargo test -p mesh-core-shell --release --features gpu --lib live_gpu_popup_validation -- --ignored --nocapture --test-threads=1
```

This opt-in test requires a reachable compositor and the GPU scene worker. It
uses the live replay's private config, state and IPC socket and separate layer
namespace. Calendar and controls open and close near bar timer deadlines, then
an idle clock window, six CPU updates and thirty spectrum updates are captured.
The test waits for actual worker completions before leaving popup/dashboard
opening phases. Raw stage, node-visit and scene-completion records plus window
timestamps are written to ignored `profiles/popup-live/<pid>/capture.json`.
A successful scene-completion record means the worker frame reached the
compositor; `PresentCommit` timing alone can also describe accepted submission.
Run the test inside `nix develop` so EGL can find the host vendor drivers.

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
present. Stage means exclude nested `attribution:` and `waste:` samples to
avoid double counting. `stage_cpu_raster_mean_ms` isolates deferred CPU replay;
`stage_paint_record_or_immediate_mean_ms` is recording, or immediate raster on
frames without deferred replay. Both are included in `stage_paint_mean_ms`.

The command builds a CPU-only binary by default. Set `MESH_LIVE_FEATURES=gpu`
to match the CLI's default feature; the report prints the presentation engine's
render-path reason. Set `MESH_RENDER_GPU=0` with that same GPU-enabled binary
for a forced-CPU comparison. GPU paint and presentation timings measure scene
recording and submission on the calling thread; they exclude asynchronous GPU
work and compositor display latency. In GPU runs, `first_present` waits for a successful scene completion after
the worker result is committed to the Wayland surface; submission alone and
unpresented completions do not count. This measures commit, not physical display.
Timings are for visibility; the only failure is structural: every live dock
hover frame must stay within the per-pass maxima of the same frame class
(hover change or animation only) in the CPU counterpart embedded in the replay.
Both use the same absolute-time schedule: 20 tile crossings, a 100 ms dwell,
six pointer positions per tile and the same out-and-back route. Slow raster
paints skip overdue sampling ticks instead of extending the dwell. Both run an
unmeasured warm-up sweep, leave the dock and settle its transitions before the
measured sweep. Tile bounds come from the settled shelf. The fixture samples
at 16 ms deadlines; live animation paints follow compositor callbacks.
Replay surfaces appear on the compositor for about a minute under the shipped
module ids with a `.mesh-replay` suffix as layer namespaces (before any `:blur`
marker), so they can be told apart from a running desktop shell's surfaces; the
replay also uses its own IPC socket and state. The shell then shuts down and its
private files are removed.

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
The glass gates use the liquid-glass theme with glass-material; the dashboard
gate compiles its modules once outside the timed region. Clock ticks run at the
clock task's own deadline, because task sleeps re-queue from the wall clock.

The idle metric bounds shell reload-maintenance deadlines, not all OS thread
wakeups or backend service traffic. Native subscription and watcher idle tests
in the full suite cover those sources separately. These gates do not establish
GPU execution or Wayland presentation budgets; those remain separate renderer
work. Disable optional stage profiling when comparing timing results.


## CPU glass timing fixture

```bash
nix develop -c cargo test -p mesh-core-shell --release --lib glass_dock_cpu_raster_timing_profile -- --ignored --nocapture --test-threads=1
```

This compositor-free timing fixture paints the shipped glass dock at 2880×114,
scale 1, bottom-aligned on a 2880×1800 output. It loads an output-sized,
nonuniform wallpaper and supplies a `BackdropFrame` that samples the source,
with the compositor-blur policy, so the dock's refraction and dispersion
execute. (Live, the top-layer dock routes its blur to the compositor and paints
only rims; the fixture keeps measuring the source-sampling path.) It uses the shell's
`SurfaceCanvas` recording and `RasterReplay::run` boundary. Three paced sweeps
cross ten pinned apps beside 150 installed apps, after entrance animations settle.
Only requested frames are sampled; preparation includes input, script and paint
recording, while replay reports CPU raster separately. Total includes both and
excludes pacing sleeps and Wayland presentation.

Before timing, snapped-animation frames must paint identical bytes directly and
through deferred replay; removing the output backdrop must change those bytes.
The fixture runs serially because the renderer's wallpaper source is global.
`MESH_BENCH_WALLPAPER=/absolute/image.png` selects a real wallpaper instead of
the deterministic source. `MESH_GLASS_TIMING_MODE=direct` measures immediate
raster; `no-backdrop` measures deferred replay with the old zero-output context.
The default is `deferred`. These are attribution controls, not identical pixel
workloads. The fixture is included in `tools/check-performance`; its timings
are diagnostic and carry no machine-independent latency ceiling. The existing
visit fixture still supplies the live replay's structural envelope.

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

### Retained dirty-set completeness

Run shell tests with `MESH_VERIFY_RETAINED=1` to compare the finalized retained
snapshots with a full fingerprint verifier after every paint. For example:

```sh
nix develop -c env MESH_VERIFY_RETAINED=1 cargo test -p mesh-core-shell --release --lib
```

The verifier is compiled only in tests and runs outside visit counters. Leave it
disabled for timing profiles: its full-tree work is intentional verification,
not production frame cost. `tools/check-performance` enables it for the dirty-set
structural gates, including the layout gate at 16 and 1,024 clean siblings.
