Span takes defaultly full width of the parent component, the tags should initially take the space as possible. So defaultly the size of text inside

Icon rendering using icon packs

Settings module to manager modules and core settings like theme and i18n

Popups, also with custom content rendering if users desire

Keybind management

Layer system, so that we can specify what to render on what layer

make sure positioning system works (relative, absolute, fixed)

Variable state management and and binding for components

Clean up the backend modules and interfaces, right now interfaces are separate from the backend, we should check our options and consider moving the interface into the module itself

Remove the icon assts from the core/ui. The icons should be installed into a folder outside the core

# Separate milestons

- GPU rendering
- i18n configurations
- package manager
- lsp / extension
- unify configurations to use .json configuration
- Improve Icon packs
- Keyboard control with custom keybinds
- 



# Major performance fixes

See `docs/performance-roadmap.md` for the durable roadmap.

Current retained-rendering status:

- Stable runtime node IDs are implemented from `_mesh_key`.
- Style-only renders now mutate the retained cached `WidgetNode` tree instead of
  cloning it.
- Retained widget-tree dirty summaries now track inserted, removed, layout,
  style, attribute, child-order, and state changes by stable node ID.
- Full dirty renders still rebuild the widget tree.
- There is not yet a retained render-object tree, retained display list,
  incremental layout consumer, or damage tracking.

Implementation order:

1. Retained widget tree with stable node identity and dirty summaries. Done for
   the widget layer.
2. Dirty-type invalidation for script/state, style, layout, paint, text,
   accessibility, metrics, and surface configuration.
3. Incremental style and layout propagation.
4. Retained display list plus damage tracking.
5. Text shaping and glyph cache.
6. Typed attribute/style slots and interned identifiers.
7. Selector indexing for restyle.
8. Display-list batching.
9. GPU backend through wgpu/Vulkan/OpenGL.
10. Parallel paint/layout after ownership bottlenecks are removed.

Key sequencing note: adding a GPU backend before retained rendering gives
diminishing returns, because rebuilding the tree and uploading brand-new paint
data every frame wastes much of the GPU win. The right sequence is retained
tree, dirty invalidation, damage tracking, retained display list, then GPU.

Two ways to fix the correctness bug                                                           
                                                                             
  A. Cheapest, restores correctness: drop partial damage and always go through the full-clear + 
  full-paint branch. Keeps the tree_has_icon + clear_rect wins from the last pass, surrenders
  the partial-paint speedup. ~5 lines deleted.                                                  
                                                                             
  B. Proper: move the PixelBuffer ownership into FrontendSurfaceComponent (or a per-surface slot
   in Shell) so it persists across paints. Reallocate only on size change. Then the
  partial-damage branch actually reuses old pixels, and damage tracking becomes a real          
  optimization. Also enables sending wl_surface.damage_buffer regions to the compositor instead
  of treating every commit as a full surface change.

  I'd recommend B — A is a regression band-aid.                                                 
   
  Why "overall rendering is still slow" — gap to Qt                                             
                                                                             
  Even after fixing damage retention, the pipeline is software-rasterizing everything on the    
  CPU, single-threaded, and reallocating per frame. The big items, ordered by impact:
                                                                                                
  1. Per-frame full surface allocation — PixelBuffer::new zeroes a fresh Vec<u8> of W*H*4 bytes 
  every render call. For a 1920×48 panel that's ~370 KB of Vec allocation + zeroing every frame
  just to throw it away. Same fix as B above.                                                   
  2. Per-frame wl_buffer allocation — wayland_surface/backend.rs:335 calls   
  pool.create_buffer(...) and copies pixels in every present. Wayland clients normally maintain 
  a 2–3 buffer SHM pool and recycle them on wl_buffer::release. Right now each frame is a fresh
  shm allocation + memcpy.                                                                      
  3. Tree-walking and command rebuild every paint — collect_paint_commands clones every
  WidgetNode (paint_node = node.clone()) into a Vec<DisplayPaintCommand> on every paint, even   
  when the tree hasn't changed. The retained-tree generation is already tracked; gate command
  list rebuild on it.                                                                           
  4. Software path for everything — borders, rounded corners, gradients, shadows, glyph blending
   all go through set_pixel/blend_pixel with bounds checks per pixel. Qt is GPU-accelerated via 
  QRhi/Vulkan/OpenGL. Two practical options:
    - Tiny-skia upgrade: keep the CPU pipeline but route geometry through tiny_skia::PixmapMut  
  (already a transitive dep through resvg). Tiny-skia is SIMD-aware and ~10–50× faster than     
  hand-rolled set_pixel loops for primitives.
    - GPU: replace PixelBuffer with a wgpu-backed renderer (use dmabuf linux-dmabuf-v1 instead  
  of shm). This is the actual Qt-parity path but it's a multi-week change.                      
  5. Glyph atlas — text is currently re-rasterized + cached per-string. Qt keeps a per-font GPU
  glyph atlas keyed on (font, glyph_id, subpixel-position, weight). On the CPU side, even a     
  HashMap<(FontId, char, subpx), Bitmap> glyph cache (vs. string-level cache) would help.
  6. Image cache hands out clones — crates/core/ui/icon's RgbaImage::clone() per lookup is a    
  full pixel copy. Should hand out Arc<RgbaImage>.                                              
  7. No frame-throttling to compositor frame callbacks — paints fire on Tokio ticks, not on
  wl_surface::frame callbacks. You can paint at 1000 Hz and it just gets dropped, while still   
  burning CPU.                                                               
                                                                                                
  My recommendation                                                                             
                                         
  Order I'd attack this in:                                                                     
                                                                             
  1. Fix the disappearing-surface bug now — Option B (persistent buffer in component or shell). 
  Without this, none of the rest matters.
  2. Reuse SHM buffers — at least 2-deep pool; biggest single win after #1, no API change.      
  3. Gate collect_paint_commands and RenderObjectTree work on retained-tree generation — skip   
  command/object rebuild when nothing changed.                                                  
  4. Route primitives through tiny_skia — keeps the architecture, enormous CPU paint speedup.   
  Already half-pulled-in via resvg.                                                             
  5. Frame-callback driven paint — paint when the compositor asks, not on a timer.
  6. (Long-horizon) wgpu backend — actual Qt parity.                                            
                                                                                                
  Want me to start with (1) — make the buffer persistent so the shell stops vanishing — and we  
  can stage the rest after? That's a contained change in render.rs + FrontendSurfaceComponent   
  and unblocks evaluating the partial-damage path on its own merits.  

    What that means for MESH               

  You don't need a GPU to be Qt-fast. You need:                                                                    
   
  1. A SIMD-aware software rasterizer (tiny-skia gets you there for free).                                         
  2. A glyph atlas / cache keyed on (font, glyph, subpixel) — Qt has one, you don't yet.
  3. Real damage-region tracking (the work you started, once the persistent-buffer bug is fixed).                  
  4. Fewer per-frame allocations (SHM pool, no tree clones).                                                       
                                                                                                                   
  GPU only becomes necessary when you want effects Qt's software path also drops: heavy blurs, large filter chains,
   very high-resolution surfaces (4K+ at 120 Hz). For a panel, launcher, notification center on a normal monitor, a
   tuned CPU pipeline is enough — that's exactly the regime Qt's software backend was built for, and it's smooth.  
                                                                             
  So: tiny-skia is the right next step after fixing the disappearing-surface bug. It buys you most of Qt's no-GPU  
  performance without committing to wgpu.
                                             