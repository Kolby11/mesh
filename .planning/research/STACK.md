# Stack Research: MESH v1.20 Compositor Integration

## Summary

Three features are in scope: HiDPI/fractional scale, compositor blur offload, and per-region
damage commits. The existing dependency tree already contains the correct protocol bindings for
two of the three features — they just need to be wired up. Only blur offload requires a new
crate dependency.

**Existing stack baseline (confirmed from Cargo.toml and Cargo.lock):**
- `smithay-client-toolkit 0.19.2` — already pulls in `wayland-protocols 0.32.12` with `staging`
  feature enabled
- `wayland-client 0.31.14` — already in use for wl_surface, wl_output, etc.
- `wayland-protocols-hyprland 1.2.0` — already a direct dep in `mesh-core-presentation`
- `wayland-protocols-plasma 0.3.12` — NOT currently in the dep tree (new dep needed for blur)

---

## Crate Additions / Version Changes

### wayland-protocols v0.32 (direct dep, already transitive) — HiDPI + fractional scale + viewporter

- **Why needed:** `wayland-protocols` 0.32 is already pulled in transitively by
  `smithay-client-toolkit 0.19`, but `mesh-core-presentation/Cargo.toml` does not list it as
  a direct dependency. The `wp_fractional_scale_v1` and `wp_viewporter` bindings live at
  `wayland_protocols::wp::fractional_scale::v1` and `wayland_protocols::wp::viewporter`
  respectively. To `use` these types in `crates/core/presentation/src/wayland_surface/`,
  `mesh-core-presentation` must declare the crate as a direct dependency.

- **Features required:** `features = ["client", "staging"]`
  - `client` — generates the client-side Rust bindings from the XML
  - `staging` — gates `wp::fractional_scale` (the `preferred_scale` event lives here);
    `wp::viewporter` is a stable protocol and does not need the `staging` flag, but the
    feature is additive so enabling both is safe

- **Integration point:** `crates/core/presentation/Cargo.toml`. Add:
  ```toml
  wayland-protocols = { version = "0.32", features = ["client", "staging"] }
  ```
  Usage in `wayland_surface/mod.rs` or a new `wayland_surface/fractional_scale.rs`:
  ```rust
  use wayland_protocols::wp::fractional_scale::v1::client::{
      wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
      wp_fractional_scale_v1::{self, WpFractionalScaleV1},
  };
  use wayland_protocols::wp::viewporter::client::{
      wp_viewport::WpViewport,
      wp_viewporter::WpViewporter,
  };
  ```
  The `preferred_scale` event delivers the scale as a uint where `scale / 120` is the actual
  factor (e.g. 180 = 1.5x). `wp_viewport.set_destination(logical_w, logical_h)` sets the
  logical size so the compositor scales the buffer down to surface coordinates.

- **No version bump needed:** The transitive copy is already 0.32.12; adding it as a direct
  dep with the same version constraint pins to the same resolved copy.

### wayland-protocols-plasma v0.3 — Blur offload (KDE/KWin)

- **Why needed:** There is no standardized `wp_blur_v1` protocol in either the stable or
  staging sections of `wayland-protocols` 0.32. The two real options are:
  1. `org_kde_kwin_blur` from `wayland-protocols-plasma` — ships in KWin today and is
     the protocol that wlroots-based compositors with blur support (e.g. Hyprland) also
     recognise. Winit uses this exact crate and approach
     (`winit-wayland/src/types/kwin_blur.rs`).
  2. `ext_background_effect_v1` from `wayland-protocols 0.32` staging — the XML carries an
     explicit "Warning! currently in testing phase" notice. No major compositor had shipped
     it as of the last check; winit does not implement it.
  The correct choice is `wayland-protocols-plasma` for production blur offload, with
  `ext_background_effect_v1` deferred until a compositor ships it.

- **Version:** `wayland-protocols-plasma = { version = "0.3", features = ["client"] }`
  The latest published version is 0.3.12 (already in local registry cache). It depends on
  `wayland-client 0.31.14` and `wayland-protocols 0.32.12`, so it resolves to the same
  copies already in the lock file — zero version conflicts.

- **Integration point:** `crates/core/presentation/Cargo.toml`. Add:
  ```toml
  wayland-protocols-plasma = { version = "0.3", features = ["client"] }
  ```
  Usage pattern (lifted from winit's `kwin_blur.rs`):
  ```rust
  use wayland_protocols_plasma::blur::client::{
      org_kde_kwin_blur::OrgKdeKwinBlur,
      org_kde_kwin_blur_manager::OrgKdeKwinBlurManager,
  };
  ```
  Bind `OrgKdeKwinBlurManager` from the global registry during state init (same pattern as
  `HyprlandFocusGrabManagerV1` already in the codebase). If the global is absent the
  compositor does not support blur — degrade silently, the surface renders without blur.

---

## What NOT to Add

- **No `ext_background_effect_v1` dependency or wiring.** It is in `wayland-protocols 0.32`
  staging but carries an explicit "testing phase" warning and has no confirmed compositor
  implementation. Adding it now means dead code until a compositor ships it. Revisit at v1.21+
  when KWin or a wlroots compositor ships support.

- **No `smithay-client-toolkit` version bump.** SCT 0.19 already exposes everything needed:
  `OutputInfo::scale_factor` (i32 from `wl_output::scale`), `OutputData::scale_factor()`, and
  the `CompositorHandler::scale_factor_changed` callback for per-surface integer scale. No
  fractional scale abstraction exists in SCT 0.19, so `wp_fractional_scale_v1` must be used
  directly via `wayland-protocols` — this is correct and avoids a major toolkit version bump.

- **No new crate for per-region damage.** `wl_surface.damage_buffer(x, y, w, h)` in
  `wayland-client 0.31` can be called multiple times before `wl_surface.commit()` — the
  protocol accumulates the rects additively. The only change needed is to the
  `present_with_damage` API boundary inside `mesh-core-presentation`: change the damage
  parameter from `Option<DamageRect>` to `&[DamageRect]` and loop over the slice calling
  `damage_buffer` once per rect. No new crate, no protocol extension.

- **No `wl_surface.set_buffer_scale` for fractional scale.** The fractional scale protocol
  spec explicitly states "The wl_surface buffer scale should remain set to 1" when using
  `wp_fractional_scale_v1` + `wp_viewport`. Using `set_buffer_scale` with a fractional value
  is wrong; the viewporter destination handles the logical-to-physical mapping.

---

## Integration Notes

### HiDPI integer scale (wl_output::scale)

`smithay-client-toolkit 0.19` already handles `wl_output::scale` event dispatch internally.
The `CompositorHandler::scale_factor_changed(conn, qh, surface, factor: i32)` callback fires
when the highest-scale output the surface occupies changes. The `State` struct in
`crates/core/presentation/src/wayland_surface/state.rs` already implements `CompositorHandler`
(confirmed from the imports in `mod.rs`). Wire the callback to store the scale factor per
surface ID in `LayerShellBackend` state and forward it to the shell as a surface configuration
event so `FrontendSurfaceComponent` can rerender at the new scale.

### Fractional scale (wp_fractional_scale_v1 + wp_viewporter)

1. During Wayland global init in `backend.rs`, bind `WpFractionalScaleManagerV1` and
   `WpViewporter` the same way `LayerShell` and `HyprlandFocusGrabManagerV1` are bound today.
2. For each layer surface, call `fractional_scale_manager.get_fractional_scale(&wl_surface, qh)`
   and `viewporter.get_viewport(&wl_surface, qh)` immediately after surface creation.
3. Implement `Dispatch<WpFractionalScaleV1, ...>` to handle the `preferred_scale` event.
   Convert to f64: `scale = preferred_scale_uint as f64 / 120.0`.
4. On scale change: recompute physical buffer size as `ceil(logical_w * scale)` x
   `ceil(logical_h * scale)`, reallocate the SHM pool at the new size, call
   `wp_viewport.set_destination(logical_w as i32, logical_h as i32)` so the compositor maps
   the larger buffer back to the logical surface size.
5. Forward the scale factor to `mesh-core-shell` so the paint pipeline renders at physical
   pixels (the existing `paint_frontend_tree_at_for_module` scale parameter accepts an f64).

### Compositor blur offload (org_kde_kwin_blur)

1. Bind `OrgKdeKwinBlurManager` during global init. If absent, set a per-backend flag
   `blur_supported: false` — all `backdrop-filter: blur(...)` CSS falls back to CPU Skia
   silently.
2. When a layer surface is created and its component has `backdrop-filter: blur(...)` in style,
   call `blur_manager.create(&wl_surface, qh)` to get an `OrgKdeKwinBlur` handle.
3. Set the blur region using a `wl_region` that covers the surface bounds (or a sub-region if
   `backdrop-filter` applies to a subset). Call `blur.set_region(&region)` then
   `blur.commit()`.
4. On surface resize or region change, update the region and recommit.
5. The compositor paints the blurred background before compositing the surface's own pixels.
   MESH does not render the blur itself when the compositor handles it — skip the CPU Skia blur
   path when `blur_supported && blur_region_committed`.

### Per-region damage

The display list in `mesh-core-render` already tracks damage per node
(`last_visual_damage: HashMap<NodeId, DamageRect>`) and the paint path accumulates a
`Vec<DamageRect>`. The bottleneck is the `present_with_damage(... damage: Option<DamageRect>)`
API which merges everything into one rect before handing it to `wl_surface.damage_buffer`.

Change the pipeline:
1. `take_present_damage()` in `shell_component.rs` returns `Vec<DamageRect>` (or
   `SmallVec<[DamageRect; 4]>` for the common case of 1-4 dirty regions).
2. `PresentationEngine::present_with_damage` signature changes to `damage: &[DamageRect]`.
3. `LayerShellBackend::attach_shm_buffer` iterates `damages`, calls
   `wl_surface.damage_buffer(r.x, r.y, r.width, r.height)` once per rect.
4. The SHM copy step still needs a single union rect for the `copy_bgra_damage_to_canvas`
   call — compute the union only for the pixel copy, submit individual rects for the damage
   commit.

`wl_surface.damage_buffer` is already used (confirmed in `backend.rs` line 260); calling it
N times per commit is protocol-correct and has been supported since Wayland protocol version 4.
