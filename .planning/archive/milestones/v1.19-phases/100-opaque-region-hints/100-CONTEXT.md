# Phase 100: Opaque Region Hints - Context

**Gathered:** 2026-06-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Walk the retained display list for each surface after rendering to find root-level fully-opaque background rects. Compute the union of those rects as a `wl_region`, send `wl_surface::set_opaque_region` from the present path, and destroy the region immediately after (Wayland protocol copy semantics). Conservative gating: skip opaque region entirely when background is translucent, uses gradients/images, has border-radius, or surface is not `Overflow::Hidden`.

This phase delivers compositor compositing optimization with zero visual regressions on shipped surfaces.
</domain>

<decisions>
## Implementation Decisions

### Opaque Region Computation
- Shell computes opaque rects from the retained display list after `render_components()`, passes rects to a new `PresentationEngine::update_opaque_region(surface_id, Option<Rect>)` method — keeps display-list knowledge out of presentation crate
- Root node background only — walk the display list's root `DisplayPaintStyle`, check `background_color.a == 255` AND `background_paint` is `BackgroundPaint::Color` with no effects
- Guard conditions: skip opaque region if `background_color.a != 255`, or `background_paint` is not `BackgroundPaint::Color`, or surface has `border-radius > 0`, or `overflow != Overflow::Hidden`

### wl_region Lifecycle
- Create `wl_region` via `WlCompositor::create_region()`, add rects, call `set_opaque_region(Some(region))`, then `region.destroy()` — create+set+destroy per present
- Send `set_opaque_region(None)` to clear any previously-set region when the surface has no opaque rects

### OpenCode's Discretion
- Exact surface dimension retrieval for opaque rect union calculation
- How to access `border-radius` and `overflow` from the display list root
- Method signature details for `update_opaque_region` on `PresentationEngine`
- Whether the opaque rect union needs damage-rect-aware filtering
</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `WlCompositor` already bound via `CompositorState::bind()` at `backend.rs:487`
- `wl_surface` already available in `SurfaceEntry` during present
- `DisplayPaintStyle` at `display_list.rs:222-224` — has `background_color: Color` and `background_paint: BackgroundPaint`
- `DisplayPrimitiveSlot::Background` at line 17 — slot type for background
- `RetainedDisplayList::paint_commands` at line 183 — `Arc<[DisplayPaintCommand]>`
- `PresentationEngine` already has `surface_waiting_for_frame_callback()` — precedent for surface-scoped query methods

### Established Patterns
- `PresentationEngine` trait methods delegate to backend-specific implementations
- `present_with_damage()` at `backend.rs:637` — the present path entry point before `attach_shm_buffer`
- Shell loop order: `render_components()` → `flush_wayland()` → `presentation_engine.pump()` — opaque rect update should go between render and flush

### Integration Points
- Shell loop at `mod.rs:188-190` — `render_components()?; self.flush_wayland()?; self.presentation_engine.pump();`
- `SurfaceEntry` at backend.rs — holds `wl_surface` and surface configuration (size, layer)
- `CompositorState` at backend.rs — provides `wl_compositor()` for `create_region()`
</code_context>

<specifics>
## Specific Ideas

No specific user requirements beyond the research findings — implementation follows ROADMAP success criteria and research recommendations.
</specifics>

<deferred>
## Deferred Ideas

- Widget-level opaque rect analysis (walking all child nodes, not just root) — future optimization
- Multiple-opaque-rect union (subtracting translucent children from opaque parent) — deferred complexity
- Compositor compatibility matrix (Sway/Hyprland/KWin/Mutter behavior differences for set_opaque_region) — addressed in testing, not implementation
</deferred>
