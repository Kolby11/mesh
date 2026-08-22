# Status

**Updated:** 2026-08-23

## Now

Icon bitmap/SVG and font-glyph preparation share one render resource broker.
The broker owns a single bounded worker queue, a 32 MiB byte-admission budget,
resource-revision cancellation tokens, and cancellation callbacks that release
typed result reservations without leaving pending keys stuck. Typed icon/glyph
caches and shell polling remain separate at the handoff boundary.

External SVG one-shot results retain fingerprints for every resolvable linked
file. Queue polling and handoff reject a result when a linked file changes, so
mutable linked assets cannot publish pixels prepared from an older dependency
set. The broker falls back to the existing synchronous path if its worker
cannot be started.

The text renderer's Skia glyph atlas now keys cached images by resource
revision and cosmic glyph identity, bounds entries at 8 MiB with an LRU byte
budget, and rejects oversized or malformed raster dimensions before upload.

Focused render/resource tests and `cargo check --workspace` pass. Existing
shell dead-code/private-interface warnings remain. The focused real-surface
navigation raster fixture still has the known 1280px layout overflow.

## Next

Extend the generation-aware resource broker and byte/dimension accounting to
text layout `Buffer` storage, then `PixelBuffer` and SHM resources before
continuing frontend/backend candidate preparation.
