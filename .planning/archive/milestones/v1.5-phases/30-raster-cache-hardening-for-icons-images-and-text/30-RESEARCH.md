# Phase 30 Research - Raster Cache Hardening for Icons, Images, and Text

**Created:** 2026-05-12  
**Mode:** Inline research after the background researcher did not return in time

## Current State

### Icon and Image Rendering

- `crates/core/frontend/render/src/surface/icon.rs` caches decoded bitmap source images in `IMAGE_CACHE`, keyed by `PathBuf`.
- Bitmap paths still resize into a fresh `RgbaImage` on every paint.
- SVG paths read, parse, and rasterize on every paint.
- The built-in missing icon fallback parses and rasterizes the embedded SVG on every paint.
- Icon-font glyphs already have a raster cache in `glyph.rs`, keyed by font path hash, codepoint, destination pixel size, tint, and supported variable-font axes.
- Icon/image raster time is recorded through `profiling::record_icon_image_raster` and reported as `icon_image_raster_micros`.

### Text and Glyph Rendering

- `TextRenderer` owns a `cosmic_text::SwashCache` and a bounded text layout cache with `TEXT_LAYOUT_CACHE_CAPACITY = 128`.
- Text cache metrics expose layout hits, misses, invalidations, shaped entries, glyph-cache-active, and shaping micros.
- `measure_styled`, `render_clipped`, and `selection_geometry` share `take_layout` / `store_layout`, so repeated identical layout inputs can hit cache.
- Existing tests cover measurement cache reuse and input-change misses.

### Retained Paint and Debug Proof

- Phase 29 already publishes retained paint and text cache proof through existing debug profiling payloads.
- Shell paint metrics subtract text shaping and icon/image raster timing from traversal timing.
- The debug inspector now renders retained paint filtering counters directly in the Surfaces view.

## Implementation Direction

### Raster Variant Cache

Add a renderer-owned raster variant cache for file-backed icons/images and the missing icon fallback. The cache should store rendered BGRA/RGBA pixels at the requested destination size, plus conservative metadata:

- width and height
- source opacity classification: fully opaque or translucent/unknown
- source freshness for file-backed assets, such as modified timestamp and file length when available
- visual inputs: source path or embedded missing-icon key, destination dimensions, tint, and multicolor mode

Cache hits should blit cached pixels into the destination buffer without recording new icon/image raster time. Cache misses should continue recording raster time.

### Correctness Rules

- Never return a stale file-backed cache entry if conservative metadata changed.
- Treat missing metadata as cacheable only for the current source identity if no safer freshness data is available, or bypass caching if correctness is unclear.
- Theme color changes should miss naturally through tint in the key.
- Multicolor source preservation must stay separate from monochrome tinting.
- Unknown alpha must be treated as translucent.

### Text/Glyph Proof

Phase 30 should not replace the text layout architecture. It should harden proof that:

- repeated measure/render/selection paths reuse layout caches;
- cache misses happen when shaping inputs change;
- glyph cache hits stay out of raster timing;
- text cache metrics continue to flow into profiling snapshots.

### Opaque/Translucent Metadata

Fully opaque cached resources can be identified from alpha pixels. That metadata should be retained with the cache entry. Any painter optimization that uses the flag must be conservative and tested. It is acceptable for Phase 30 to expose metadata and use it for proof first, leaving advanced blend/background policy tuning to Phase 31.

## Verification Strategy

Recommended commands:

```text
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render icon
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render glyph
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render text_cache
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling
```

## Pitfalls

- Path-only raster caches can serve stale output after file changes.
- Caching decoded images but not resized/tinted variants does not remove steady-state resize cost.
- SVG cache keys must include tint/multicolor mode and dimensions.
- Missing-icon fallback can silently dominate icon raster time if it is not cached.
- Opaque metadata must not assume opacity from file extension; inspect actual alpha.
- Cache metrics must distinguish real zero-cost cache hits from missing/unavailable payloads.
