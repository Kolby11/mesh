use super::*;
use mesh_core_render::DamageRect;
use smallvec::SmallVec;

impl WaylandSurfaceBackend {
    pub fn present_with_damage(
        &mut self,
        surface_id: &str,
        _title: &str,
        visible: bool,
        buffer: &PixelBuffer,
        damage_rects: &[DamageRect],
    ) -> Result<PresentStatus, PresentationError> {
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
            // present() called before configure() — nothing to do.
            return Ok(PresentStatus::Presented);
        }
        if !self.surface_ready_to_present(surface_id) {
            return Ok(PresentStatus::NotReady);
        }

        let qh = self.state.qh.clone();
        let state = &mut self.state;
        let pool = state
            .pool
            .as_mut()
            .ok_or_else(|| PresentationError::BufferAlloc("shm pool not initialised".into()))?;
        let Some(entry) = state.surfaces.get_mut(surface_id) else {
            return Ok(PresentStatus::Presented);
        };
        if !entry.configured {
            return Ok(PresentStatus::NotReady);
        }

        // Get the logical dimensions from compositor-configured size
        let logical_w = entry.width.max(1);
        let logical_h = entry.height.max(1);
        let scale = entry.scale;

        // SHM copy must use physical buffer dimensions for the copy region
        let physical_w = buffer.width().max(1);
        let physical_h = buffer.height().max(1);

        // Damage rects arrive in logical/CSS coordinates; scale to physical
        // before the copy so each SHM buffer can retain disjoint pending
        // regions without expanding them into one bounding rectangle.
        let mut shm_copy_damage: SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]> = damage_rects
            .iter()
            .copied()
            .map(|r| scale_damage_rect_to_physical(r, scale))
            .collect();
        if shm_copy_damage.is_empty() {
            // If the slice is empty (shouldn't normally reach here due to the
            // skip gate in render.rs), upload the full buffer.
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
        // Commit the kde_blur region before the wl_surface commit below.
        if entry.blur_region_dirty {
            if !entry.blur_regions.is_empty() {
                // Lazily create the blur object the first time this surface
                // actually needs blur, so surfaces without any
                // backdrop-filter never acquire the compositor's default
                // whole-surface blur.
                if entry.kde_blur.is_none()
                    && let Some(ref manager) = state.blur_manager
                {
                    let wl_surface = entry.wl_surface().clone();
                    entry.kde_blur = Some(manager.create(&wl_surface, &qh, ()));
                }
                if let Some(ref kde_blur) = entry.kde_blur
                    && let Ok(region) = Region::new(&state.compositor_state)
                {
                    for region_rect in &entry.blur_regions {
                        region.add(
                            region_rect.x as i32,
                            region_rect.y as i32,
                            region_rect.width as i32,
                            region_rect.height as i32,
                        );
                    }
                    kde_blur.set_region(Some(region.wl_region()));
                    kde_blur.commit();
                    entry.blur_committed = true;
                }
                entry.blur_region_dirty = false;
            } else {
                // No blur regions: remove blur entirely. A null region on
                // org_kde_kwin_blur means "blur the whole surface", not "no
                // blur", so we must unset via the manager and destroy the blur
                // object rather than calling set_region(None).
                if let Some(kde_blur) = entry.kde_blur.take() {
                    if let Some(ref manager) = state.blur_manager {
                        let wl_surface = entry.wl_surface().clone();
                        manager.unset(&wl_surface);
                    }
                    kde_blur.release();
                }
                entry.blur_committed = false;
                entry.blur_region_dirty = false;
            }
        }
        // Derive the input region from the reserve this surface declared and
        // apply it as pending state so the present commit below carries it.
        //
        // Recomputed on every present rather than pushed by a separate shell
        // call: the reserve is what makes the surface bigger than its content,
        // so re-deriving here is the invariant "a surface never takes input
        // over pixels it only reserved for painting" being enforced at the one
        // place that can enforce it. Comparing against the last committed value
        // keeps it to one protocol request per actual change, and an entry that
        // was destroyed and recreated (role swap, window hide/show) starts with
        // `applied_input_region: None` and so republishes automatically.
        let desired_input_region = entry.content_input_region();
        if entry.applied_input_region != Some(desired_input_region) {
            match desired_input_region {
                Some(rect) => {
                    if let Ok(region) = Region::new(&state.compositor_state) {
                        region.add(
                            rect.x as i32,
                            rect.y as i32,
                            rect.width as i32,
                            rect.height as i32,
                        );
                        entry
                            .wl_surface()
                            .set_input_region(Some(region.wl_region()));
                        entry.applied_input_region = Some(desired_input_region);
                    }
                }
                None => {
                    entry.wl_surface().set_input_region(None);
                    entry.applied_input_region = Some(desired_input_region);
                }
            }
        }
        // Window geometry rides the same commit as the buffer, so the
        // compositor never sees a frame whose declared window rect disagrees
        // with the pixels in it.
        entry.apply_window_geometry(desired_input_region.unwrap_or(DamageRect {
            x: 0,
            y: 0,
            width: logical_w,
            height: logical_h,
        }));
        entry.attach_shm_buffer(
            &qh,
            buffer_index,
            logical_w,
            logical_h,
            physical_w,
            physical_h,
            damage_rects,
            &copy_damage,
            scale,
        );

        Ok(PresentStatus::Presented)
    }

    pub(crate) fn update_opaque_region(
        &mut self,
        surface_id: &str,
        opaque_rect: Option<DamageRect>,
    ) {
        let Some(entry) = self.state.surfaces.get(surface_id) else {
            return;
        };
        let wl_surface = entry.wl_surface();

        let Some(rect) = opaque_rect else {
            wl_surface.set_opaque_region(None);
            return;
        };

        if rect.width == 0 || rect.height == 0 {
            wl_surface.set_opaque_region(None);
            return;
        }

        let Ok(region) = Region::new(&self.state.compositor_state) else {
            return;
        };
        region.add(
            rect.x as i32,
            rect.y as i32,
            rect.width as i32,
            rect.height as i32,
        );
        wl_surface.set_opaque_region(Some(region.wl_region()));
    }

    /// The input region currently derived for a surface, for tests and
    /// diagnostics. `None` for an unknown surface or one whose whole area is
    /// content.
    pub(crate) fn input_region(&self, surface_id: &str) -> Option<DamageRect> {
        self.state.surfaces.get(surface_id)?.content_input_region()
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
