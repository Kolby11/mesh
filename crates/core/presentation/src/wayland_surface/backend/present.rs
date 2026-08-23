use super::*;
use crate::NegotiatedCapabilities;
use mesh_core_render::DamageRect;
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy, Default)]
struct PreparedSurfaceState {
    input_region: Option<DamageRect>,
    input_region_staged: bool,
    opaque_region_staged: bool,
    window_geometry: Option<DamageRect>,
    blur_staged: bool,
}

impl PreparedSurfaceState {
    fn has_commit_work(self) -> bool {
        self.input_region_staged
            || self.opaque_region_staged
            || self.window_geometry.is_some()
            || self.blur_staged
    }
}

impl WaylandSurfaceBackend {
    pub fn present_with_damage(
        &mut self,
        surface_id: &str,
        _title: &str,
        visible: bool,
        buffer: &PixelBuffer,
        damage_rects: &[DamageRect],
    ) -> Result<PresentStatus, PresentationError> {
        if let Some(error) = self.state.connection_lost_error() {
            return Err(error);
        }
        if !visible {
            self.state.release_surface_focus_grab(surface_id);
            // A hidden window is *destroyed*, not detached. Detaching a buffer
            // unmaps an `xdg_toplevel`, and remapping it requires a fresh
            // configure the compositor is under no obligation to send for a
            // commit that carries no new state — measured on Hyprland 0.56, a
            // re-shown window waits out the configure deadline every frame and
            // never reappears. Recreating on the next `configure()` is the same
            // lifecycle promoted popovers already use, and it costs nothing
            // above the presentation layer: the component VM, retained tree,
            // and Lua state are untouched.
            if self
                .state
                .surfaces
                .get(surface_id)
                .is_some_and(|entry| entry.role.is_window())
            {
                self.destroy_surface(surface_id);
                return Ok(PresentStatus::Presented);
            }
            // Only detach a buffer (to hide) if the compositor has already configured this
            // surface. Before the first configure event the surface has no buffer attached
            // and is already invisible; committing a null buffer before configure arrives
            // triggers a Wayland protocol error.
            if let Some(entry) = self.state.surfaces.get_mut(surface_id)
                && entry.configured
            {
                // Clear compositor blur before hiding. A null
                // region blurs the whole surface, so destroy the blur object to
                // actually remove it rather than calling set_region(None).
                if let Some(kde_blur) = entry.kde_blur.take() {
                    if let Some(ref manager) = self.state.blur_manager {
                        let wl_surface = entry.wl_surface().clone();
                        manager.unset(&wl_surface);
                    }
                    kde_blur.release();
                }
                entry.blur_committed = false;
                entry.blur_region_dirty = false;
                entry.blur_regions.clear();
                entry.hide();
            }
            return Ok(PresentStatus::Presented);
        }

        if !self.state.surfaces.contains_key(surface_id) {
            // A visible frame for a surface that was closed or failed to be
            // created did not reach the compositor. Keep its damage alive so
            // the shell can recreate the role and retry the frame.
            return Ok(PresentStatus::SurfaceMissing);
        }
        if !self.surface_ready_to_present(surface_id) {
            return Ok(PresentStatus::NotReady);
        }

        let qh = self.state.qh.clone();
        // A spanning layer surface may receive a zero configure dimension.
        // Resolve it against the output selected for this surface before any
        // buffer validation or protocol state is prepared. The same extent is
        // exposed by `surface_size_if_known`, so shell paint and presentation
        // cannot disagree about the logical destination.
        let output_size = self.output_logical_size_for_surface(surface_id);
        let (buffer_index, copy_damage, logical_w, logical_h, physical_w, physical_h, scale) = {
            let state = &mut self.state;
            let pool = state
                .pool
                .as_mut()
                .ok_or_else(|| PresentationError::BufferAlloc("shm pool not initialised".into()))?;
            let Some(entry) = state.surfaces.get_mut(surface_id) else {
                return Ok(PresentStatus::SurfaceMissing);
            };
            if !entry.configured {
                return Ok(PresentStatus::NotReady);
            }

            let (logical_w, logical_h) = entry.resolved_extent(output_size);
            let scale = entry.scale;

            // SHM copy must use physical buffer dimensions for the copy region.
            let physical_w = buffer.width().max(1);
            let physical_h = buffer.height().max(1);

            // Damage rects arrive in logical/CSS coordinates; scale to physical
            // before the copy so each SHM buffer can retain disjoint pending
            // regions without expanding them into one bounding rectangle.
            let mut shm_copy_damage: SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]> =
                damage_rects
                    .iter()
                    .copied()
                    .map(|r| scale_damage_rect_to_physical(r, scale))
                    .collect();
            if shm_copy_damage.is_empty() {
                // If the slice is empty (shouldn't normally reach here due to
                // the skip gate in render.rs), upload the full buffer.
                shm_copy_damage.push(full_damage(physical_w, physical_h));
            }
            let Some((buffer_index, copy_damage)) = entry.copy_into_shm_buffer(
                pool,
                buffer.data(),
                physical_w,
                physical_h,
                &shm_copy_damage,
            )?
            else {
                return Ok(PresentStatus::NotReady);
            };
            (
                buffer_index,
                copy_damage,
                logical_w,
                logical_h,
                physical_w,
                physical_h,
                scale,
            )
        };
        // Stage compositor state only after a reusable SHM buffer has been
        // prepared. A buffer backpressure/allocation failure must not consume
        // a region update that still needs a successful wl_surface commit.
        let prepared_state = self.prepare_surface_state(surface_id);
        // Window geometry rides the same commit as the buffer, so the
        // compositor never sees a frame whose declared window rect disagrees
        // with the pixels in it.
        let state = &mut self.state;
        let Some(entry) = state.surfaces.get_mut(surface_id) else {
            return Ok(PresentStatus::SurfaceMissing);
        };
        if let Err(error) = entry.attach_shm_buffer(
            &qh,
            surface_id,
            buffer_index,
            logical_w,
            logical_h,
            physical_w,
            physical_h,
            damage_rects,
            &copy_damage,
            scale,
        ) {
            // copy_into_shm_buffer consumes the selected slot's accumulated
            // damage before copying it. Put it back when activation/attach
            // fails, otherwise a retry would publish a partially refreshed
            // SHM buffer as if the frame had been delivered.
            entry.restore_copied_damage(
                buffer_index,
                &copy_damage,
                full_damage(physical_w, physical_h),
            );
            return Err(error);
        }
        Self::mark_surface_state_committed(entry, prepared_state);

        Ok(PresentStatus::Presented)
    }

    /// Commit staged surface state without attaching a new SHM buffer. This is
    /// the path used for region-only updates on otherwise idle frames.
    pub(crate) fn commit_surface_state(
        &mut self,
        surface_id: &str,
    ) -> Result<SurfaceStateStatus, PresentationError> {
        if let Some(error) = self.state.connection_lost_error() {
            return Err(error);
        }
        let Some(entry) = self.state.surfaces.get(surface_id) else {
            return Ok(SurfaceStateStatus::SurfaceMissing);
        };
        if !entry.configured {
            return Ok(SurfaceStateStatus::NotReady);
        }
        let prepared_state = self.prepare_surface_state(surface_id);
        if !prepared_state.has_commit_work() {
            return Ok(SurfaceStateStatus::Unchanged);
        }
        if let Some(entry) = self.state.surfaces.get(surface_id) {
            entry.wl_surface().commit();
        }
        if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
            Self::mark_surface_state_committed(entry, prepared_state);
        }
        Ok(SurfaceStateStatus::Committed)
    }

    fn prepare_surface_state(&mut self, surface_id: &str) -> PreparedSurfaceState {
        let qh = self.state.qh.clone();
        let output_size = self.output_logical_size_for_surface(surface_id);
        let state = &mut self.state;
        let Some(entry) = state.surfaces.get_mut(surface_id) else {
            return PreparedSurfaceState::default();
        };
        let extent = entry.resolved_extent(output_size);
        let input_region = entry.content_input_region_for_extent(extent);
        let input_region_staged = entry.input_region_needs_commit(input_region)
            && entry.stage_input_region(input_region, &state.compositor_state);
        let opaque_region_staged = entry.stage_opaque_region(&state.compositor_state);
        let blur_staged =
            stage_blur_region(&state.blur_manager, &state.compositor_state, entry, &qh);
        let window_geometry = entry.stage_window_geometry(input_region.unwrap_or(DamageRect {
            x: 0,
            y: 0,
            width: extent.0,
            height: extent.1,
        }));
        PreparedSurfaceState {
            input_region,
            input_region_staged,
            opaque_region_staged,
            window_geometry,
            blur_staged,
        }
    }

    fn mark_surface_state_committed(entry: &mut SurfaceEntry, prepared: PreparedSurfaceState) {
        if prepared.input_region_staged {
            entry.mark_input_region_committed(prepared.input_region);
        }
        if prepared.opaque_region_staged {
            entry.mark_opaque_region_committed();
        }
        if prepared.blur_staged {
            entry.blur_region_dirty = false;
        }
        if let Some(geometry) = prepared.window_geometry {
            entry.mark_window_geometry_committed(geometry);
        }
    }

    pub(crate) fn update_opaque_region(
        &mut self,
        surface_id: &str,
        opaque_rect: Option<DamageRect>,
    ) {
        let Some(entry) = self.state.surfaces.get_mut(surface_id) else {
            return;
        };
        let opaque_rect = opaque_rect.filter(|rect| rect.width > 0 && rect.height > 0);
        if entry.pending_opaque_region != opaque_rect {
            entry.pending_opaque_region = opaque_rect;
            entry.opaque_region_dirty = true;
        }
    }

    /// The input region currently derived for a surface, for tests and
    /// diagnostics. `None` for an unknown surface or one whose whole area is
    /// content.
    pub(crate) fn input_region(&self, surface_id: &str) -> Option<DamageRect> {
        let output_size = self.output_logical_size_for_surface(surface_id);
        let entry = self.state.surfaces.get(surface_id)?;
        entry.content_input_region_for_extent(entry.resolved_extent(output_size))
    }

    /// Set the logical-coordinate blur regions for a surface.
    /// The regions are sent as kde_blur protocol calls before the next
    /// wl_surface.commit(). If `blur_regions` is empty, no kde_blur
    /// calls are emitted — the compositor gets no blur hint.
    pub(crate) fn update_blur_regions(&mut self, surface_id: &str, blur_regions: Vec<DamageRect>) {
        let Some(entry) = self.state.surfaces.get_mut(surface_id) else {
            return;
        };
        set_pending_blur_regions(
            &mut entry.blur_regions,
            &mut entry.blur_region_dirty,
            blur_regions,
        );
    }

    pub fn surface_size(
        &mut self,
        surface_id: &str,
    ) -> Result<Option<(u32, u32)>, PresentationError> {
        self.dispatch_available()?;

        Ok(self.surface_size_if_known(surface_id))
    }

    pub fn surface_ready_to_present(&self, surface_id: &str) -> bool {
        surface_is_configured_or_missing(&self.state, surface_id)
    }

    pub fn surface_size_if_known(&self, surface_id: &str) -> Option<(u32, u32)> {
        let output_size = self.output_logical_size_for_surface(surface_id);
        self.state
            .surfaces
            .get(surface_id)
            .filter(|entry| entry.configured)
            .map(|entry| resolved_surface_size(entry, output_size))
    }

    pub fn surface_waiting_for_frame_callback(&self, surface_id: &str) -> bool {
        self.state
            .surfaces
            .get(surface_id)
            .is_some_and(SurfaceEntry::waiting_for_frame_callback)
    }

    pub fn surface_waiting_for_buffer_release(&self, surface_id: &str) -> bool {
        self.state
            .surfaces
            .get(surface_id)
            .is_some_and(SurfaceEntry::waiting_for_buffer_release)
    }

    pub fn surface_generation(&self, surface_id: &str) -> Option<SurfaceGeneration> {
        self.state
            .surfaces
            .get(surface_id)
            .map(SurfaceEntry::surface_generation)
    }

    pub fn negotiated_capabilities(&self) -> NegotiatedCapabilities {
        self.state.negotiated_capabilities
    }

    pub fn surface_scale(&self, surface_id: &str) -> f32 {
        self.state
            .surfaces
            .get(surface_id)
            .map(|entry| entry.scale)
            .unwrap_or(1.0)
    }

    pub fn surface_needs_full_redraw(&self, surface_id: &str) -> bool {
        self.state
            .surfaces
            .get(surface_id)
            .map(|entry| entry.needs_full_redraw)
            .unwrap_or(false)
    }

    pub fn clear_surface_needs_full_redraw(&mut self, surface_id: &str) {
        if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
            entry.needs_full_redraw = false;
        }
    }
}

fn stage_blur_region(
    blur_manager: &Option<OrgKdeKwinBlurManager>,
    compositor_state: &CompositorState,
    entry: &mut SurfaceEntry,
    qh: &QueueHandle<State>,
) -> bool {
    if !entry.blur_region_dirty {
        return false;
    }
    if !entry.blur_regions.is_empty() {
        // Lazily create the blur object the first time this surface actually
        // needs blur, so surfaces without a backdrop filter never acquire the
        // compositor's default whole-surface blur.
        if entry.kde_blur.is_none()
            && let Some(manager) = blur_manager.as_ref()
        {
            let wl_surface = entry.wl_surface().clone();
            entry.kde_blur = Some(manager.create(&wl_surface, qh, ()));
        }
        if let Some(kde_blur) = entry.kde_blur.as_ref()
            && let Ok(region) = Region::new(compositor_state)
        {
            for rect in &entry.blur_regions {
                region.add(
                    rect.x as i32,
                    rect.y as i32,
                    rect.width as i32,
                    rect.height as i32,
                );
            }
            kde_blur.set_region(Some(region.wl_region()));
            kde_blur.commit();
            entry.blur_committed = true;
            return true;
        }
        // A compositor without the optional blur manager cannot apply this
        // hint. Treat it as an intentional no-op rather than retrying forever;
        // a later backend recreation will receive the desired regions again.
        if blur_manager.is_none() {
            entry.blur_region_dirty = false;
        }
        return false;
    }

    // A null KDE blur region means whole-surface blur, so clear blur by
    // unsetting and releasing the auxiliary object instead.
    if let Some(kde_blur) = entry.kde_blur.take() {
        if let Some(manager) = blur_manager.as_ref() {
            let wl_surface = entry.wl_surface().clone();
            manager.unset(&wl_surface);
        }
        kde_blur.release();
        entry.blur_committed = false;
        return true;
    }
    entry.blur_committed = false;
    entry.blur_region_dirty = false;
    false
}
