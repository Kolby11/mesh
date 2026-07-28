# 18-02 Summary: Targeted Hotspot Optimization

## Outcome

Optimized benchmark snapshot construction for the selected render-visible profiling path.

## Changes

- Added `BenchmarkProfilingView` to resolve `@mesh/navigation-bar`, `@mesh/audio-popover`, and the active backend candidate once per debug benchmark snapshot.
- Reused cached profiling references across benchmark rows instead of repeatedly scanning profiling surfaces and backends.
- Added `phase18_benchmark_payload_preserves_render_visible_contract_after_lookup_cache` to protect scenario order, benchmark status behavior, backend target selection, and render-visible metrics.

## Commands Run

| Command | Result |
| --- | --- |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt` | pass |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase18_` | pass |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` | pass |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check` | pass |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | pass |

## Next

Proceed to 18-03 and document the before/after proof with at least a 10% improvement claim backed by the lookup-pass reduction and guardrail results.
