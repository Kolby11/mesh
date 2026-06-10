# Architecture Research: MESH v1.20 Compositor Integration

## Summary

Three Wayland compositor protocol integrations are being added to MESH v1.20.
Each feature touches a different layer of the pipeline — HiDPI/fractional scale
runs through the full layout-render-present stack; compositor blur offload is a
present-time protocol signal derived from the existing display list; per-region
damage is a present-path improvement that the retained renderer already
computes internally but does not yet expose as a multi-rect vector to the
presentation layer.

The recommended build order is: per-region damage first (purely additive change
confined to shell→presentation handoff), fractional scale second (wider change
that touches layout, PixelBuffer sizing, and presentation together), compositor
blur offload last (pure protocol addition on top of the finished present path).

---

## Feature 1: HiDPI / Fractional Scale

### Overview

The compositor reports a per-surface scale factor via `wl_surface::enter` +
`wl_output` integer scale (wl_output protocol v2+) or via the
`wp_fractional_scale_v1` + `wp_viewporter` extension pair for sub-integer
ratios. MESH currently renders at a fixed logical pixel size and hands the
resulting `PixelBuffer` to `SurfaceEntry::copy_into_shm_buffer`. There is no
scale factor in the rendering pipeline; `PixelBuffer::new(width, height)` is
always called with logical sizes.

### Integration Points

**`crates/core/presentation/src/wayland_surface/state.rs` — `State` struct**
Owns `output_state: OutputState` but currently does not track per-surface scale
factors. A `surface_scale: HashMap<String, f64>` field (or per-surface scale on
`SurfaceEntry`) must be added.

**`crates/core/presentation/src/wayland_surface/handlers.rs` — `CompositorHandler for State`**
`scale_factor_changed` is the existing hook for integer scale changes. It
currently does nothing. This is the primary wiring point for integer scale
events. For fractional scale the protocol fires a dedicated `preferred_scale`
event on the `wp_fractional_scale_v1` object; a new `Dispatch` impl is needed.

**`crates/core/presentation/src/wayland_surface/backend.rs` — `SurfaceEntry` and `LayerShellBackend`**
`SurfaceEntry` must carry `scale_factor: f64` (default `1.0`). The public
`surface_size_if_known` method that the shell consults to size its PixelBuffer
must return physical pixels (logical * scale), not logical pixels. A new method
`surface_scale` should expose the current scale for a surface id so the shell
can pass it to the renderer.

**`crates/core/presentation/src/lib.rs` — `PresentationEngine`**
A new method `surface_scale(&self, surface_id: &str) -> f64` must be added and
forwarded to `LayerShellBackend`. DevWindow always returns `1.0`.

**`crates/core/frontend/render/src/surface/buffer.rs` — `PixelBuffer`**
`PixelBuffer::new(width, height)` does not need to change structurally, but
callers must pass physical pixel dimensions. No internal change is required.

**`crates/core/shell/src/shell/runtime/render.rs` — `Shell::render_components`**
The call site that allocates `PixelBuffer::new(width, height)` (around line 172)
currently uses logical pixel dimensions from `surface_size_if_known`. It must
query `presentation_engine.surface_scale(surface_id)` and multiply width/height
before allocation, then pass the scale to `paint_frontend_tree_at_for_module`
(the `scale` parameter already exists on that function — it is currently
hardcoded to `1.0` or the logical pixel equivalent throughout the shell's paint
path).

**`crates/core/frontend/render/src/surface/mod.rs`**
`paint_frontend_tree` and related exported functions already accept a `scale:
f32` parameter. Verify all call sites in `shell_component.rs` and
`render.rs` pass the new per-surface scale rather than a constant.

**Layout: Taffy geometry**
Taffy works in CSS px (logical units). The scale multiplier is applied at paint
time. CSS px geometry does not need to change.

### New Components

**`wp_fractional_scale_v1` binding** in `crates/core/presentation/Cargo.toml`:
```toml
wayland-protocols = { version = "0.32", features = ["client", "staging"] }
```
The `wp_fractional_scale_v1` and `wp_viewporter` protocols live in
`wayland-protocols` staging. Both are needed: fractional scale delivers the
preferred scale; viewporter maps the physical-pixel buffer to the logical
surface size so the compositor composites without blur. The existing
`wayland-protocols-hyprland` crate does not contain these.

**`FractionalScaleManager` optional global** in `State` (alongside the existing
optional `activation_state`, `focus_grab_manager`). Bind from the registry with
`globals.bind(&qh, 1..=1, GlobalData).ok()`.

**`wp_viewporter::Viewport` per surface** — after creating the `LayerSurface`
in `LayerShellBackend::configure`, also create a `Viewport` for the surface and
store it on `SurfaceEntry`. The `set_destination` call on the viewport must be
made before each `wl_surface::commit` so the compositor scales down the
physical buffer back to logical size.

### Modified Components

| File | Change |
|------|--------|
| `state.rs` | Add `fractional_scale_manager: Option<WpFractionalScaleManagerV1>` and `viewport_manager: Option<WpViewporter>` |
| `backend.rs` | `SurfaceEntry`: add `scale_factor: f64`, `fractional_scale: Option<WpFractionalScaleV1>`, `viewport: Option<WpViewport>` |
| `handlers.rs` | Implement `scale_factor_changed` to update `SurfaceEntry::scale_factor`; add `Dispatch<WpFractionalScaleV1, ...>` for preferred-scale events |
| `lib.rs` | Add `PresentationEngine::surface_scale(surface_id) -> f64` |
| `render.rs` (shell) | Multiply surface size by scale before `PixelBuffer::new`; pass scale to paint functions |
| `attach_shm_buffer` in `backend.rs` | Call `viewport.set_destination(logical_w, logical_h)` before commit when scale != 1.0 |

### Data Flow Changes

```
OutputHandler / WpFractionalScaleV1::preferred_scale event
  -> SurfaceEntry::scale_factor updated in State

Shell::render_components
  -> presentation_engine.surface_scale(surface_id)   [new query]
  -> PixelBuffer::new(logical_w * scale, logical_h * scale)   [physical px]
  -> component.paint(..., physical_w, physical_h, buffer)
  -> paint_frontend_tree_at_for_module(..., scale=scale_factor, ...)
    -> Taffy layout still uses logical px, painter multiplies by scale when blitting

LayerShellBackend::attach_shm_buffer
  -> wl_surface.damage_buffer(...)     [already physical px]
  -> viewport.set_destination(logical_w, logical_h)   [new: tell compositor logical size]
  -> wl_surface.commit()
```

### Protocol Lifecycle Concerns

`wp_fractional_scale_v1` must be created before the first commit on a surface.
If the compositor does not advertise the manager global, fall back gracefully to
integer scale from `scale_factor_changed`. `wp_viewporter` is widely supported
(all major compositors since ~2021). Bind both as optional; present without them
if unavailable, accepting slight blur on HiDPI compositors that use fractional
scale without viewporter support.

---

## Feature 2: Compositor Blur Offload

### Overview

Some compositors (KWin/KDE, Hyprland via `org_kde_kwin_blur_v1`) support
setting a blur region on a surface. When set, the compositor blurs whatever is
behind the surface within that region before compositing the surface's own
pixels on top. This replaces the CPU-side Skia blur in `backdrop-filter` with a
zero-cost compositor-side effect.

MESH already tracks `backdrop_filter: VisualFilter` (which has `blur_radius:
f32`) on `DisplayPaintStyle` inside `RetainedDisplayList`. The display list
walker already identifies backdrop-filter nodes during the paint traversal.
The only missing piece is: detect the protocol, compute a union of all
backdrop-filter node rects in the display list, and send that rect to the
compositor as a blur region alongside each surface commit.

### Integration Points

**`crates/core/presentation/src/wayland_surface/state.rs` — `State`**
Add `kde_blur_manager: Option<OrgKdeKwinBlurManager>` (optional global, bound
with `globals.bind(&qh, 1..=1, GlobalData).ok()`).

**`crates/core/presentation/src/wayland_surface/backend.rs` — `SurfaceEntry`**
Add `kde_blur: Option<OrgKdeKwinBlur>` to hold the per-surface blur object.

**`crates/core/presentation/src/lib.rs` — `PresentationEngine`**
Add a new method:
```rust
pub fn update_blur_region(
    &mut self,
    surface_id: &str,
    blur_rect: Option<DamageRect>,
)
```
Called from `Shell::render_components` alongside the existing
`update_opaque_region` call.

**`crates/core/shell/src/shell/runtime/render.rs` — `Shell::render_components`**
After the existing `update_opaque_region` call (around line 241), add a
`update_blur_region` call. The blur rect must be computed from the display list
paint commands by walking `DisplayPaintCommand` entries where
`node.style.backdrop_filter.blur_radius > 0.0` and unioning their
`node.layout` rects.

**`crates/core/shell/src/shell/component/shell_component.rs`**
The public method `display_list_paint_commands()` already exposes the command
list to `Shell::render_components`. A new method `blur_region_rect()` should
compute and return `Option<DamageRect>` by walking the retained display list
commands. Alternatively the computation can live inline in `render.rs`, but
encapsulating it on the component avoids exposing the entire command slice.

### New Components

**`org_kde_kwin_blur_v1` binding** — this protocol is only in
`wayland-protocols-kde` or as part of `wayland-protocols-hyprland`. Add a
dependency:
```toml
wayland-protocols-kde = { version = "0.3", features = ["client"] }
```
The `org_kde_kwin_blur` protocol provides a manager object and per-surface blur
objects with `set_region` and `commit`. The blur region is a `wl_region`
(composited via `wl_compositor`). `wl_region` is already used for
`set_opaque_region` in `update_opaque_region` — the `Region::new(&compositor_state)`
pattern from that method transfers directly.

### Modified Components

| File | Change |
|------|--------|
| `state.rs` | Add `kde_blur_manager: Option<OrgKdeKwinBlurManager>` |
| `backend.rs` | `SurfaceEntry`: add `kde_blur: Option<OrgKdeKwinBlur>`. In `attach_shm_buffer`, if blur is set, call `kde_blur.set_region(region)` + `kde_blur.commit()` before `wl_surface.commit()` |
| `lib.rs` | Add `PresentationEngine::update_blur_region` forwarded to `LayerShellBackend` |
| `render.rs` (shell) | After `update_opaque_region`, call `update_blur_region` with the computed rect |
| `shell_component.rs` | Add `blur_region_rect() -> Option<DamageRect>` |

### Protocol Lifecycle Concerns

`org_kde_kwin_blur_v1` is compositor-specific. If the global is absent, skip
silently; the CPU Skia path continues to run. The blur region is committed
per-frame (not lazily) because the surface content and backdrop-filter nodes
can move. If the blur region is empty (no backdrop-filter nodes), call the blur
object's `set_region(None)` to remove any previously set region before commit.
Ordering: blur region update must precede the surface commit in the same
`EventQueue::flush()` cycle. The existing `attach_shm_buffer` path ends with
`layer_surface.commit()`, so the blur update call belongs just before that
commit. Blur rects must be in physical pixels after Phase 2 (fractional scale);
scale the `node.layout` rect by `scale_factor` when constructing the region.

---

## Feature 3: Per-Region Damage

### Overview

MESH's retained renderer already computes per-node damage rects inside
`FrontendSurfaceComponent`. The `select_effective_damage_rects` function returns
an `EffectiveDamage` struct that has both a unified bounding `rect:
Option<DamageRect>` and a `rects: Vec<DamageRect>` with individual dirty
regions. The paint path conditionally uses the multi-rect vector for
`select_paint_commands_for_rects` vs the single-rect `select_paint_commands`.
However `take_present_damage()` (shell_component.rs line 897) only exposes
`last_present_damage: Option<DamageRect>` — a single bounding rect — to
`Shell::render_components`, which passes that single rect to
`PresentationEngine::present_with_damage`.

`wl_surface::damage_buffer` accepts one rect per call and can be called
multiple times before a commit. This allows the compositor to skip compositing
the undamaged regions of the surface, improving throughput when damage is small
and scattered.

### Integration Points

**`crates/core/shell/src/shell/component/shell_component.rs`**
`take_present_damage()` returns `Option<DamageRect>` (single rect). This must
change to return `Option<Vec<DamageRect>>`. The existing `last_present_damage:
Option<DamageRect>` field changes to `last_present_damage_rects:
Option<Vec<DamageRect>>`. `EffectiveDamage::rects` is already populated at
line 685 (`effective_damage_scratch = std::mem::take(&mut effective_damage.rects)`);
expose it through `take_present_damage`.

The conservative fallback remains: if `effective_damage.full_surface` is true,
return a single full-surface rect (or `None` to let the caller use a full
buffer), as today.

**`crates/core/shell/src/shell/component.rs` (the `ShellComponent` trait)**
The `take_present_damage` method signature must change from
`fn take_present_damage(&mut self) -> Option<DamageRect>` to
`fn take_present_damage(&mut self) -> Option<Vec<DamageRect>>`.

**`crates/core/shell/src/shell/runtime/render.rs` — `Shell::render_components`**
The call to `take_present_damage()` currently assigns a single
`Option<DamageRect>`. After the signature change, iterate the vec and pass rects
to a new `present_with_damage_rects` method on `PresentationEngine`. The
existing `present_with_damage(single_rect)` can be kept as a deprecated
forwarding wrapper for DevWindow.

**`crates/core/presentation/src/lib.rs` — `PresentationEngine`**
Add:
```rust
pub fn present_with_damage_rects(
    &mut self,
    surface_id: &str,
    title: &str,
    visible: bool,
    buffer: &PixelBuffer,
    damage_rects: Option<&[DamageRect]>,
) -> Result<(), PresentationError>
```
DevWindow backend ignores damage rects. Wayland backend maps each to a
`wl_surface::damage_buffer` call.

**`crates/core/presentation/src/wayland_surface/backend.rs` — `SurfaceEntry`**
`SurfaceShmBuffer::pending_damage` (currently `Option<DamageRect>` at line 75)
changes to `Vec<DamageRect>` to accumulate multiple outstanding rects per slot
for the triple-buffering catch-up logic.

`copy_into_shm_buffer` is called with `damage: Option<DamageRect>` today. It
must accept `damage_rects: Option<&[DamageRect]>` and accumulate the slice into
the per-slot vec via union or append. The `copy_bgra_damage_to_canvas` helper
is called once per rect (each already clips independently).

`attach_shm_buffer` currently calls `wl_surface.damage_buffer` once with the
single accumulated rect. Change to iterate `damage_rects: &[DamageRect]` and
call `wl_surface.damage_buffer` once per rect.

### Modified Components

| File | Change |
|------|--------|
| `shell_component.rs` | `take_present_damage` returns `Option<Vec<DamageRect>>`; `last_present_damage` becomes `Option<Vec<DamageRect>>` |
| `component.rs` (trait) | `take_present_damage` signature update |
| `render.rs` (shell) | Use `present_with_damage_rects`; iterate damage vec |
| `lib.rs` (presentation) | Add `present_with_damage_rects` forwarding |
| `backend.rs` (presentation) | `SurfaceShmBuffer::pending_damage` becomes `Vec<DamageRect>`; `copy_into_shm_buffer` and `attach_shm_buffer` accept slices; call `damage_buffer` per rect |

### Data Flow Changes

```
RetainedDisplayList::update_with_dirty_nodes / update_for_retained_generation
  -> EffectiveDamage { rects: Vec<DamageRect>, rect: Option<DamageRect>, ... }

FrontendSurfaceComponent::paint
  -> last_present_damage_rects = Some(effective_damage.rects)   [was single rect]

Shell::render_components
  -> component.take_present_damage() -> Option<Vec<DamageRect>>
  -> presentation_engine.present_with_damage_rects(surface_id, ..., rects.as_deref())

LayerShellBackend::present_with_damage_rects (Wayland path)
  -> entry.copy_into_shm_buffer(..., damage_rects)
  -> entry.attach_shm_buffer(..., damage_rects)
    -> for each rect: wl_surface.damage_buffer(rect.x, rect.y, rect.width, rect.height)
  -> layer_surface.commit()
```

The per-buffer pending-damage accumulation in `SurfaceShmBuffer` continues to
union all pending rects per slot (to handle the triple-buffer catch-up case when
a buffer was busy while damage accumulated). On reuse, the accumulated rects
plus the current frame's rects become the damage committed for that buffer.

---

## Suggested Build Order

### Phase 1: Per-Region Damage (do first)

**Why first:** The damage rect plumbing is already internally correct in the
retained renderer. The only missing link is the handoff from
`take_present_damage` through `Shell::render_components` to
`attach_shm_buffer`. The change is confined to the shell-to-presentation
boundary with no new crate dependencies and no layout or scale changes. It is
also the lowest risk: the existing single-rect path degrades gracefully if a
compositor ignores or coalesces multiple `damage_buffer` calls.

Damage coordinates are currently in logical pixels. This is still correct for
the wl_surface damage protocol because at integer scale 1.0 logical == physical.
After Phase 2 introduces fractional scale, the damage rects will naturally be in
physical pixels because `PixelBuffer` will be allocated at physical size.

**Scope:** `shell_component.rs`, `component.rs` trait, `render.rs`,
`lib.rs` (presentation), `backend.rs` (presentation). No new crate
dependencies. DevWindow path unchanged.

**Verification:** Existing presentation tests in `crates/core/presentation` cover
buffer copying and damage clipping; extend to cover multi-rect `damage_buffer`
calls. Existing pixel-equivalence tests in the shell component cover the damage
selection logic.

### Phase 2: HiDPI / Fractional Scale (do second)

**Why second:** Requires new crate dependencies (`wayland-protocols` with
staging features), new protocol objects per surface, changes to both the
presentation layer (SurfaceEntry, binding new globals) and the shell render
loop (scale factor query, PixelBuffer sizing). Building on a stable per-region
damage path means the damage coordinates (now in physical pixels post-scale) are
correct when fractional scale lands without a second coordinate-space migration.

**Scope:** `Cargo.toml` (presentation crate), `state.rs`, `backend.rs`,
`handlers.rs`, `lib.rs` (presentation), `render.rs` (shell). New protocol
binding boilerplate. DevWindow returns `scale=1.0` and is otherwise unchanged.

**Verification:** Test at integer scale (1x) for no regression; test at 2x with
a compositor that reports integer scale via `scale_factor_changed`; test
graceful fallback when `wp_fractional_scale_manager` global is absent.

### Phase 3: Compositor Blur Offload (do last)

**Why last:** Purely additive. Reads existing `DisplayPaintStyle::backdrop_filter`
data from the already-correct post-scale display list. Sends a protocol hint to
the compositor with no rendering logic changes. If the compositor global is
absent the feature is a no-op; the CPU Skia blur path continues unchanged.
Blur rects are derived from `node.layout` and must be in physical pixels — this
is only guaranteed correct after Phase 2. Building last avoids a coordinate bug.

**Scope:** `Cargo.toml` (presentation crate, new `wayland-protocols-kde`),
`state.rs`, `backend.rs`, `lib.rs` (presentation), `render.rs` (shell),
`shell_component.rs` (new `blur_region_rect` method).

**Verification:** Test on a compositor with `org_kde_kwin_blur_v1` (KWin) to
confirm blur appears behind surfaces. Test on a compositor without the protocol
to confirm no crash or regression. Test that clearing the blur region (no
backdrop-filter nodes) removes a previously committed blur region.

### Dependency Matrix

```
Feature                  | Depends on
-------------------------|---------------------------------------------------
Per-region damage        | Nothing new — purely additive to existing path
Fractional scale         | Stable present path (Phase 1 solidifies this)
Compositor blur offload  | Physical-pixel layout rect correctness (Phase 2)
```

### Cross-Feature Coordinate Space Note

After Phase 2 (fractional scale), all coordinates in `PixelBuffer` are physical
pixels. `DamageRect` used in `wl_surface::damage_buffer` calls is already in
physical pixels per the Wayland protocol. The `update_blur_region` rect
(Phase 3) must also be in physical pixels; derive it from `node.layout` rects
scaled by the surface's `scale_factor` when constructing the `wl_region` for
the blur object. Building in the recommended order ensures this is correct
without a later fix-up.
