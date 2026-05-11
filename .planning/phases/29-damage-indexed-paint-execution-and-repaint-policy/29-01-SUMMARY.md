# Phase 29 Plan 01 Summary

**Phase:** 29 - Damage-Indexed Paint Execution and Repaint Policy  
**Plan:** 29-01 - Damage-indexed retained paint execution and repaint-policy proof  
**Completed:** 2026-05-11  
**Status:** Complete

## What Changed

- Added retained command-span metadata in `RetainedDisplayList`, keyed by owning `NodeId`, with command range, aggregate bounds, command count, and scrollbar participation.
- Added render-owned repaint policy accounting with externally visible labels `minimal_damage`, `bounding_rect`, and `full_surface`.
- Added `select_paint_commands(...)` so partial damage routes ordered filtered command input into the software painter instead of always passing the full retained command list.
- Extended `invalidation.paint` debug payloads with repaint policy, filtered span count, filtered command count, skipped command count, and fallback count.
- Recorded Phase 29 benchmark proof in `29-01-BENCHMARK.md` using the existing five canonical scenario IDs.

## Requirements Covered

- `PIPE-03`: Partial damage can select only retained command spans that intersect damage, while preserving scrollbar inclusion through owning spans.
- `PIPE-04`: Filtered command input preserves original display-list order among surviving commands and keeps tooltip overlay drawing separate.
- `CULL-03`: Shell repaint selection now carries minimal-damage, bounding-rect, or full-surface policy through render metrics and debug proof.

## Verification

```text
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render painter_
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling
```

All listed commands passed. The Nix eval cache reported transient SQLite busy messages during parallel tests, but the test processes completed successfully.

## Notes

- Phase 31 still owns visible-smoothness acceptance and repaint-policy threshold tuning.
- No new benchmark harness, trace persistence path, or per-command debug payload was added.
