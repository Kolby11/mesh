# Phase 102: HiDPI / Fractional Scale — Context

**Gathered:** 2026-06-10
**Status:** Ready for planning
**Mode:** Auto-accepted (autonomous smart discuss)

<domain>
## Phase Boundary

Wire compositor-provided scale factors (integer via `wl_surface::preferred_buffer_scale`, fractional via `wp_fractional_scale_v1`) as authoritative scale sources. Allocate `PixelBuffer` at physical pixel dimensions (logical × scale). Pair with `wp_viewporter` for non-integer ratios so the compositor scales the buffer down to logical viewport. Layout/styling coordinates and the damage rect pipeline remain in logical CSS pixels throughout.

**Out of scope:** CSS `scale()` transforms (user-authored visual effects), font rendering at higher DPI (font backend concerns), changing the layout engine to use physical coordinates.
</domain>

<decisions>
## Implementation Decisions

### Scale Factor Plumbing

- Store `scale: f32` on `SurfaceEntry` (or the Wayland state), initialized from `wl_surface::preferred_buffer_scale()` (integer from compositor) and updated by `wp_fractional_scale_v1` events when available.
- Thread scale through `PresentationEngine::present` path so the render dispatch creates buffers at physical dimensions. The render layer only sees a `scale: f32` parameter — it should not care about protocol details.
- `wl_surface::set_buffer_scale(scale)` for integer scales. For fractional scales, use `wp_viewporter::set_destination(logical_w, logical_h)` with integer buffer_scale and viewport destination.

### Buffer Sizing Strategy

- `PixelBuffer::new(physical_width, physical_height)` where `physical = logical × scale`. Allocation `stride = physical_width * 4`. This is a single-site change since `PixelBuffer::new()` is called from one centralized render path.
- Skia paint surfaces and any intermediate framebuffer objects must also use physical dimensions so rasterized text/images are at native resolution.
- The SHM buffer (`pool.create_buffer`) receives physical dimensions so the compositor gets device-pixel data. Command bytes sent are `stride × physical_height` with damage rects in physical coordinate space.

### wp_viewporter Integration

- Add `wayland-protocols` (for `wp_fractional_scale_v1`) and `wayland-protocols-wlr` (for `wp_viewporter`) as optional dependencies in `mesh-core-presentation/Cargo.toml`.
- Bind `wp_viewporter` during compositor handshake (in `Connection::connect_to_env()`). If binding fails (non-KDE, non-wlroots compositor), skip — wp_viewporter is optional.
- On configure, create a viewport from the `wp_viewporter` global for the surface. On present, if scale is non-integer: set `set_buffer_scale(ceil(scale))`, `set_destination(logical_w, logical_h)`.
- For integer scale: use only `set_buffer_scale(scale)`. No viewporter usage.

### Scale Change Handling (Hotplug / Monitor Unplug)

- `wp_fractional_scale_v1::preferred_scale` events from compositor update the stored scale factor.
- On scale change: mark the surface as needing a full redraw (force resize path). Damage rects remain in logical space and are scaled to physical in the present path.
- `wl_surface::preferred_buffer_scale` events (integer scale) also trigger the same full-redraw path.
- The `surface_change_requires_fresh_configure()` check does NOT need to add scale — scale changes are handled via the same recomposition/buffer-resize path as config changes.

### Fallback Behavior (No wp_fractional_scale_v1)

- If `wp_fractional_scale_v1` is not available: use `wl_output::scale` integer via output state. The `scale_factor_changed` handler (currently no-op at handlers.rs:4) must store the scale and trigger redraw.
- If `wp_viewporter` is not available and scale is non-integer: round scale to nearest integer, use `set_buffer_scale(round(scale))`, accept slight sizing mismatch.
- If neither fractional-scale nor viewporter: fall back to integer-only scaling via `set_buffer_scale()`.

### OpenCode's Discretion

- Exact field placement for `scale` on `SurfaceEntry` vs a separate `ScaleState` struct.
- Whether `wp_fractional_scale_v1` events are handled inline in `handlers.rs` or in a separate handler module.
- Test strategy: unit tests for scale math (logical↔physical conversion), integration tests for buffer allocation at different scales.
</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets

- `scale: f32` parameter already flows through the entire render pipeline (`paint_selected_display_list_for_module_with_profiling_metrics`, `paint_layout_bounds`) — just needs to receive device-pixel scale instead of `1.0`.
- `scale_factor_changed()` handler exists at `handlers.rs:4` — currently no-op, needs to store and propagate.
- `LayerShellHandler::configure()` at `handlers.rs:98` already receives `new_size` from compositor — scale can be stored similarly.
- `CompositorHandler` trait at `handlers.rs:1` — already in place, just needs scale callback implementation.
- `OutputState` is bound at `backend.rs:94` — `output_state.info()` returns logical dimensions; output scale accessible via `output.info().scale_factor`.

### Established Patterns

- `SurfaceEntry` is the per-surface state container (`backend.rs:29-55`): stores config, dimensions, SHM pool index, configured flag, frame seq. Scale naturally fits here.
- Protocol dispatch uses `delegate_compositor!()` and `delegate_output!()` macros (`handlers.rs:492-494`). New protocol handlers (fractional-scale, viewporter) can follow the same pattern.
- `SlotPool` at `backend.rs:146-149` for SHM buffer pool with reallocation on resize. Physical-sized buffer allocation increases SHM memory usage — pool initial size should account for this.
- Render dispatch at `render.rs:245` gets buffer from `surface.body.buffer()` and calls `present_with_damage` → scale is a new parameter to thread.

### Integration Points

1. **Scale acquisition:** `handlers.rs:4` → store on `SurfaceEntry` or `State`
2. **Scale → render:** `render.rs:172` → `PixelBuffer::new(physical_w, physical_h)` instead of logical
3. **Scale → present:** `backend.rs:665` → `set_buffer_scale()`, optionally `set_destination()`
4. **Protocol binding:** `backend.rs:507` → `connect_to_env()` → bind `wp_fractional_scale_v1`, `wp_viewporter`
5. **Cargo.toml:** Add `wayland-protocols`, `wayland-protocols-wlr` features
6. **Damage rects:** Already in logical space from Phase 101 — must be scaled to physical before `damage_buffer` calls
</code_context>

<specifics>
## Specific Ideas

- Buffer allocation: `physical_width = ceil(logical_width * scale)`, same for height. Allocate `PixelBuffer` at physical size.
- Damage rect scaling: convert logical damage rects to physical before `damage_buffer` calls. `rect.x = (logical_x * scale) as u32`, same for y, width, height.
- `wp_fractional_scale_v1` sends `preferred_scale(scale: u32)` as 120× scale (1.0 = 120, 1.5 = 180, 2.0 = 240). Convert: `scale_f32 = preferred_scale as f32 / 120.0`.
- `wl_output::scale` is integer only (1, 2, 3). Used as fallback when fractional-scale protocol not available.
</specifics>

<deferred>
## Deferred Ideas

- Font rendering at higher DPI (separate font subsystem concern)
- GPU/OpenGL rendering path (v1.25)
- Per-monitor independent scale factors for multi-monitor (complex, defer)
- Exposing scale factor to Luau scripts/extension authors (not needed for v1.20 scope)
</deferred>
