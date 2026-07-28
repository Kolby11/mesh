# Phase 18 Baseline and Hotspot Selection

## Measurement Source

Fresh deterministic shell profiling fixture in `phase18_baseline_ranks_hotspots_by_absolute_latency`.

The fixture enables debug profiling, records frontend render samples, records one backend sample, records frontend impact for that backend path, builds `Shell::build_debug_snapshot()`, and ranks eligible candidates by absolute latency in microseconds.

## Ranked Hotspots

| Rank | Benchmark/Stage | Surface or Provider | Metric | Value (us) | Eligible |
| --- | --- | --- | --- | ---: | --- |
| 1 | surface_render / TotalSurfaceRender | @mesh/navigation-bar | max_micros | 142 | yes |
| 2 | backend_update / StatePublishDelivery | mesh.audio -> @mesh/pipewire-audio | max_micros with visible frontend render | 109 | yes |
| 3 | backend_update / TotalSurfaceRender | @mesh/audio-popover | total_surface_render_time_micros | 88 | yes |
| 4 | surface_render / Paint | @mesh/navigation-bar | max_micros | 77 | yes |

## Selected Hotspot

Selected hotspot: `surface_render / TotalSurfaceRender` for `@mesh/navigation-bar`, with a fresh baseline value of `142us`.

This is the largest eligible absolute latency observed in the phase 18 baseline fixture. The phase 18 optimization pass will target work that reduces benchmark/debug payload overhead on the render-visible profiling path without changing the public debug payload contract.

## Tie-Breaker

The user-selected tie-breaker is largest absolute latency. No tie was present in the baseline ranking.

## Backend Eligibility

The backend candidate is eligible because the fixture records both backend publish latency and a frontend `TotalSurfaceRender` sample for a backend-visible surface.

Backend path not selected because a frontend/render hotspot had larger absolute latency or backend impact was not visible.

## Commands Run

| Command | Result |
| --- | --- |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase18_baseline` | pass |
