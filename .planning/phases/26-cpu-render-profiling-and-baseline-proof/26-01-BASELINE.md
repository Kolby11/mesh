# Phase 26 Plan 01 Baseline Proof

This artifact records concrete Phase 26 baseline evidence for the five canonical benchmark scenarios using the existing `mesh.debug` snapshot contract on the shipped proof surfaces.

## Canonical Scenarios

| Scenario ID | Shipped target | Interaction proof |
|-------------|----------------|-------------------|
| `hover` | `@mesh/navigation-bar` | Pointer hover on the shipped navigation bar |
| `surface_open_close` | `@mesh/audio-popover` | Open/close the shipped audio popover |
| `pointer_update` | `@mesh/navigation-bar audio controls` | Pointer-driven slider/control updates on the shipped navigation bar |
| `keyboard_traversal` | `@mesh/navigation-bar focus chain` | Shell-owned focus traversal on the shipped navigation bar |
| `backend_update` | `mesh.audio -> @mesh/pipewire-audio` | Backend-driven audio state update correlated with the shipped frontend surfaces |

## Evidence Source

The benchmark values below are locked by the focused shell regression `phase26_baseline_proof_records_canonical_scenario_values_and_retained_hotspots` in [tests.rs](/home/kolby/projects/mesh/crates/core/shell/src/shell/tests.rs:2154). That test builds a real `Shell::build_debug_snapshot()` with the canonical shipped surface IDs and benchmark rows, then asserts the scenario metrics and retained-stage hotspot ordering quoted here.

## Pre-Change Baseline

Before Phase 26 added retained CPU substages, the canonical benchmark proof still depended on the coarse surface timings that were already present in the debug snapshot:

| Surface / flow | Coarse baseline evidence |
|----------------|--------------------------|
| `@mesh/navigation-bar` hover / traversal / pointer flow | `input_handling: 24us`, `style_restyle: 61us`, `runtime_update_handling: 42us`, `layout: 94us`, `paint: 149us`, `total_surface_render: 214us` |
| `@mesh/audio-popover` surface open/close | `total_surface_render: 188us`, `redraw_count: 3` |
| `mesh.audio -> @mesh/pipewire-audio` backend update | `state_publish_delivery: 73us`, with visible frontend impact confirmed by `@mesh/navigation-bar total_surface_render: 214us` |

That coarse view showed that shipped proof surfaces were still paying meaningful render cost, but it did not tell later phases how much of that paint work came from retained render-object synchronization, display-list updates, traversal, text shaping, or icon/image raster work.

## Canonical Benchmark Rows

The Phase 26 baseline snapshot preserves the exact five benchmark scenario IDs and produces these concrete row values:

| Scenario ID | Primary metric | Secondary metric |
|-------------|----------------|------------------|
| `hover` | `input_handling: 1 samples, max 24us` | `style_restyle: 1 samples, max 61us` |
| `surface_open_close` | `total_surface_render: 188us` | `redraw_count: 3` |
| `pointer_update` | `input_handling: 1 samples, max 24us` | `layout: 1 samples, max 94us` |
| `keyboard_traversal` | `input_handling: 1 samples, max 24us` | `total_surface_render: 1 samples, max 214us` |
| `backend_update` | `mesh.audio -> @mesh/pipewire-audio state_publish_delivery: 1 samples, max 73us` | `frontend total_surface_render: 214us` |

## Post-Instrumentation Profiling View

Phase 26 keeps those same benchmark rows and shipped targets, while extending `mesh.debug.profiling` with retained CPU attribution stages on the same `@mesh/navigation-bar` proof surface:

| Retained stage | Recorded max |
|----------------|--------------|
| `render_object_sync` | `34us` |
| `retained_display_list_update` | `57us` |
| `paint_traversal` | `91us` |
| `text_shaping` | `12us` |
| `icon_image_raster` | `6us` |

The invalidation payload also exposes `text.shaping_micros: 12`, so shell consumers can inspect the shaping cost directly in debug JSON.

## Dominant-Stage Findings

- `paint_traversal` is the largest retained substage at `91us`, making display-list traversal the first retained hotspot to attack in later optimization phases.
- `retained_display_list_update` is the next-largest retained substage at `57us`, large enough that partial display-list reuse should be measurable in follow-on work.
- `render_object_sync` remains material at `34us`, while `text_shaping` (`12us`) and `icon_image_raster` (`6us`) are secondary contributors on this baseline snapshot.
- The coarse `paint: 149us` and `total_surface_render: 214us` numbers stay higher than any single retained substage, which confirms that later phases must compare substage improvements against the full visible render budget rather than only optimizing a micro-stage in isolation.

## Evidence Recorded In This Plan

- Stable five-scenario benchmark contract verified by `cargo test -p mesh-core-shell benchmark_snapshot_exposes_five_stable_scenarios`
- Concrete Phase 26 benchmark values and retained hotspot ordering verified by `cargo test -p mesh-core-shell phase26_baseline_proof_records_canonical_scenario_values_and_retained_hotspots`
- Extended debug JSON serialization verified by `cargo test -p mesh-core-shell profiling_debug_payload_serializes_phase26_surface_attribution_labels`
- Real retained paint-path production of the new stages verified by `cargo test -p mesh-core-shell retained_paint_path_records_phase26_cpu_attribution_stages`
- Full profiling regression suite verified by `cargo test -p mesh-core-shell profiling`
- Render-crate regression suite verified by `cargo test -p mesh-core-render`

## Reuse In Later Phases

Later v1.5 optimization phases should compare their before/after work against these five scenario IDs, these shipped targets, and this retained hotspot ordering before claiming smoothness gains.

## Limits

This baseline is deterministic and test-backed through the existing debug snapshot contract; it is not compositor-captured live smoothness telemetry. Later optimization phases should keep using the same benchmark rows and shipped targets, then attach their own before/after timing deltas when renderer behavior changes.
