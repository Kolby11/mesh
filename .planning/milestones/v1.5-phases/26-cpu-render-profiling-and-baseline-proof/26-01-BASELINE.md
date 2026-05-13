# Phase 26 Plan 01 Baseline Proof

This artifact records concrete Phase 26 baseline evidence for the five canonical benchmark scenarios using the shipped proof surfaces plus the existing `mesh.debug` snapshot contract.

## Canonical Scenarios

| Scenario ID | Shipped target | Interaction proof |
|-------------|----------------|-------------------|
| `hover` | `@mesh/navigation-bar` | Pointer hover on the shipped navigation bar |
| `surface_open_close` | `@mesh/audio-popover` | Open/close the shipped audio popover |
| `pointer_update` | `@mesh/navigation-bar audio controls` | Pointer-driven slider/control updates on the shipped navigation bar |
| `keyboard_traversal` | `@mesh/navigation-bar focus chain` | Shell-owned focus traversal on the shipped navigation bar |
| `backend_update` | `mesh.audio -> @mesh/pipewire-audio` | Backend-driven audio state update correlated with the shipped frontend surfaces |

## Evidence Sources

Phase 26 now uses two complementary proof sources:

1. **Real shipped-surface measurements** from `phase26_real_surface_baseline_emits_canonical_proof_measurements` in [component/tests.rs](/home/kolby/projects/mesh/crates/core/shell/src/shell/component/tests.rs:216). This test renders the shipped `@mesh/navigation-bar` and `@mesh/audio-popover` components, drives hover/pointer/keyboard/backend-update flows through the real component input and service-event paths, and prints the measured retained-stage timings captured on this environment with:

   ```text
   env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture
   ```

2. **Deterministic benchmark-row contract proof** from `phase26_baseline_proof_records_canonical_scenario_values_and_retained_hotspots` in [tests.rs](/home/kolby/projects/mesh/crates/core/shell/src/shell/tests.rs:2259). That shell test builds a `Shell::build_debug_snapshot()` with the canonical benchmark rows and locks the scenario-id/metric formatting that later phases reuse when they compare before/after deltas.

## Measured Shipped-Surface Baseline

Captured on 2026-05-11 from the real-surface proof command above:

| Scenario ID | Measured retained/cpu evidence | Smoothness note |
|-------------|--------------------------------|-----------------|
| `hover` | `style_restyle: 157us`, `paint: 3244us`, `paint_traversal: 1877us` | Full rebuild path on the shipped navigation bar (`retained=false`, `full_rebuild=true`); paint dominates simple hover restyles. |
| `surface_open_close` | `paint: 33449us`, `paint_traversal: 31240us`, `text.shaping_micros: 1251us` | Opening the shipped audio popover is the heaviest measured frontend path in this capture; most cost sits inside paint/traversal rather than shaping. |
| `pointer_update` | `layout: 106us`, `paint: 2005us`, `paint_traversal: 1094us` | Pointer-driven audio control updates stay cheaper than popover open/close, but still rebuild the full surface on this baseline capture. |
| `keyboard_traversal` | `style_restyle: 93us`, `paint: 3037us`, `paint_traversal: 1694us` | Focus movement on the shipped navigation bar remains paint-bound and follows the same full-rebuild path as hover. |
| `backend_update` | `paint: 31468us`, `paint_traversal: 30011us`, `text.shaping_micros: 0us` | Backend-driven audio state changes on the shipped navigation bar still land as a paint-dominated full rebuild. |

## Pre-Change Baseline Contract

Before Phase 26 added retained CPU substages, the canonical benchmark proof still depended on the coarse surface timings that were already present in the debug snapshot contract:

| Surface / flow | Coarse baseline evidence |
|----------------|--------------------------|
| `@mesh/navigation-bar` hover / traversal / pointer flow | `input_handling: 24us`, `style_restyle: 61us`, `runtime_update_handling: 42us`, `layout: 94us`, `paint: 149us`, `total_surface_render: 214us` |
| `@mesh/audio-popover` surface open/close | `total_surface_render: 188us`, `redraw_count: 3` |
| `mesh.audio -> @mesh/pipewire-audio` backend update | `state_publish_delivery: 73us`, with visible frontend impact confirmed by `@mesh/navigation-bar total_surface_render: 214us` |

That coarse view showed that shipped proof surfaces were still paying meaningful render cost, but it did not tell later phases how much of that paint work came from retained render-object synchronization, display-list updates, traversal, text shaping, or icon/image raster work.

## Canonical Benchmark Rows

The deterministic Phase 26 snapshot proof preserves the exact five benchmark scenario IDs and produces these reusable row values for later phase comparisons:

| Scenario ID | Primary metric | Secondary metric |
|-------------|----------------|------------------|
| `hover` | `input_handling: 1 samples, max 24us` | `style_restyle: 1 samples, max 61us` |
| `surface_open_close` | `total_surface_render: 188us` | `redraw_count: 3` |
| `pointer_update` | `input_handling: 1 samples, max 24us` | `layout: 1 samples, max 94us` |
| `keyboard_traversal` | `input_handling: 1 samples, max 24us` | `total_surface_render: 1 samples, max 214us` |
| `backend_update` | `mesh.audio -> @mesh/pipewire-audio state_publish_delivery: 1 samples, max 73us` | `frontend total_surface_render: 214us` |

## Post-Instrumentation Profiling View

Phase 26 keeps those same benchmark rows and shipped targets, while extending `mesh.debug.profiling` with retained CPU attribution stages on the shipped proof surfaces:

| Retained stage | Recorded max |
|----------------|--------------|
| `render_object_sync` | `34us` |
| `retained_display_list_update` | `57us` |
| `paint_traversal` | `91us` |
| `text_shaping` | `12us` |
| `icon_image_raster` | `6us` |

The invalidation payload also exposes `text.shaping_micros: 12`, so shell consumers can inspect the shaping cost directly in debug JSON.

## Dominant-Stage Findings

- The real shipped-surface capture shows the same story as the deterministic snapshot proof: `paint_traversal` is the dominant retained/render stage on the proof surfaces, and audio-popover open/close plus backend-driven updates are the heaviest paths.
- `retained_display_list_update` remains the next retained hotspot in the deterministic snapshot proof, large enough that partial display-list reuse should be measurable in follow-on work.
- `render_object_sync` remains material, while `text_shaping` and `icon_image_raster` are secondary contributors relative to the broader paint/traversal budget.
- The shipped-surface capture hit `retained=false` / `full_rebuild=true` for all five scenario classes, which is the key Phase 26 baseline fact for later optimization phases: they should aim to shrink or eliminate those full-rebuild paint paths first.

## Evidence Recorded In This Plan

- Stable five-scenario benchmark contract verified by `cargo test -p mesh-core-shell benchmark_snapshot_exposes_five_stable_scenarios`
- Real shipped-surface proof values recorded by `cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture`
- Deterministic Phase 26 benchmark-row values and retained hotspot ordering verified by `cargo test -p mesh-core-shell phase26_baseline_proof_records_canonical_scenario_values_and_retained_hotspots`
- Extended debug JSON serialization verified by `cargo test -p mesh-core-shell profiling_debug_payload_serializes_phase26_surface_attribution_labels`
- Real retained paint-path production of the new stages verified by `cargo test -p mesh-core-shell retained_paint_path_records_phase26_cpu_attribution_stages`
- Full profiling regression suite verified by `cargo test -p mesh-core-shell profiling`
- Render-crate regression suite verified by `cargo test -p mesh-core-render`

## Reuse In Later Phases

Later v1.5 optimization phases should compare their before/after work against these five scenario IDs, these shipped targets, the real-surface paint/traversal measurements above, and the retained hotspot ordering before claiming smoothness gains.

## Limits

This baseline mixes a real headless shipped-surface capture with deterministic benchmark-row contract tests; it is still not compositor-captured live telemetry. Later optimization phases should keep using the same benchmark rows and shipped targets, then attach their own before/after deltas when renderer behavior changes.
