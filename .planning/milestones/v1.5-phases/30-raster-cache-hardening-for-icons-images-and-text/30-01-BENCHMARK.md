---
phase: 30
plan: 01
title: Raster cache hardening benchmark proof
created: 2026-05-12
status: complete
canonical_scenarios:
  - hover
  - surface_open_close
  - pointer_update
  - keyboard_traversal
  - backend_update
---

# Phase 30 Benchmark Proof

## Scope

Phase 30 does not claim final visible-smoothness tuning. That remains Phase 31.
This proof records deterministic cache-hit/cache-miss evidence through the existing profiling path and focused renderer unit tests.

## Renderer Cache Evidence

Automated proof:

- `cargo test -p mesh-core-render icon`
- `cargo test -p mesh-core-render glyph`
- `cargo test -p mesh-core-render text_cache`
- `cargo test -p mesh-core-render display_list`

Deterministic assertions:

- Repeated SVG paints with identical path, dimensions, tint, multicolor mode, and freshness produce `raster_cache_hits=1`, `raster_cache_misses=0`, and `icon_image_raster_micros=0` on the second paint.
- Repeated bitmap paints reuse cached resized/tinted variants and report opaque/translucent hit classes from cached pixels.
- Missing-icon fallback reuse reports a raster cache hit on the second paint.
- Tint and multicolor changes produce distinct raster cache misses.
- File length/modified metadata changes produce a fresh miss instead of serving a stale file-backed variant.
- SVGs with external `href` or `url(...)` resources bypass the file-backed cache instead of serving stale linked resources.
- Path identities are retained without lossy UTF-8 conversion, including Unix non-UTF filenames.
- Metadata or external-resource bypasses are reported separately from hits and misses.
- Text measure, render, and selection paths reuse unchanged layout entries.
- Text layout-affecting changes produce misses, including font family and alignment.
- Glyph tint, size, and supported axis changes use distinct keys; exact cached glyph hits do not add icon/image raster time.
- Opaque display-list backgrounds remain batchable; translucent backgrounds keep conservative translucency barriers.
- Warmed cached opaque file icons are batchable, warmed cached translucent icons keep translucency barriers, and unknown icons keep conservative icon barriers.

## Canonical Scenario Evidence

Captured with:

`env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture`

| Scenario | Paint evidence | Text cache evidence | Raster cache evidence |
| --- | ---: | ---: | ---: |
| hover | paint recorded | hits 5, misses 0, shaping 0us | hits 2, misses 2, bypasses 0 |
| surface_open_close | paint recorded | hits 0, misses 6, shaping 1493us | hits 0, misses 1, bypasses 0 |
| pointer_update | paint recorded | hits 4, misses 2, shaping 272us | hits 1, misses 0, bypasses 0 |
| keyboard_traversal | paint recorded | hits 5, misses 0, shaping 0us | hits 4, misses 0, bypasses 0 |
| backend_update | paint recorded | hits 3, misses 2, shaping 1365us | hits 4, misses 0, bypasses 0 |

## Interpretation

- `surface_open_close` is expected to include a cold miss for the first audio popover icon paint.
- `pointer_update`, `keyboard_traversal`, and `backend_update` demonstrate warm steady-state raster reuse through existing surface profiling.
- `hover` intentionally changes visual state and records both misses and hits, proving key separation and reuse in the same shipped-surface path.
- Timing values are recorded only as smoke evidence that real paint work occurred. Phase 30 acceptance rests on deterministic cache counters, not wall-clock thresholds.
