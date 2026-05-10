# Phase 26 Plan 01 Baseline Proof

This artifact records the canonical benchmark evidence contract for Phase 26 in the existing debug benchmark path.

## Canonical Scenarios

| Scenario ID | Shipped target | Interaction proof |
|-------------|----------------|-------------------|
| `hover` | `@mesh/navigation-bar` | Pointer hover on the shipped navigation bar |
| `surface_open_close` | `@mesh/audio-popover` | Open/close the shipped audio popover |
| `pointer_update` | `@mesh/navigation-bar audio controls` | Pointer-driven slider/control updates on the shipped navigation bar |
| `keyboard_traversal` | `@mesh/navigation-bar focus chain` | Shell-owned focus traversal on the shipped navigation bar |
| `backend_update` | `mesh.audio -> @mesh/pipewire-audio` | Backend-driven audio state update correlated with the shipped frontend surfaces |

## Pre-Change Baseline

Before Phase 26 instrumentation, the existing debug profiling path exposed the canonical scenario list and the coarse surface stages:

- `tree_build`
- `style_restyle`
- `layout`
- `paint`
- `present_commit`
- `total_surface_render`

That baseline was enough to identify that retained CPU render work still felt expensive on shipped proof surfaces, but it did not attribute the retained-path cost inside render-object synchronization, retained display-list update work, display-list traversal, text shaping, or icon/image raster work.

## Post-Instrumentation Profiling View

Phase 26 keeps the same benchmark rows and shipped targets, while extending `mesh.debug.profiling` with the retained CPU attribution stages:

- `render_object_sync`
- `retained_display_list_update`
- `paint_traversal`
- `text_shaping`
- `icon_image_raster`

The invalidation payload now also exposes `text.shaping_micros` for shell consumers that inspect the debug JSON directly.

## Evidence Recorded In This Plan

- Stable five-scenario benchmark contract verified by `cargo test -p mesh-core-shell benchmark_snapshot_exposes_five_stable_scenarios`
- Extended debug JSON serialization verified by `cargo test -p mesh-core-shell profiling_debug_payload_serializes_phase26_surface_attribution_labels`
- Real retained paint-path production of the new stages verified by `cargo test -p mesh-core-shell retained_paint_path_records_phase26_cpu_attribution_stages`
- Full profiling regression suite verified by `cargo test -p mesh-core-shell profiling`
- Render-crate regression suite verified by `cargo test -p mesh-core-render`

## Reuse In Later Phases

Later v1.5 optimization phases should compare before/after work only against these five scenario IDs and shipped targets, and should treat the Phase 26 stage list above as the baseline profiling surface for retained CPU render attribution.

## Limits

This isolated workspace execution records the benchmark-target and profiling-attribution baseline contract, not compositor-captured live smoothness measurements. Later optimization phases should add their own before/after timing samples against the same benchmark rows when renderer behavior changes.
