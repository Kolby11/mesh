---
phase: 30
plan: 01
title: Renderer-owned raster cache hardening and proof
status: complete
completed: 2026-05-12
requirements: ["CACHE-01", "CACHE-02", "CACHE-03"]
---

# Phase 30 Plan 01 Summary

## Delivered

- Added a renderer-owned bounded raster variant cache for bitmap, SVG, and built-in missing-icon paints.
- Keyed file-backed variants by lossless path identity, dimensions, tint, multicolor mode, and conservative file freshness.
- Bypassed caching for SVG files with external `href` or `url(...)` resources, and reported bypasses separately from hits and misses.
- Retained conservative cached opacity metadata and wired warmed file-icon opacity into display-list barrier decisions:
  - opaque cached file icons can batch;
  - translucent cached file icons keep translucency barriers;
  - unknown, missing, named, or bypassed icons keep conservative icon barriers.
- Extended raster profiling with hit, miss, bypass, opaque-hit, and translucent-hit counters.
- Serialized raster cache proof through existing shell debug/profiling invalidation payloads.
- Extended text cache proof for measure, render, selection, font-family changes, alignment changes, and other layout-affecting inputs.
- Extended glyph cache proof for exact-hit raster timing and tint/size/supported-axis key separation.
- Recorded canonical shipped-surface proof in `30-01-BENCHMARK.md`.

## Review Fixes

- Fixed stale SVG cache risk by bypassing SVGs that reference external resources.
- Fixed non-UTF path identity collapse by storing `PathBuf` in raster cache keys instead of lossy strings.
- Added `raster_cache_bypasses` to distinguish conservative bypasses from misses.
- Replaced counter-only opacity proof with display-list barrier behavior driven by warmed cached resource opacity.
- Strengthened shipped-surface assertions to require warm raster hits for steady-state scenarios.

## Notes

Phase 30 intentionally avoids final capacity tuning and subjective smoothness acceptance. Those remain Phase 31 scope.
