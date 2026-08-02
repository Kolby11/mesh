# Direct SHM paint target

## Goal

Remove the extra full-frame copy from `PixelBuffer` into a Wayland SHM slot on
full presents, without weakening retained comparison or partial-damage
correctness.

## Current boundary

`Shell` paints display lists into its owned `PixelBuffer`. The presentation
layer subsequently selects an available `SlotPool` canvas and copies either
the current damage or the complete buffer into that mapping. The SHM slot is
not available until presentation, so the renderer cannot target it directly.

Borrowing that mapping as a temporary `Vec<u8>` would be unsound: `SlotPool`
owns it and may reuse it after the callback. Painting direct and then copying
the whole mapping back into `PixelBuffer` preserves correctness but merely
reverses the same bandwidth cost.

## Design

Introduce an explicit per-frame `RasterTarget` selected before painting:

1. Presentation reserves an available SHM slot and exposes a scoped
   BGRA-premultiplied target (data, physical extent, stride) for one full
   present. The scope cannot outlive the slot borrow.
2. The renderer accepts that scoped target as well as an owned `PixelBuffer`.
   It must retain one Skia canvas session across display-list, glyph, icon,
   filter, tooltip, and debug-overlay work; no renderer code may assume a
   growable `Vec`.
3. After paint, presentation commits the reserved slot directly. Its damage
   state records a complete, current frame. `PixelBuffer` remains the source
   for sparse repaint/compare paths, not an unconditional mirror of every
   direct full paint.
4. If the next repaint is sparse and no current retained copy exists, promote
   that target back to an owned retained buffer once, or take the existing
   full-paint path. The per-buffer generation/damage state must make this
   transition explicit; never reconstruct a stale slot by copying unrelated
   damage.
5. Keep the existing byte-copy path for partial presents, unavailable slots,
   resized/fractional-scale transitions, DevWindow, and testing.

## Required verification

- Pixel-identical direct/full and existing-copy output across transparent,
  text, icon, filter/blur, tooltip, and fractional-scale cases.
- Multi-slot reuse test: direct full frame followed by sparse damage on a
  different available slot cannot expose stale pixels.
- Resize and viewport-cropped SHM tests preserve stride and visible extent.
- Release gate: repeated full presents of a representative display list,
  recording paint plus present time and copied bytes. It must report machine,
  build profile, workload shape, before/after ranges, and gate name in
  `performance-log.md`.

## Non-goals

- No unsafe long-lived alias from a `PixelBuffer` to `SlotPool` memory.
- No change to partial-damage semantics or Wayland buffer-release ownership.
- GPU/EGL presentation remains the separate planned backend.
