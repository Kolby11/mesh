# Text Rendering Performance Notes

This file used to track the initial text-rendering integration work. That
integration is complete: text nodes, input text, tooltips, selection highlights,
ellipsis handling, shaped-layout caching, and glyph raster caching are all wired
through `mesh-core-render`.

Remaining text-rendering performance work:

- Share shaped-layout cache entries between layout measurement and paint. Done:
  `SharedTextMeasurer` and `FrontendRenderEngine` now use the same thread-local
  `TextRenderer`, so measurements performed during layout can be reused by paint
  on the same render thread.
- Improve first-miss ellipsis truncation by using shaped glyph advances instead
  of binary-search substring measurement.
- Add eviction/pressure visibility for text and glyph caches to profiling
  output, including layout-cache entry count, hits, misses, invalidations, and
  shaping time.
- Include locale/script/direction-sensitive text cases in canonical performance
  workloads before changing shaping behavior further.
