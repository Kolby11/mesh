# Phase 29 Plan 02 Summary

**Phase:** 29 - Damage-Indexed Paint Execution and Repaint Policy  
**Plan:** 29-02 - Debug-inspector retained paint counter readability  
**Completed:** 2026-05-12  
**Status:** Complete

## What Changed

- Added retained paint filtering rows to the debug-inspector Surfaces view.
- Surface rows now show repaint policy, filtered fallback count, filtered command count, filtered span count, and skipped command count.
- Missing or partial paint-counter payloads now render unavailable labels instead of coercing missing counters to zero.
- Added a real debug-inspector integration test proving those counters render from `profiling.surfaces[].invalidation.paint` and that partial payloads use unavailable labels.
- Updated the Phase 29 UAT gap with root cause, fixed status, and touched artifacts.

## Gap Closed

The Phase 29 payload already exposed retained-paint proof, but the inspection surface did not make it readable. Operators can now see whether a surface used `minimal_damage`, `bounding_rect`, or `full_surface`, and whether filtered execution skipped commands.

## Verification

```text
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render painter_
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_inspector
```

All listed commands passed.
