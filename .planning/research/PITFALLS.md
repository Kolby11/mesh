# Pitfalls Research: MESH v1.20 Compositor Integration

**Domain:** Adding Wayland compositor protocol integrations to an existing Rust shell framework
**Researched:** 2026-06-10
**Overall confidence:** HIGH — based on direct inspection of the MESH codebase, winit-wayland reference implementation, and protocol definitions in the cargo registry

---

## Summary

All three integrations share a common failure mode: they operate in a different coordinate space or unit than what the existing code already assumes. MESH's retained renderer, `PixelBuffer`, `DamageRect`, shell layout, and layer-shell surface sizes all currently live entirely in **logical pixels** with no scale factor in sight. Adding HiDPI, blur regions, and multi-rect damage requires careful boundary work to prevent silent corruption that only manifests on HiDPI screens, specific compositors, or after the first partial-damage frame.

The most dangerous pitfall is scale factor amnesia during surface lifecycle events — configure, hide/remap, output-enter — where the existing code in `SurfaceEntry` and `CompositorHandler::scale_factor_changed` silently ignores the event. The current `scale_factor_changed` handler in `handlers.rs` is an empty no-op; every downstream assumption will be wrong the moment a scale event arrives and nothing stores it.

---

## HiDPI / Fractional Scale Pitfalls

| Pitfall | Warning Sign | Prevention | Phase |
|---------|-------------|------------|-------|
| **Buffer allocated at logical size, rendered at logical size, but viewporter destination set to logical size** — net result is compositor upscaling the buffer (blurry) | UI looks blurry on HiDPI; pixel-perfect text disappears | `PixelBuffer::new(width * scale, height * scale)` for the buffer; `wp_viewport.set_destination(logical_width, logical_height)` for the compositor to downscale correctly | HiDPI implementation phase |
| **Forgetting `wl_surface::set_buffer_scale` is wrong when viewporter is used** — calling both `set_buffer_scale` and `wp_viewport.set_destination` together is a protocol error that causes undefined compositor behavior | Compositor crash or garbled surface on some compositors; works on others | When `wp_fractional_scale_v1` is active, use only the viewporter path. Only call `set_buffer_scale` when the compositor does not advertise the fractional scale global (integer-scale fallback path). The winit-wayland reference code at `window/state.rs:1056` gates this explicitly | HiDPI implementation phase |
| **scale_factor_changed in CompositorHandler is an empty no-op** — the current handler stores nothing; surfaces never learn the scale | No crash; UI silently renders at 1x even on a 2x display | Implement the handler to store the scale per surface in `SurfaceEntry` and trigger a repaint. Also wire `preferred_scale` from `wp_fractional_scale_v1::Event::PreferredScale` (denominator is 120, as in winit's `SCALE_DENOMINATOR`) into the same per-surface field | HiDPI implementation phase |
| **Scale event arrives before the first configure** — the fractional scale preferred event is specified to arrive before the first configure; if the code processes configure before applying the stored scale, the first frame is rendered at 1x then immediately redrawn | One-frame 1x flash or wrongly-sized initial SHM buffer allocation | Store the pending scale in `SurfaceEntry` at global-bind time; apply it when `copy_into_shm_buffer` actually runs, not on configure | HiDPI implementation phase |
| **DamageRect coordinates are in logical pixels but damage_buffer expects buffer pixels** — the current `attach_shm_buffer` calls `wl_surface.damage_buffer(damage.x, damage.y, damage.width, damage.height)` treating `DamageRect` as buffer-pixel coordinates. At scale 1.0 these are the same. At scale 1.5 or 2.0 the damage region undershoots by `scale` factor and the compositor leaves stale pixels outside the reported region | Stale pixel artifacts at surface edges after partial updates on HiDPI | Scale the `DamageRect` by the buffer scale factor before passing it to `damage_buffer`. Use `damage_buffer` (buffer pixels), not `damage` (logical pixels) — the current code already uses `damage_buffer` which is correct, but the coordinates fed into it must be in buffer pixels | HiDPI implementation phase |
| **PixelBuffer stride mismatch after scale factor change** — if the buffer was previously created at logical size and the scale factor changes, `copy_into_shm_buffer` compares `ShmPoolConfig {width, height, stride}` to the stored config; a scale change with unchanged logical size will reuse the wrong-size pool | Garbled rendering after monitor hotplug or scale change | When scale changes, invalidate the `shm_pool_config` by resizing or reconstructing `SurfaceEntry::shm_buffers`. Treat scale factor change the same as size change for buffer pool purposes | HiDPI implementation phase |
| **Fractional rounding produces a buffer 1 pixel too small** — computing `(logical_size * scale_numerator / 120).round()` vs `(logical_size * scale_numerator / 120).ceil()` at 150% on a 1px-wide surface can produce a buffer that is 1 pixel short of covering the viewporter destination | Single-pixel gap at surface edge with translucent compositor background bleeding through | Use `ceil` when computing buffer dimensions from fractional scale, not `round`. A 1px over-allocation is always safe; a 1px under-allocation leaves uninitialized compositor memory visible | HiDPI implementation phase |
| **wp_fractional_scale_v1 and wp_viewporter are separate global binds** — both must be negotiated independently. Binding one but not the other leaves the surface broken without any error | Surface renders at 1x even after scale event, no protocol error | Bind both `WpFractionalScaleManagerV1` and `WpViewporter` as optional globals in `LayerShellBackend::new()`, paired; if either is unavailable fall back to integer `wl_output::scale` | HiDPI implementation phase |
| **surface_enter/surface_leave handlers are empty** — when a surface moves to an output with a different scale, the `surface_enter` handler should update per-surface scale | Wrong scale after moving between monitors | Implement `surface_enter` and `surface_leave` to update `SurfaceEntry::scale` and trigger a repaint | HiDPI implementation phase |

---

## Compositor Blur Offload Pitfalls

| Pitfall | Warning Sign | Prevention | Phase |
|---------|-------------|------------|-------|
| **Blur region coordinates are in logical (surface) pixels, not buffer pixels** — `org_kde_kwin_blur` takes a `wl_region` and the region is in surface-local coordinates. At scale 2x, passing buffer-pixel coordinates makes the blur region cover only one quarter of the intended area | Blur appears in wrong position or covers wrong region on HiDPI | Always compute blur regions in logical pixels. If the blur region is derived from `DamageRect` or `PixelBuffer` coordinates that have been scaled up, divide back to logical before creating the `wl_region` | Blur implementation phase |
| **Blur object must be committed after region changes** — `org_kde_kwin_blur::commit()` is a separate call. Without it, blur region changes are silently ignored by the compositor. The winit reference code calls `blur.commit()` immediately after creating the blur object but does not re-commit on region update | Blur region stuck at initial value despite `set_region` calls | Call `blur.commit()` after every `blur.set_region(...)` and after `blur.set_region(None)` (full-surface blur). The commit is separate from `wl_surface::commit` | Blur implementation phase |
| **wp_blur_v1 does not exist in the wild** — as of 2025-2026, `wp_blur_v1` is a proposed protocol that no production compositor ships. KDE Plasma ships `org_kde_kwin_blur` only; GNOME has no blur protocol at all | Implementation targets `wp_blur_v1` exclusively and never activates on any real system | Treat `org_kde_kwin_blur` as the primary path and `wp_blur_v1` as an optional future extension. Bind both at startup as optional globals. If neither is available, fall back to CPU blur in the MESH renderer | Blur implementation phase |
| **wayland-protocols-plasma is not yet a direct MESH dependency** — `org_kde_kwin_blur` lives in `wayland-protocols-plasma`, which is present in the cargo registry (0.3.12) but is not in any MESH Cargo.toml. Adding it is required before writing the binding | Build error with no other symptom | Add `wayland-protocols-plasma = { version = "0.3", features = ["client"] }` to `mesh-core-presentation/Cargo.toml` | Blur implementation phase |
| **Blur region must be re-sent on every surface remap** — after `entry.hide()` detaches the buffer and later re-attaches it, the compositor treats it as a new surface configuration. Blur protocol state is not preserved across null-buffer commits on some compositors | Blur disappears after surface toggle (e.g. launcher open/close) | Store blur config per `SurfaceEntry`; re-apply the blur object and commit it as part of the present path after any remap, alongside the existing opaque region update in `update_opaque_region` | Blur implementation phase |
| **Calling unset() without checking if blur was ever set** — `OrgKdeKwinBlurManager::unset(surface)` on a surface that never had blur set is a no-op on some compositors and a protocol error on others | Intermittent protocol error log; works on one compositor, fails on another | Only call `unset` if the surface currently has an active `OrgKdeKwinBlur` object; track this per `SurfaceEntry` | Blur implementation phase |
| **CPU blur still runs when compositor blur is active** — if the MESH renderer's existing `backdrop-filter: blur(...)` CPU Skia path remains active while the compositor blur protocol is also wired, the client blurs its own pixels before sending them, and the compositor then blurs those pre-blurred pixels | Double-blurred, unusably soft appearance on KDE | Add a `compositor_blur_active` flag per surface; when it is set, the render path for `backdrop-filter: blur()` should skip the CPU Skia blur and render the region as transparent instead | Blur implementation phase |

---

## Per-Region Damage Pitfalls

| Pitfall | Warning Sign | Prevention | Phase |
|---------|-------------|------------|-------|
| **damage_buffer vs damage coordinate system confusion** — `wl_surface::damage` takes logical pixels; `wl_surface::damage_buffer` takes buffer pixels. MESH already correctly calls `damage_buffer` in `attach_shm_buffer`. If multi-rect damage is added and any rect is produced by a code path that uses logical coordinates, it will be wrong by the scale factor | Partial repaints miss damaged areas or mark too much on HiDPI | All `damage_buffer` calls must use buffer-space coordinates. Audit every site that constructs a `DamageRect` to confirm it is in buffer pixels before it reaches `attach_shm_buffer`. Add a type wrapper (`BufferDamageRect` vs `LogicalDamageRect`) to make the distinction compile-time-checked | Multi-rect damage phase |
| **union_damage flattening defeats multi-rect** — the current `copy_into_shm_buffer` path calls `union_damage` on pending slots, collapsing multiple pending rects to one bounding box per buffer. Moving to multi-rect damage requires tracking a `Vec<DamageRect>` per slot, not a single `Option<DamageRect>` | Multi-rect damage API exists but intermediate pending damage is still a single bounding box, recovering only partial benefit | Replace `SurfaceShmBuffer::pending_damage: Option<DamageRect>` with `Vec<DamageRect>`. Union logic for pending slots must union the rect lists. The `copy_bgra_damage_to_canvas` copy path must iterate over the list of rects | Multi-rect damage phase |
| **Sending too many damage rects degrades compositor performance** — `wl_surface::damage_buffer` is a stateful call and calling it N times per commit with N very small rects adds compositor overhead. On Weston and KWin, more than ~32 rects per commit starts to cost more than a single bounding rect | Performance regression on compositors with many small dirty regions | Cap multi-rect damage at a maximum count (e.g. 16–32 rects). If the retained display list produces more dirty rects than the cap, fall back to a single bounding-box damage call. Measure before and after | Multi-rect damage phase |
| **Empty damage commit skips frame but compositor still holds the buffer** — if no damage is reported (zero damage rects), the existing code still calls `wl_surface.frame()` and `commit()`. Without a `damage_buffer` call, the compositor may not actually scan out the new buffer. Some compositors silently drop the commit; others present stale content | No visual update even though the buffer was committed | Always call `damage_buffer` at least once per commit with a non-empty rect. If the damage list is empty, skip the commit entirely rather than committing without damage | Multi-rect damage phase |
| **Pending damage accumulation order matters for SHM buffer rotation** — the current 2-deep buffer pool (`SHM_BUFFER_POOL_DEPTH = 2`) unions damage across frames into `pending_damage` per slot to account for the fact that the previous-slot's buffer may be a frame behind. With `Vec<DamageRect>` pending damage this still works, but the union must be per-rect-list, not per-single-rect | After switching to Vec damage, some rects in the second slot are not fully refreshed, causing single-pixel flickering on partial-damage frames | When converting `Option<DamageRect>` accumulation to `Vec<DamageRect>`, keep the union-across-slots logic: for each slot, append the new frame's rects to its existing pending list rather than replacing it. Deduplicate or merge overlapping rects before copying | Multi-rect damage phase |
| **Damage tracking at display-list level is per-entry, but painting at surface level needs buffer-space coords** — the `DisplayList::compute_damage` result in `display_list.rs` returns a single `DamageRect` in layout space; the entries individually store `bounds: DamageRect` also in layout space. If scale factor is >1, these must be converted to buffer pixels before being reported as damage | Damage region is correct at 1x, wrong at 2x — undercovers on HiDPI, leaving stale pixels | When extracting multi-rect damage from `DisplayList` entries, apply the surface scale factor before creating the `Vec<DamageRect>` passed to `attach_shm_buffer`. This conversion must happen at the shell/presentation boundary, not inside the display list | Multi-rect damage phase |

---

## Integration Pitfalls (Cross-Cutting)

### Single source of truth for scale factor

The existing `State` struct holds no per-surface scale. `CompositorHandler::scale_factor_changed` is empty, `OutputHandler::update_output` is empty, `surface_enter` is empty. All three integrations need scale to be authoritative. Whoever adds scale first (HiDPI phase) must put it in `SurfaceEntry` — not in a separate map, not as a global field on `State` (multiple outputs can have different scales). All later work (damage rects, blur regions) must read from `SurfaceEntry::scale_factor: f64`.

**Warning sign:** Any field called `global_scale` or `state.scale` that is not per-surface.

**Phase:** Must be established in the HiDPI phase; blur and multi-rect damage phases must not re-derive scale independently.

### Protocol availability and graceful degradation

None of the three protocols is universally available. `wp_fractional_scale_v1` requires the explicit Wayland global; `org_kde_kwin_blur` is KDE-only; `wp_viewporter` is near-universal but still optional. The existing pattern in `LayerShellBackend::new()` for optional globals (`ActivationState::bind(&globals, &qh).ok()`) is correct — use `Option<Manager>` for all three new globals and silently skip protocol work when they are absent.

**Warning sign:** Unwrapping a newly bound global instead of using `.ok()` or checking `Option`.

**Phase:** All phases.

### wl_surface::commit applies all pending state atomically

`wl_surface::commit` applies pending state for ALL double-buffered properties in one atomic step: buffer, damage, input region, opaque region, blur region, viewport destination. The existing `attach_shm_buffer` commits in order: `damage_buffer`, `buffer.attach_to`, `frame`, `commit`. Adding blur or viewporter calls must happen before the `commit()` call on the same `wl_surface`, not after. Setting the viewporter destination or blur region after `commit()` queues it for the next frame, causing a one-frame lag.

**Warning sign:** `viewport.set_destination(...)` or `blur.commit()` placed after `layer_surface.commit()` in `attach_shm_buffer`.

**Phase:** All phases; reviewed during each new protocol integration.

### SHM buffer pool size vs HiDPI memory

At scale 2x, a 1920x32 panel surface becomes a 3840x64 buffer: 983 040 bytes instead of 245 760. The initial `SlotPool::new(256 * 256 * 4, &shm)` pool (262 144 bytes) is smaller than one 2x-scaled frame for a full-width panel. The pool grows on demand but fails silently if the initial size is below one buffer.

**Warning sign:** `BufferAlloc` error on first present at 2x scale.

**Phase:** HiDPI implementation phase — increase or make the initial pool size scale-aware, or rely on `SlotPool`'s on-demand growth and verify no allocation error path returns early silently.

### configure → scale ordering on surface creation

The layer-shell configure event arrives asynchronously. The current `wait_for_surface_configure` spins up to 10 roundtrips. With fractional scale, the `preferred_scale` event from `wp_fractional_scale_v1` should arrive before the configure (per protocol spec), but this is compositor-implementation-defined. Treat scale as mutable after configure: always read `SurfaceEntry::scale_factor` at render time, not at configure time.

**Warning sign:** Scale factor is captured once in the configure handler and never updated.

**Phase:** HiDPI implementation phase.

### hide() clears configured state — all protocol objects must survive it

`SurfaceEntry::hide()` calls `wl_surface.attach(None, 0, 0)` and sets `configured = false`. The blur `OrgKdeKwinBlur` object and the viewporter `WpViewport` object are tied to the `wl_surface` lifetime, not the buffer lifetime. They survive `hide()` and must not be destroyed on hide. They should only be destroyed when the `SurfaceEntry` itself is dropped.

**Warning sign:** `blur.release()` or `viewport.destroy()` called inside `hide()`.

**Phase:** Both blur and HiDPI phases.

### No wp_blur_v1 crate exists yet

The v1.20 milestone spec mentions `wp_blur_v1` alongside `org_kde_kwin_blur_v1`. As of the available registry contents, `wp_blur_v1` is not in `wayland-protocols` 0.32.12 or any available crate. `wayland-protocols-plasma` 0.3.12 provides `org_kde_kwin_blur`. Do not block the blur feature on `wp_blur_v1` availability — ship the KDE path, add a feature flag for `wp_blur_v1` when it lands.

**Warning sign:** Implementation phase blocked waiting for `wp_blur_v1` crate.

**Phase:** Blur implementation phase — acknowledge this scope difference at phase start.
