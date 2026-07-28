# 18-01 Summary: Fresh Baseline and Hotspot Ranking

## Outcome

Completed the fresh phase 18 baseline and selected `@mesh/navigation-bar` `TotalSurfaceRender` as the largest eligible absolute-latency hotspot.

## Changes

- Added `phase18_baseline_ranks_hotspots_by_absolute_latency` to prove hotspot ranking against a fresh deterministic profiling snapshot.
- Recorded the baseline ranking and selected hotspot in `18-BASELINE.md`.
- Preserved backend eligibility by requiring a backend stage sample and visible frontend render impact.

## Commands Run

| Command | Result |
| --- | --- |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase18_baseline` | pass |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` | pass |
| `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | pass |

## Next

Proceed to 18-02 and optimize the selected render-visible profiling path while preserving benchmark payload contracts.
