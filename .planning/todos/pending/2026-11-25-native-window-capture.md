# Native window capture

Use the [ScreenCast portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html)
as the compositor-facing consent and window-selection mechanism. The active
Hyprland session advertises ScreenCast version 5 and the window source bit.
That query establishes availability. Transport and visual verification require
a user-selected test window.

A synthetic 16 × 16 ABGR8888 image round-tripped through independent EGL
contexts on `/dev/dri/renderD128` using the exported modifier and a native
acquire fence. All 1,024 bytes matched. The passing probe wrote the texture
after exporting its storage. Writing before export without preserving the
EGL image failed the pixel comparison. The probe did not open a portal session.

## Authority and lifetime

Shell owns the capability check, portal session and requesting instance
identity. Each request selects a single `WINDOW` with no persistent permission.
Accept one window stream from `Start` and connect to its node on the restricted
remote from `OpenPipeWireRemote`. Keep request and session objects attached to
the requesting instance generation. The portal picker names the actual source.
Surface selection does not take a backend provider's window ID as authority.

Tie the preview handle to the authorized audience at resource resolution.
Publish capture status and a stop action through a core-owned channel that a
shell module can present as a privacy indicator. Stop, portal closure, owner
retirement, and lost capability all retire the stream. A delayed reply must
belong to the current session before it can publish. Invalidation must cover
already-recorded scenes and outputs queued for presentation, as well as new
resource lookups. Test cancel and retirement while a request, GPU import, or
presentation is in flight.

## Buffer transport

Negotiate formats through [PipeWire's DMA-BUF contract](https://docs.pipewire.org/1.4/page_dma_buf.html).
The stream worker owns dequeue and requeue. A frame descriptor carries the
fourcc, modifier, plane offsets and strides, physical dimensions, and acquire
and release synchronization. Keep ownership through GPU completion and bound
the producer pool. Coalesce new frames and release skipped leases through the
same worker. A PipeWire explicit-sync timeline can be converted to a fence at
the EGL boundary. Query the GPU worker's supported formats and modifiers and
offer only the combinations its import path supports.

The existing GPU scene records raster Skia images on the UI thread. EGL and
Skia GPU images must be created on the GPU worker. The workspace's
`skia-safe` 0.97 Ganesh bindings expose borrowing an existing backend texture
and leave promise-texture creation unbound. Prove a deferred texture draw that
preserves the image's position in the display list, including masks and
compositing layers, then connect capture to that path. The CPU fallback
negotiates shared-memory packed formats and copies each valid frame through the
bounded CPU frame channel. Validate chunk extent, stride, format and size
before publishing. Reconfigure through negotiation when import is unavailable.

Verify denied and cancelled selection, unsupported source types, worker failure,
buffer backpressure, retirement, CPU pixels, and GPU pixels with a real fence.
Exercise the implemented path on a window-capable portal and the unavailable
path on one that advertises only monitors.
