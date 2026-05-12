# Phase 30 Discussion Log

**Date:** 2026-05-12
**Mode:** Smart discuss via `$gsd-autonomous`

## Grey Area 1/3: Cache Boundaries and Keys

Accepted all recommended answers:

- Cache rasterized icon/image output variants first.
- Keep cache ownership in `mesh-core-render`.
- Key by source identity, rendered dimensions, tint/multicolor mode, icon axes, and conservative freshness metadata.
- Invalidate through explicit visual input changes and conservative file metadata changes.

## Grey Area 2/3: Correctness, Metrics, and Fallbacks

Accepted all recommended answers:

- Cache hits must be visually identical.
- Existing fallback behavior remains.
- Proof uses existing raster/text profiling counters.
- Opaque/translucent metadata is conservative.

## Grey Area 3/3: Scope and Sequencing

Accepted all recommended answers:

- Phase 30 focuses on renderer-owned cache hardening and deterministic proof.
- Basic bounded capacity is in scope if needed for safety.
- Sophisticated tuning and visible-smoothness acceptance are deferred to Phase 31.
- Text/glyph cache work remains in scope, focused on preserving and extending existing caches.
