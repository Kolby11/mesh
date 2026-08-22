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

The shaped-text layout cache now uses a conservative 16 MiB byte budget in
addition to its 512-entry cap. It reports resident/max bytes, counts
byte-driven evictions, and leaves layouts above safe input or estimate bounds
renderable without retaining them.

`PixelBuffer` now keeps dimensions and backing storage private, exposes only
read-only dimensions and slice access, and provides checked, bounded,
fallible allocation. Dynamic shell surface and child-surface allocation uses
that fallible path, so a live `PixelCanvasSession` cannot observe a backing
allocation replacement.

Wayland SHM pool configurations now validate rounded dimensions, stride, and
byte length before entering `SlotPool`. Each surface accounts for retained SHM
bytes, and shared pool growth is capped at 512 MiB while per-buffer and
per-surface limits reject oversized allocations deterministically.

Focused render/resource tests and `cargo check --workspace` pass. Existing
shell dead-code/private-interface warnings remain. The focused real-surface
navigation raster fixture still has the known 1280px layout overflow.

## Next

Make Wayland presentation transactional across copy/attach failures: retain
pending damage, propagate attach errors, and avoid acknowledging an unshown
frame. Text shaping remains synchronous and is not yet broker-owned.
