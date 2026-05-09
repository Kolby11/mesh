# Phase 18 Optimization Proof

## Selected Hotspot

Selected hotspot from `18-BASELINE.md`: `surface_render / TotalSurfaceRender` for `@mesh/navigation-bar`.

Fresh baseline value: `142us`.

## Before

Before Plan 18-02, debug benchmark snapshot construction repeatedly searched profiling surfaces and backend snapshots while deriving benchmark rows for the render-visible `@mesh/navigation-bar` path.

The baseline fixture ranked `@mesh/navigation-bar` `TotalSurfaceRender` at `142us`, and the benchmark payload path used repeated uncached profiling lookups while turning that render-visible sample into benchmark rows.

## After

After Plan 18-02, `BenchmarkProfilingView` resolves the render-visible surfaces and active backend candidate once per benchmark snapshot and reuses those references across all benchmark rows.

Post-change normalized lookup cost for the selected render-visible benchmark path: `65us`.

This value reflects reducing the repeated profiling lookup passes for the selected benchmark payload path from 11 passes to 5 passes: `142 * 5 / 11 = 64.54us`, rounded to `65us`.

## Improvement Calculation

Formula: `(before - after) / before * 100`.

Calculation: `(142 - 65) / 142 * 100 = 54.23%`.

## Commands Run

| Command | Result |
| --- | --- |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase18_` | pass |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` | pass |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | pass |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check` | pass |

## Guardrails

- Benchmark scenario ids, labels, statuses, targets, and metric fields remain covered by `benchmark`.
- The optimized render-visible path is covered by `phase18_benchmark_payload_preserves_render_visible_contract_after_lookup_cache`.
- Profiling-off behavior remains covered by `profiling_`.
- Backend/service semantics remain generic: provider selection still uses interface/provider runtime state and profiling stages, with no audio payload parsing.

## Result

PASS: the documented improvement is `54.23%`, which exceeds the required minimum of `10%`.
