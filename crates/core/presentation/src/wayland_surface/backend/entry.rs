use super::*;
use mesh_core_render::DamageRect;
use smallvec::{SmallVec, smallvec};

const MAX_FRAME_CALLBACK_WAIT: Duration = Duration::from_millis(50);

/// The compositor-side object backing a [`SurfaceEntry`]. Layer surfaces own
/// shell chrome (panels, launchers, overlays); windows are `xdg_toplevel`s the
/// compositor manages like any application window; popups are `xdg_popup`
/// children promoted from a `<popover>`. All three expose a `wl_surface`, so the
/// entire SHM buffer / present / scale / input path below is shared — only
/// creation, configuration, and teardown differ.
pub(in crate::wayland_surface) enum WaylandRole {
    Layer(LayerSurface),
    Window(WindowRole),
    Popup(PopupRole),
}

pub(in crate::wayland_surface) struct WindowRole {
    pub(in crate::wayland_surface) window: Window,
    /// Size the compositor last configured, when it named one. `None` until the
    /// first sized configure arrives — a toplevel's initial configure usually
    /// carries no size, meaning "pick your own", which is when the CSS-measured
    /// content size is authoritative. After that the compositor's size wins;
    /// this is the inverse of the layer-shell direction.
    pub(in crate::wayland_surface) compositor_size: Option<(u32, u32)>,
    /// Last size hints handed to `xdg_toplevel`, so identical hints aren't
    /// re-sent on every frame.
    pub(in crate::wayland_surface) applied_size_hints: Option<WindowSizeHints>,
    /// Last window geometry committed via `xdg_surface.set_window_geometry`.
    pub(in crate::wayland_surface) applied_geometry: Option<DamageRect>,
    /// `xdg_toplevel` states from the last configure, projected by the shell
    /// onto the surface tree as CSS state.
    pub(in crate::wayland_surface) states: WindowStates,
}

/// The two kinds of surface a popup can be parented to, kept apart because
/// each is attached by a different protocol call.
pub(super) enum PopupParent {
    Layer(LayerSurface),
    Window(xdg_surface::XdgSurface),
}

/// The `xdg_toplevel` min/max size pair. `None` on either side means "no
/// constraint" — the compositor is free to pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wayland_surface) struct WindowSizeHints {
    min: Option<(u32, u32)>,
    max: Option<(u32, u32)>,
}

pub(in crate::wayland_surface) struct PopupRole {
    pub(in crate::wayland_surface) popup: Popup,
    /// `surface_id` of the parent surface this popup is a child of. The parent
    /// may be either a layer surface or a window.
    pub(in crate::wayland_surface) parent_id: String,
}

impl WaylandRole {
    pub(in crate::wayland_surface) fn wl_surface(&self) -> &wl_surface::WlSurface {
        match self {
            WaylandRole::Layer(layer) => layer.wl_surface(),
            WaylandRole::Window(role) => role.window.wl_surface(),
            WaylandRole::Popup(role) => role.popup.wl_surface(),
        }
    }

    pub(in crate::wayland_surface) fn as_layer(&self) -> Option<&LayerSurface> {
        match self {
            WaylandRole::Layer(layer) => Some(layer),
            _ => None,
        }
    }

    pub(in crate::wayland_surface) fn as_window(&self) -> Option<&Window> {
        match self {
            WaylandRole::Window(role) => Some(&role.window),
            _ => None,
        }
    }

    /// The `xdg_surface` a child popup can be parented to. Layer surfaces have
    /// none (they use `zwlr_layer_surface_v1.get_popup` instead); a window
    /// parents its popups the ordinary xdg-shell way.
    pub(in crate::wayland_surface) fn as_popup_parent_xdg_surface(
        &self,
    ) -> Option<&xdg_surface::XdgSurface> {
        match self {
            WaylandRole::Window(role) => Some(role.window.xdg_surface()),
            _ => None,
        }
    }

    pub(super) fn is_popup(&self) -> bool {
        matches!(self, WaylandRole::Popup(_))
    }

    pub(super) fn is_window(&self) -> bool {
        matches!(self, WaylandRole::Window(_))
    }
}

pub(in crate::wayland_surface) struct SurfaceEntry {
    pub(in crate::wayland_surface) role: WaylandRole,
    pub(in crate::wayland_surface) cfg: SurfaceConfig,
    pub(in crate::wayland_surface) applied_keyboard_mode: KeyboardMode,
    pub(in crate::wayland_surface) width: u32,
    pub(in crate::wayland_surface) height: u32,
    pub(in crate::wayland_surface) configured: bool,
    /// Set until title/app_id have been pushed to the toplevel once. The first
    /// `apply_config` after creation sees a config equal to the one the entry
    /// was constructed with, so identity would otherwise never be sent.
    applied_window_identity_pending: bool,
    shm_buffers: Vec<SurfaceShmBuffer>,
    shm_buffer_bytes: usize,
    shm_pool_config: Option<ShmPoolConfig>,
    next_shm_buffer: usize,
    pub(in crate::wayland_surface) frame_pending: bool,
    pub(in crate::wayland_surface) frame_pending_since: Option<Instant>,
    pub(in crate::wayland_surface) scale: f32,
    pub(in crate::wayland_surface) needs_full_redraw: bool,
    pub(in crate::wayland_surface) fractional_scale: Option<WpFractionalScaleV1>,
    pub(in crate::wayland_surface) viewport: Option<WpViewport>,
    pub(in crate::wayland_surface) kde_blur: Option<OrgKdeKwinBlur>,
    pub(in crate::wayland_surface) blur_regions: Vec<DamageRect>,
    pub(in crate::wayland_surface) blur_committed: bool,
    pub(in crate::wayland_surface) blur_region_dirty: bool,
    /// Desired opaque region staged by the shell. It becomes committed only
    /// after the next successful wl_surface commit.
    pub(in crate::wayland_surface) pending_opaque_region: Option<DamageRect>,
    pub(in crate::wayland_surface) opaque_region_dirty: bool,
    /// Which part of this surface is paint-only reserve. Copied from the
    /// config/popup-config that sized the surface, and the *only* input the
    /// input region is derived from — see [`SurfacePadding`].
    pub(in crate::wayland_surface) padding: SurfacePadding,
    /// The input region last committed to the compositor, in surface-local
    /// logical coordinates. `Some(None)` means "whole-surface input has been
    /// committed"; the outer `None` means nothing has been committed yet, which
    /// is what forces a freshly created (or recreated) surface to publish its
    /// region again instead of inheriting a stale "already applied" belief.
    pub(in crate::wayland_surface) applied_input_region: Option<Option<DamageRect>>,
    /// The output this surface's `wl_surface` currently overlaps, tracked via
    /// `wl_surface::enter`/`leave`. A surface can technically straddle more
    /// than one output; the most recent `enter` wins, which is exactly what a
    /// single-output-bound layer surface (the common case: `output: None` at
    /// creation, compositor picks one) needs. `None` before the first `enter`
    /// arrives (e.g. between surface creation and the first frame).
    pub(in crate::wayland_surface) output: Option<wl_output::WlOutput>,
}

impl SurfaceEntry {
    pub(super) fn new(
        role: WaylandRole,
        cfg: SurfaceConfig,
        applied_keyboard_mode: KeyboardMode,
    ) -> Self {
        Self {
            role,
            width: cfg.width.max(1),
            height: cfg.height.max(1),
            padding: cfg.padding,
            cfg,
            applied_keyboard_mode,
            configured: false,
            applied_window_identity_pending: true,
            shm_buffers: Vec::new(),
            shm_buffer_bytes: 0,
            shm_pool_config: None,
            next_shm_buffer: 0,
            frame_pending: false,
            frame_pending_since: None,
            scale: 1.0,
            needs_full_redraw: false,
            fractional_scale: None,
            viewport: None,
            kde_blur: None,
            blur_regions: Vec::new(),
            blur_committed: false,
            blur_region_dirty: false,
            pending_opaque_region: None,
            opaque_region_dirty: false,
            applied_input_region: None,
            output: None,
        }
    }

    pub(in crate::wayland_surface) fn wl_surface(&self) -> &wl_surface::WlSurface {
        self.role.wl_surface()
    }

    pub(in crate::wayland_surface) fn destroy_auxiliary_protocol_objects(&self) {
        if let Some(viewport) = self.viewport.as_ref() {
            viewport.destroy();
        }
        if let Some(fractional_scale) = self.fractional_scale.as_ref() {
            fractional_scale.destroy();
        }
        if let Some(blur) = self.kde_blur.as_ref() {
            blur.release();
        }
    }

    /// The input region this surface should currently have: its
    /// compositor-configured size minus the reserve it declared.
    ///
    /// Derived on every commit rather than pushed by the shell. A derived value
    /// cannot be missed by a caller, cannot be dropped because the surface did
    /// not exist yet, and is automatically re-established when the compositor
    /// object is torn down and recreated (role swap, window hide/show) — the
    /// three ways this has regressed before.
    pub(in crate::wayland_surface) fn content_input_region(&self) -> Option<DamageRect> {
        self.padding
            .content_rect(self.width.max(1), self.height.max(1))
    }

    pub(super) fn needs_reconfigure(
        &self,
        cfg: &SurfaceConfig,
        keyboard_mode: KeyboardMode,
    ) -> bool {
        !self.configured || self.config_change(cfg, keyboard_mode) != SurfaceConfigChange::Unchanged
    }

    pub(super) fn config_change(
        &self,
        cfg: &SurfaceConfig,
        keyboard_mode: KeyboardMode,
    ) -> SurfaceConfigChange {
        surface_config_change(&self.cfg, self.applied_keyboard_mode, cfg, keyboard_mode)
    }

    pub(super) fn apply_config(&mut self, cfg: SurfaceConfig, keyboard_mode: KeyboardMode) {
        // Adopt the reserve before anything can return early: the padding is
        // client-side state with no protocol request of its own, and a config
        // that changed only the reserve must still reach `content_input_region`.
        self.padding = cfg.padding;
        // A toplevel takes title/app_id/size-hint requests instead of the
        // layer-shell placement requests, and never invalidates `configured`
        // for them: the compositor is not obliged to answer a title change with
        // a fresh configure, and blocking presents until it does would freeze
        // the window.
        if self.role.is_window() {
            self.apply_window_config(cfg, keyboard_mode);
            return;
        }
        // Layer-shell config (anchor/layer/size/margins) only applies to layer
        // surfaces. Popups are positioned by their `xdg_positioner`, never by
        // these requests, so there is nothing to apply for the popup role.
        let Some(layer_surface) = self.role.as_layer() else {
            return;
        };
        let requires_fresh_configure =
            surface_config_change(&self.cfg, self.applied_keyboard_mode, &cfg, keyboard_mode)
                .requires_fresh_configure();
        let effective_cfg = cfg.with_keyboard_mode(keyboard_mode);
        apply_config(layer_surface, &effective_cfg);
        layer_surface.commit();
        self.cfg = cfg;
        self.applied_keyboard_mode = keyboard_mode;
        // Geometry/layout reconfiguration can require a fresh layer-shell
        // configure before another buffer attach. Keyboard interactivity-only
        // changes do not, and some compositors never answer them with a new
        // configure event. Keeping `configured` true in that case avoids
        // dropping every subsequent present after a mouse-triggered focus
        // transition on an already visible surface.
        if requires_fresh_configure {
            self.configured = false;
        }
    }

    /// Apply the toplevel-facing half of a [`SurfaceConfig`].
    ///
    /// `cfg.width`/`cfg.height` are the CSS-measured *content* size. For a
    /// window they are a request, not a decision: they seed the initial size,
    /// and nothing more — unless the surface is declared non-resizable, in
    /// which case reporting them as both min and max pins the window to its
    /// content.
    ///
    /// A resizable window deliberately sends *no* min size. The measured size
    /// is a component's natural size, not its minimum; publishing it as
    /// `set_min_size` would let the user grow a window but never shrink it, and
    /// would fight a tiling compositor over every layout.
    fn apply_window_config(&mut self, cfg: SurfaceConfig, keyboard_mode: KeyboardMode) {
        let Some(window) = self.role.as_window() else {
            return;
        };
        let window = window.clone();

        if cfg.window.title != self.cfg.window.title || self.applied_window_identity_pending {
            window.set_title(cfg.window.title.clone());
        }
        if cfg.window.app_id != self.cfg.window.app_id || self.applied_window_identity_pending {
            window.set_app_id(cfg.window.app_id.clone());
        }
        self.applied_window_identity_pending = false;

        let content = (cfg.width.max(1), cfg.height.max(1));
        let pinned = (!cfg.window.resizable).then_some(content);
        let hints = WindowSizeHints {
            min: pinned,
            max: pinned,
        };
        if let WaylandRole::Window(role) = &mut self.role
            && role.applied_size_hints != Some(hints)
        {
            window.set_min_size(hints.min);
            window.set_max_size(hints.max);
            role.applied_size_hints = Some(hints);
        }

        self.cfg = cfg;
        self.applied_keyboard_mode = keyboard_mode;
        window.commit();
        tracing::debug!(
            configured = self.configured,
            width = self.cfg.width,
            height = self.cfg.height,
            "layer_shell: window config committed"
        );
    }

    /// Tell the compositor which part of the buffer is the window proper.
    ///
    /// Without this the compositor measures a toplevel by its whole buffer,
    /// which for MESH includes the transparent tooltip overlay reserve — the
    /// window would be placed, snapped, and decorated as if it were larger than
    /// it looks. The content rect is the same one the input region is confined
    /// to, so a click landing outside the window and a compositor drawing
    /// outside the window are ruled out by the same number.
    pub(super) fn stage_window_geometry(&self, content: DamageRect) -> Option<DamageRect> {
        let WaylandRole::Window(role) = &self.role else {
            return None;
        };
        let geometry = DamageRect {
            x: content.x,
            y: content.y,
            width: content.width.max(1),
            height: content.height.max(1),
        };
        if role.applied_geometry == Some(geometry) {
            return None;
        }
        role.window.xdg_surface().set_window_geometry(
            content.x as i32,
            content.y as i32,
            geometry.width as i32,
            geometry.height as i32,
        );
        Some(geometry)
    }

    pub(super) fn mark_window_geometry_committed(&mut self, geometry: DamageRect) {
        if let WaylandRole::Window(role) = &mut self.role {
            role.applied_geometry = Some(geometry);
        }
    }

    pub(super) fn input_region_needs_commit(&self) -> bool {
        self.applied_input_region != Some(self.content_input_region())
    }

    pub(super) fn stage_input_region(&self, compositor_state: &CompositorState) -> bool {
        let Some(rect) = self.content_input_region() else {
            self.wl_surface().set_input_region(None);
            return true;
        };
        let Ok(region) = Region::new(compositor_state) else {
            return false;
        };
        region.add(
            rect.x as i32,
            rect.y as i32,
            rect.width as i32,
            rect.height as i32,
        );
        self.wl_surface().set_input_region(Some(region.wl_region()));
        true
    }

    pub(super) fn mark_input_region_committed(&mut self, region: Option<DamageRect>) {
        self.applied_input_region = Some(region);
    }

    pub(super) fn stage_opaque_region(&self, compositor_state: &CompositorState) -> bool {
        if !self.opaque_region_dirty {
            return false;
        }
        let Some(rect) = self.pending_opaque_region else {
            self.wl_surface().set_opaque_region(None);
            return true;
        };
        let Ok(region) = Region::new(compositor_state) else {
            return false;
        };
        region.add(
            rect.x as i32,
            rect.y as i32,
            rect.width as i32,
            rect.height as i32,
        );
        self.wl_surface()
            .set_opaque_region(Some(region.wl_region()));
        true
    }

    pub(super) fn mark_opaque_region_committed(&mut self) {
        self.opaque_region_dirty = false;
    }

    pub(super) fn hide(&mut self) {
        self.frame_pending = false;
        self.frame_pending_since = None;
        let wl_surface = self.role.wl_surface();
        wl_surface.attach(None, 0, 0);
        wl_surface.commit();
        // Wait for a fresh configure before attaching a buffer again after remap.
        self.configured = false;
    }

    pub(super) fn copy_into_shm_buffer(
        &mut self,
        pool: &mut SlotPool,
        src: &[u8],
        width: u32,
        height: u32,
        damage: &[DamageRect],
    ) -> Result<Option<(usize, SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]>)>, PresentationError>
    {
        let width = width.max(1);
        let height = height.max(1);
        let full = full_damage(width, height);
        let pool_config = try_shm_pool_config_for(width, height, self.viewport.is_some())?;
        if self.shm_pool_config != Some(pool_config) {
            self.shm_buffers.clear();
            self.shm_buffer_bytes = 0;
            self.next_shm_buffer = 0;
            self.shm_pool_config = Some(pool_config);
        }

        while self.shm_buffers.len() < SHM_BUFFER_POOL_DEPTH {
            if self.shm_buffer_bytes > MAX_SHM_SURFACE_BYTES.saturating_sub(pool_config.bytes)
                || !shm_pool_growth_allowed(pool.len(), pool_config.bytes)
            {
                return Err(PresentationError::BufferAlloc(format!(
                    "SHM pool budget exceeded while allocating {}x{} ({} bytes)",
                    pool_config.width, pool_config.height, pool_config.bytes
                )));
            }
            self.shm_buffers
                .push(create_surface_shm_buffer(pool, pool_config, full)?);
            self.shm_buffer_bytes += pool_config.bytes;
        }

        for slot in &mut self.shm_buffers {
            extend_pending_damage(&mut slot.pending_damage, damage, full);
        }

        let len = self.shm_buffers.len();
        for offset in 0..len {
            let index = (self.next_shm_buffer + offset) % len;
            if let Some(canvas) = pool.canvas(&self.shm_buffers[index].buffer) {
                let copy_damage = std::mem::take(&mut self.shm_buffers[index].pending_damage);
                for rect in &copy_damage {
                    if let Err(error) = copy_bgra_damage_to_canvas(
                        src,
                        canvas,
                        width,
                        height,
                        pool_config.width,
                        *rect,
                    ) {
                        restore_pending_damage(
                            &mut self.shm_buffers[index].pending_damage,
                            &copy_damage,
                            full,
                        );
                        return Err(PresentationError::BufferCopy(error.to_string()));
                    }
                }
                self.next_shm_buffer = (index + 1) % self.shm_buffers.len();
                // When a buffer is reused while older frame callbacks are still
                // outstanding, `pending_damage` can be larger than the current
                // frame's damage. We must report the region we actually copied
                // into this buffer, otherwise the compositor may keep showing
                // stale pixels outside `frame_damage`.
                return Ok(Some((index, copy_damage)));
            }
        }

        if self.shm_buffers.len() >= SHM_BUFFER_POOL_MAX {
            // The compositor still owns every slot. No allocation failed: this
            // is ordinary Wayland backpressure (notably for occluded/offscreen
            // surfaces whose frame callbacks and releases may be throttled).
            // Let the present boundary return NotReady so the shell retains
            // this frame's damage and retries after dispatching more events.
            return Ok(None);
        }

        if self.shm_buffer_bytes > MAX_SHM_SURFACE_BYTES.saturating_sub(pool_config.bytes)
            || !shm_pool_growth_allowed(pool.len(), pool_config.bytes)
        {
            return Err(PresentationError::BufferAlloc(format!(
                "SHM pool budget exceeded while allocating {}x{} ({} bytes)",
                pool_config.width, pool_config.height, pool_config.bytes
            )));
        }

        let index = self.shm_buffers.len();
        let (wl_buffer, canvas) = pool
            .create_buffer(
                pool_config.width as i32,
                pool_config.height as i32,
                pool_config.stride,
                wl_shm::Format::Argb8888,
            )
            .map_err(|e| PresentationError::BufferAlloc(format!("create_buffer: {e}")))?;
        copy_bgra_to_canvas(src, canvas, width, height, pool_config.width)
            .map_err(|error| PresentationError::BufferCopy(error.to_string()))?;
        self.shm_buffers.push(SurfaceShmBuffer {
            buffer: wl_buffer,
            pending_damage: SmallVec::new(),
        });
        self.shm_buffer_bytes += pool_config.bytes;
        self.next_shm_buffer = (index + 1) % self.shm_buffers.len();
        Ok(Some((index, smallvec![full])))
    }

    pub(super) fn attach_shm_buffer(
        &mut self,
        qh: &QueueHandle<State>,
        index: usize,
        logical_width: u32,
        logical_height: u32,
        physical_width: u32,
        physical_height: u32,
        damage_rects: &[DamageRect],
        copy_damage: &[DamageRect],
        scale: f32,
    ) -> Result<(), PresentationError> {
        let buffer = &self.shm_buffers[index].buffer;
        let wl_surface = self.role.wl_surface();

        // Activate the slot before staging any other surface state. If the
        // activation fails, the caller restores the copied damage and no
        // partially prepared commit is acknowledged as presented.
        buffer
            .attach_to(wl_surface)
            .map_err(|error| PresentationError::BufferAttach(error.to_string()))?;

        // Scale and clip in one pass. Keep the common <=16 rect path inline so
        // ordinary presents avoid heap allocation entirely.
        let mut clipped_damage: SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]> = damage_rects
            .iter()
            .map(|r| scale_damage_rect_to_physical(*r, scale))
            .map(|r| clip_damage_rect_to_buffer(r, physical_width, physical_height))
            .collect();

        // The region actually copied into THIS shm buffer (`copy_damage`, already
        // in physical coordinates) can be larger than the current frame's damage:
        // a buffer that was busy across earlier frames accumulates their damage in
        // `pending_damage` and refreshes all of it on reuse. The compositor only
        // re-composites the regions we report here, so we must include the full
        // copied region — otherwise the stale pixels outside `damage_rects` keep
        // showing the previous buffer's content (the transparent rectangular cutout).
        clipped_damage.extend(
            copy_damage
                .iter()
                .map(|rect| clip_damage_rect_to_buffer(*rect, physical_width, physical_height)),
        );

        let protocol_damage =
            protocol_damage_rects(&clipped_damage, physical_width, physical_height);
        for rect in protocol_damage.iter().copied() {
            wl_surface.damage_buffer(
                rect.x as i32,
                rect.y as i32,
                rect.width as i32,
                rect.height as i32,
            );
        }

        let scale_is_integer = (scale - scale.round()).abs() < f32::EPSILON;
        let buffer_scale = if scale_is_integer {
            // Integer scale uses the viewport crop below when it is available.
            scale as i32
        } else if self.viewport.is_some() {
            // Fractional scale WITH viewporter: set_buffer_scale to ceil(scale),
            // then set_destination to logical dimensions so the compositor scales down.
            scale.ceil() as i32
        } else {
            // Fractional scale WITHOUT viewporter: round to nearest integer and
            // accept the slight sizing mismatch.
            scale.round() as i32
        }
        .max(1);
        wl_surface.set_buffer_scale(buffer_scale);

        // Viewporter source coordinates apply *after* buffer scaling. Crop the
        // rounded SHM allocation to the actual rendered extent, then scale that
        // source to the authoritative logical size. Without a viewport, pool
        // configs stay exact-sized so this branch is not needed.
        if let Some(ref viewport) = self.viewport {
            let (source_width, source_height) =
                viewport_source_dimensions(physical_width, physical_height, buffer_scale);
            viewport.set_source(0.0, 0.0, source_width, source_height);
            viewport.set_destination(logical_width as i32, logical_height as i32);
        }

        wl_surface.frame(qh, wl_surface.clone());
        wl_surface.commit();
        self.frame_pending = true;
        self.frame_pending_since = Some(Instant::now());
        self.width = logical_width;
        self.height = logical_height;
        Ok(())
    }

    pub(super) fn restore_copied_damage(
        &mut self,
        index: usize,
        copied_damage: &[DamageRect],
        bounds: DamageRect,
    ) {
        if let Some(buffer) = self.shm_buffers.get_mut(index) {
            restore_pending_damage(&mut buffer.pending_damage, copied_damage, bounds);
        }
    }

    pub(in crate::wayland_surface) fn waiting_for_frame_callback(&self) -> bool {
        self.frame_pending
            && self
                .frame_pending_since
                .is_some_and(|since| since.elapsed() < MAX_FRAME_CALLBACK_WAIT)
    }
}

pub(super) fn surface_is_configured_or_missing(state: &State, surface_id: &str) -> bool {
    state
        .surfaces
        .get(surface_id)
        .map(|entry| entry.configured)
        .unwrap_or(true)
}

pub(super) fn set_pending_blur_regions(
    current: &mut Vec<DamageRect>,
    dirty: &mut bool,
    next: Vec<DamageRect>,
) {
    if *current == next && !*dirty {
        return;
    }
    *current = next;
    *dirty = true;
}
