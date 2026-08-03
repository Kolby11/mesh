mod child;
mod popup;

#[cfg(test)]
mod performance_tests;
#[cfg(test)]
mod tests;

use popup::*;

use super::super::*;
use crate::shell::types::{ChildSurface, SurfaceTarget};
use mesh_core_elements::style::BackgroundPaint;
use mesh_core_elements::{PopoverAnchor, PopoverConstraintAdjustment, PopoverGrab, PopoverGravity};
use mesh_core_presentation::{
    LayerSurfaceSizePolicy, PopupAnchor, PopupConfig, PopupConstraint, PopupGravity,
    PopupPlacement, PresentStatus, SurfacePadding,
};
use mesh_core_render::{DamageRect, DisplayPaintCommand};
use mesh_core_wayland::SurfaceRole;
use smallvec::SmallVec;

const DEBUG_INSPECTOR_SURFACE_ID: &str = "@mesh/debug-inspector";

impl Shell {
    pub(in crate::shell) fn render_components(&mut self) -> Result<(), ShellRunError> {
        self.drain_dismissed_popups()?;
        self.drain_window_close_requests()?;

        if self.debug.enabled {
            let mut debug_requests = self.publish_debug_snapshot()?;
            self.drain_requests(&mut debug_requests)?;
        }

        let mut components_want_render_after_frame = false;
        let mut any_component_presented = false;
        for index in 0..self.components.len() {
            let surface_id = self.components[index].surface_id.clone();
            // Ahead of the `wants_render` gate: a compositor state change is
            // precisely what makes an otherwise-quiet window want to render.
            // Asking behind the gate would strand a fullscreened surface at its
            // floating style until something else happened to dirty it.
            // Keyed off the shell-surface record — what has actually been
            // applied — not the component's requested role. A promotion is
            // applied by `render_layout` further down this same frame, so the
            // record catches up before the compositor has any states to report.
            // Demotion needs no delivery here at all: `surface_role_changed`
            // clears the states itself, because a layer surface has none.
            if self
                .surfaces
                .get(&surface_id)
                .is_some_and(|surface| surface.role == SurfaceRole::Window)
            {
                let states = self.presentation_engine.window_states(&surface_id);
                self.components[index]
                    .component
                    .surface_window_states_changed(states);
            }
            if !self.components[index].component.wants_render() {
                continue;
            }
            let visible = self.surface_is_effectively_visible(&surface_id);
            if !visible
                && self.components[index].parent.last_surface_config.is_none()
                && self.components[index].parent.known_surface_size.is_none()
            {
                continue;
            }
            if visible
                && self
                    .presentation_engine
                    .surface_waiting_for_frame_callback(&surface_id)
            {
                components_want_render_after_frame = true;
                continue;
            }
            if visible
                && !self
                    .presentation_engine
                    .surface_ready_to_present(&surface_id)
            {
                components_want_render_after_frame = true;
                continue;
            }
            let surface_size = {
                let surface = self
                    .surfaces
                    .get(&surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(surface_id.clone()))?;
                if surface.width == 0 || surface.height == 0 {
                    self.presentation_engine.surface_size(&surface_id)?
                } else {
                    Some((surface.width.max(1), surface.height.max(1)))
                }
            };
            if let Some((width, height)) = surface_size {
                let resolved_size = (width, height);
                if self.components[index].parent.known_surface_size != Some(resolved_size) {
                    self.components[index].parent.known_surface_size = Some(resolved_size);
                    self.components[index]
                        .component
                        .surface_size_changed(width, height);
                }
            }
            let profiling_enabled = self.profiling_enabled();
            let total_render_started = profiling_enabled.then(std::time::Instant::now);
            let allocation_started = (profiling_enabled
                && mesh_core_debug::allocation::profiling_available())
            .then(mesh_core_debug::allocation::snapshot);
            let mut rerender_attempts = 0;
            let mut component_stage_records = Vec::new();
            let component_id = surface_id.as_str();
            // Hoist logical dimensions and scale before the loop so that
            // the post-loop force-full-redraw and debug-overlay paths can
            // reference them without depending on loop-scoped mutable borrows.
            let (width, height, scale) = {
                let surface = self
                    .surfaces
                    .get(&surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(surface_id.clone()))?;
                let requested_width = surface.width;
                let requested_height = surface.height;
                let (width, height) = if requested_width == 0 || requested_height == 0 {
                    let dynamic_size = self.resolve_dynamic_surface_size(index, &surface_id)?;
                    let w = if requested_width == 0 {
                        dynamic_size.map(|(w, _)| w).unwrap_or(1)
                    } else {
                        requested_width.max(1)
                    };
                    let h = if requested_height == 0 {
                        dynamic_size.map(|(_, h)| h).unwrap_or(1)
                    } else {
                        requested_height.max(1)
                    };
                    (w, h)
                } else {
                    (requested_width.max(1), requested_height.max(1))
                };
                // See the in-loop override below: for a window the compositor's
                // configured size wins over CSS measurement.
                let (width, height) = self
                    .presentation_engine
                    .window_configured_size(&surface_id)
                    .map(|(w, h)| (w.max(1), h.max(1)))
                    .unwrap_or((width, height));
                let scale = self.presentation_engine.surface_scale(&surface_id);
                (width, height, scale)
            };
            let mut width = width;
            let mut height = height;
            let mut scale = scale;
            // Buffer/present dimensions: content plus the tooltip overlay
            // reserve for parent layer surfaces (popups stay content-sized).
            let mut paint_width = width;
            let mut paint_height = height;
            loop {
                let surface = self
                    .surfaces
                    .get_mut(&surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(surface_id.clone()))?;
                self.components[index]
                    .component
                    .set_profiling_enabled(profiling_enabled);
                self.components[index]
                    .component
                    .render(surface)
                    .map_err(ShellRunError::Component)?;

                let visible = self
                    .core
                    .surfaces
                    .get(&surface_id)
                    .map(|state| state.visible)
                    .unwrap_or(surface.visible);
                if !visible {
                    // Do not reconfigure hidden surfaces to synthetic 1x1/zero-margin
                    // geometry before detaching them. Some compositors can show that
                    // transient geometry during close, which makes anchored popovers
                    // appear to fly toward the default screen position.
                    let runtime = &mut self.components[index];
                    if runtime
                        .parent
                        .paint_buffer
                        .as_ref()
                        .map(|buffer| buffer.width != 1 || buffer.height != 1)
                        .unwrap_or(true)
                    {
                        runtime.parent.paint_buffer = Some(PixelBuffer::new(1, 1));
                    }
                    runtime.parent.known_surface_size = None;
                    runtime.parent.last_surface_config = None;
                    runtime.parent.last_popup_size = None;
                    break;
                }

                // Popup surfaces (xdg_popup) skip the layer-surface configure
                // path entirely — they are created/repositioned via
                // configure_popup() after the content size is resolved below.
                let is_popup = self.components[index].parent.popup_config.is_some();

                // A never-before-configured layer surface has no retained
                // widget tree yet, so `render()` (which measures content from
                // the *existing* tree) reports a stale/zero size on this very
                // first pass — the real tree is only built moments later by
                // `paint()` below. Popups already dodge this with
                // `defer_popup_create` + an immediate-rerender pass; mirror
                // that here so the first-ever configure for a layer surface
                // isn't sent with an unmeasured size (which the layer-shell
                // backend then has to clamp to a broken 1x1, see
                // `layer_protocol_size`).
                let first_layer_configure =
                    !is_popup && self.components[index].parent.last_surface_config.is_none();

                // Compare all copy fields before cloning namespace (the only heap field).
                let size_policy = self.components[index].parent.surface_size_policy;
                let layer = surface.layer.unwrap_or(Layer::Top);
                // The compositor-facing layer surface is inflated by the
                // tooltip overlay reserve so tooltips can paint outside the
                // content box. `surface.width/height` (and everything the
                // component sees) stay content-sized, and `configured_padding`
                // travels with the inflated size so the presentation layer can
                // confine pointer input back to the content rect.
                let (configured_width, configured_height, configured_padding) =
                    surface_geometry_with_overlay_reserve(
                        &surface_id,
                        surface.role,
                        surface.width,
                        surface.height,
                    );
                let config_changed = self.components[index]
                    .parent
                    .last_surface_config
                    .as_ref()
                    .map_or(true, |last| {
                        last.edge != surface.edge
                            || last.layer != layer
                            || last.size_policy != size_policy
                            || last.width != configured_width
                            || last.height != configured_height
                            || last.padding != configured_padding
                            || last.exclusive_zone != surface.exclusive_zone
                            || last.keyboard_mode != surface.keyboard_mode
                            || last.margin_top != surface.margin_top
                            || last.margin_right != surface.margin_right
                            || last.margin_bottom != surface.margin_bottom
                            || last.margin_left != surface.margin_left
                            || last.blur != surface.blur
                            || last.role != surface.role
                            || last.window != surface.window
                    });
                if config_changed && !is_popup {
                    let cfg = SurfaceConfig {
                        role: surface.role,
                        window: surface.window.clone(),
                        edge: surface.edge,
                        layer,
                        size_policy,
                        width: configured_width,
                        height: configured_height,
                        padding: configured_padding,
                        exclusive_zone: surface.exclusive_zone,
                        keyboard_mode: surface.keyboard_mode,
                        namespace: surface_id.clone(),
                        margin_top: surface.margin_top,
                        margin_right: surface.margin_right,
                        margin_bottom: surface.margin_bottom,
                        margin_left: surface.margin_left,
                        blur: surface.blur,
                    };
                    tracing::debug!(
                        surface_id = %surface_id,
                        width = configured_width,
                        height = configured_height,
                        edge = ?surface.edge,
                        exclusive_zone = surface.exclusive_zone,
                        margin_top = surface.margin_top,
                        margin_right = surface.margin_right,
                        margin_bottom = surface.margin_bottom,
                        margin_left = surface.margin_left,
                        first_layer_configure,
                        rerender_attempts,
                        "sending layer-surface configure"
                    );
                    self.presentation_engine.configure(&surface_id, cfg.clone());
                    self.components[index].parent.last_surface_config = Some(cfg);
                    // A geometry-changing configure invalidates the
                    // compositor's previously-acked size: `apply_config`
                    // flips the backend's `entry.configured` to false until a
                    // fresh configure event arrives, but `known_surface_size`
                    // here is a separate shell-side cache that isn't tied to
                    // that flag. Left stale, `resolve_dynamic_surface_size`
                    // short-circuits on the old (possibly clamped-to-1x1)
                    // size below instead of waiting for the new ack, which
                    // pins the surface at its first-ever (often wrong) size
                    // forever.
                    self.components[index].parent.known_surface_size = None;
                }

                let inner_requested_width = surface.width;
                let inner_requested_height = surface.height;
                // A content-measured popup has no real size until its first
                // paint measures the content. Defer creating the `xdg_popup`
                // until the loop's immediate-rerender pass below, so it is
                // created at the measured size instead of a placeholder that
                // visibly grows on the next open.
                let defer_popup_create =
                    is_popup && self.components[index].component.needs_content_measure();
                if is_popup {
                    // A popup's size must come from the component's own
                    // CSS-measured content size, NOT the presentation surface
                    // size. The presentation/shell-surface size can be unknown
                    // before first creation, compositor-reported after creation,
                    // or a stale layer-surface render-buffer size that includes
                    // transparent tooltip padding. `render` runs the loop's
                    // first paint, which populates `measured_size`; the loop's
                    // immediate-rerender pass then reaches this point with the
                    // real measured size and creates/repositions the popup to
                    // that geometry within the same frame. (Layer surfaces keep
                    // their own `set_size`/`resolve_dynamic_surface_size` path
                    // below; it feeds `measured_size` to the compositor via
                    // `render_layout`, which is skipped for promoted popups.)
                    let (measured_w, measured_h) =
                        self.components[index].component.declared_or_measured_size();
                    width = measured_w.max(1);
                    height = measured_h.max(1);
                } else {
                    let dynamic_size = if inner_requested_width == 0 || inner_requested_height == 0
                    {
                        self.resolve_dynamic_surface_size(index, &surface_id)?
                    } else {
                        None
                    };
                    let (fallback_width, fallback_height) =
                        self.components[index].component.declared_or_measured_size();
                    width = if inner_requested_width == 0 {
                        dynamic_size
                            .map(|(w, _)| w)
                            .or((fallback_width > 0).then_some(fallback_width))
                            .unwrap_or(1)
                    } else {
                        inner_requested_width.max(1)
                    };
                    height = if inner_requested_height == 0 {
                        dynamic_size
                            .map(|(_, h)| h)
                            .or((fallback_height > 0).then_some(fallback_height))
                            .unwrap_or(1)
                    } else {
                        inner_requested_height.max(1)
                    };
                }
                // A window surface sizes in the opposite direction to a layer
                // surface: once the compositor has decided a size (tiling
                // layout, maximize, interactive resize) that decision is
                // binding and the content lays out into it, rather than the
                // CSS-measured size being sent as a request.
                if let Some((window_width, window_height)) =
                    self.presentation_engine.window_configured_size(&surface_id)
                {
                    width = window_width.max(1);
                    height = window_height.max(1);
                }
                let resolved_size = (width, height);
                if self.components[index].parent.known_surface_size != Some(resolved_size) {
                    self.components[index].parent.known_surface_size = Some(resolved_size);
                    self.components[index]
                        .component
                        .surface_size_changed(width, height);
                }

                // For xdg_popup surfaces, call configure_popup with the
                // resolved content size. This creates the surface on first
                // show and repositions it when the size changes (e.g. the
                // content grows or shrinks between opens).
                if is_popup
                    && !defer_popup_create
                    && self.components[index].parent.last_popup_size != Some(resolved_size)
                {
                    self.components[index].parent.last_popup_size = Some(resolved_size);
                    let config = self.components[index]
                        .parent
                        .popup_config
                        .as_mut()
                        .map(|c| {
                            c.placement.size = resolved_size;
                            c.clone()
                        });
                    if let Some(config) = config {
                        if let Err(e) = self
                            .presentation_engine
                            .configure_popup(&surface_id, config)
                        {
                            tracing::warn!("configure_popup for {surface_id} failed: {e}");
                        }
                    }
                }

                scale = self.presentation_engine.surface_scale(&surface_id);
                // The paint buffer matches the compositor-configured surface:
                // content plus the tooltip overlay reserve for parent layer
                // surfaces. `width`/`height` stay content-sized for the
                // component-facing notifications and popup config above, and
                // the component is handed both halves so its own unmeasured
                // fallback can never pick up the reserve.
                let paint_extent = if is_popup {
                    SurfaceExtent::unpadded(width, height)
                } else {
                    let (padded_width, padded_height, _) = surface_geometry_with_overlay_reserve(
                        &surface_id,
                        self.surfaces
                            .get(&surface_id)
                            .map(|surface| surface.role)
                            .unwrap_or_default(),
                        width,
                        height,
                    );
                    SurfaceExtent::padded((width, height), (padded_width, padded_height))
                };
                (paint_width, paint_height) = paint_extent.padded;
                let physical_w = ((paint_width as f32 * scale).ceil() as u32).max(1);
                let physical_h = ((paint_height as f32 * scale).ceil() as u32).max(1);

                // Cap the buffer allocation so a bad measured size cannot ask
                // for gigabytes.
                const MAX_BUFFER_BYTES: u64 = 512 * 1024 * 1024;
                let requested_bytes = (physical_w as u64) * (physical_h as u64) * 4;
                if requested_bytes > MAX_BUFFER_BYTES {
                    return Err(ShellRunError::BufferAlloc {
                        surface_id: surface_id.clone(),
                        logical_w: paint_width,
                        logical_h: paint_height,
                        physical_w,
                        physical_h,
                        scale,
                        requested_bytes,
                        max_bytes: MAX_BUFFER_BYTES,
                    });
                }

                let runtime = &mut self.components[index];
                if runtime
                    .parent
                    .paint_buffer
                    .as_ref()
                    .map(|buffer| buffer.width != physical_w || buffer.height != physical_h)
                    .unwrap_or(true)
                {
                    runtime.parent.paint_buffer = Some(PixelBuffer::new(physical_w, physical_h));
                    // A resized buffer starts fully transparent; `paint()` only
                    // repaints dirty regions against the retained tree, so
                    // without forcing a full present the untouched pixels of a
                    // freshly-allocated buffer never get drawn until something
                    // else marks the whole surface dirty.
                    runtime.parent.force_full_present = true;
                }
                runtime
                    .component
                    .paint(
                        self.theme.active(),
                        paint_extent,
                        runtime
                            .parent
                            .paint_buffer
                            .as_mut()
                            .expect("paint buffer initialised"),
                        scale,
                    )
                    .map_err(ShellRunError::Component)?;
                component_stage_records.extend(runtime.component.take_profiling_records());

                // When popup creation was deferred to measure the content, the
                // paint above has now populated `measured_size`; force one more
                // iteration so the `xdg_popup` is created at the measured size
                // (the immediate-rerender gate alone returns false for a
                // surface-config-only change). Layer surfaces get the same
                // treatment on their first-ever configure: the paint above
                // just built the retained tree for the first time, so
                // re-running render() now lets it re-measure and send a
                // corrected `configure()` instead of leaving the surface
                // stuck at the unmeasured first-pass size.
                if (!self.components[index].component.wants_immediate_rerender()
                    && !defer_popup_create
                    && !first_layer_configure)
                    || rerender_attempts >= 1
                {
                    break;
                }

                rerender_attempts += 1;
            }

            // Component(VM)-level profiling + invalidation are recorded once,
            // regardless of how many surface targets the component drives.
            let observation_summary = self.components[index]
                .component
                .service_observation_summary();
            self.service_delivery_index
                .mark_dirty_if_summary_changed(index, observation_summary);
            for record in component_stage_records {
                let module_id = record
                    .module_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                    .or(Some(component_id));
                self.record_surface_profiling_stage(
                    &surface_id,
                    module_id,
                    record.stage,
                    record.duration,
                    record.trigger_kind.as_deref(),
                );
            }
            if let Some(invalidation) = self.components[index]
                .component
                .take_invalidation_snapshot()
            {
                self.record_surface_invalidation(&surface_id, Some(component_id), invalidation);
            }

            // Present the component's parent surface. Child popup targets paint
            // their own subtree and are presented separately during reconcile.
            let presented = self.present_surface_target(
                index,
                TargetRef::Parent,
                component_id,
                paint_width,
                paint_height,
                scale,
                total_render_started,
                allocation_started,
            )?;
            any_component_presented |= presented;
            if presented {
                components_want_render_after_frame |=
                    self.components[index].component.wants_render();
            }

            let child_presented = self.reconcile_child_surface_requests(
                index,
                component_id,
                &surface_id,
                scale,
                total_render_started,
            )?;
            any_component_presented |= child_presented;
            // Reconciliation can invalidate the component without presenting
            // a child yet (notably the staged first entrance paint).
            components_want_render_after_frame |= self.components[index].component.wants_render();
        }
        self.components_want_render = components_want_render_after_frame;
        self.presented_last_frame = any_component_presented;
        Ok(())
    }

    /// Act on close requests from window surfaces (title-bar button, compositor
    /// close binding).
    ///
    /// Closing hides the surface rather than destroying the component: the
    /// module, its services, and its Lua state survive, so reopening the window
    /// is the same cheap show that reopening a hidden panel is. Nothing here
    /// unmaps the toplevel directly — `set_surface_visibility_now` runs the
    /// same hide path every other surface uses, including the CSS exit
    /// transition.
    fn drain_window_close_requests(&mut self) -> Result<(), ShellRunError> {
        for surface_id in self.presentation_engine.take_close_requests() {
            if self.component_index_for_surface(&surface_id).is_none() {
                continue;
            }
            tracing::info!(surface_id, "window close requested; hiding surface");
            let mut pending = self.set_surface_visibility_now(surface_id, false)?;
            self.drain_requests(&mut pending)?;
        }
        Ok(())
    }

    fn drain_dismissed_popups(&mut self) -> Result<(), ShellRunError> {
        for surface_id in self.presentation_engine.take_dismissed_popups() {
            match self.component_target_for_surface(&surface_id) {
                Some((index, TargetRef::Child(child_index))) => {
                    let node_key = self.components[index].children[child_index]
                        .node_key
                        .clone();
                    self.destroy_child_surface_at(index, child_index);
                    if let Some(runtime) = self.components.get_mut(index) {
                        runtime.dismissed_child_node_keys.insert(node_key);
                    }
                }
                Some((index, TargetRef::Parent))
                    if self.components[index].parent.popup_parent_surface.is_some() =>
                {
                    self.pending_popover_hides.remove(&surface_id);
                    let mut pending = self.set_surface_visibility_now(surface_id, false)?;
                    self.drain_requests(&mut pending)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(in crate::shell) fn destroy_all_child_surfaces(&mut self, index: usize) {
        while !self.components[index].children.is_empty() {
            self.destroy_child_surface_at(index, 0);
        }
    }

    pub(in crate::shell) fn destroy_child_surface_at(&mut self, index: usize, child_index: usize) {
        if child_index >= self.components[index].children.len() {
            return;
        }
        let surface_id = self.components[index].children[child_index]
            .target
            .surface_id
            .clone();
        self.components[index].children.remove(child_index);
        self.presentation_engine.destroy_popup(&surface_id);
        self.core.surfaces.remove(&surface_id);
        self.surfaces.remove(&surface_id);
        self.component_by_surface.remove(&surface_id);
        if self.keyboard_focus_surface.as_deref() == Some(surface_id.as_str()) {
            self.keyboard_focus_surface = None;
        }
        self.transfer_owned_keyboard_modes.remove(&surface_id);
        self.rebuild_component_surface_index();
    }

    /// Run the post-paint present pipeline for one surface target of a
    /// component — its parent surface, or (later) a child popup. Computes
    /// opaque/input/blur regions, resolves present damage (handling force-full
    /// and scale-change full redraws), paints the debug layout overlay, commits
    /// the buffer, and records profiling. Returns whether a present was issued.
    ///
    /// Region and debug-overlay computation is parent-only for now; child popup
    /// targets supply their own subtree damage when reconciled.
    fn present_surface_target(
        &mut self,
        index: usize,
        target: TargetRef,
        component_id: &str,
        width: u32,
        height: u32,
        scale: f32,
        total_render_started: Option<std::time::Instant>,
        allocation_started: Option<mesh_core_debug::allocation::AllocationCounters>,
    ) -> Result<bool, ShellRunError> {
        let surface_id = self.components[index].target(target).surface_id.clone();
        let visible = self.surface_is_effectively_visible(&surface_id);
        let is_parent = matches!(target, TargetRef::Parent);

        if visible && is_parent {
            let generation = self.components[index].component.display_list_generation();
            let surface_size = self.components[index].target(target).known_surface_size;
            let content_size = self.components[index].component.content_input_size();
            let region_state = (generation, surface_size, content_size);
            if self.components[index].target(target).last_region_state != Some(region_state) {
                let commands = self.components[index]
                    .component
                    .display_list_paint_commands();
                let opaque_rect = surface_size.and_then(|(surface_w, surface_h)| {
                    compute_opaque_rect_for_root(commands, surface_w, surface_h)
                });
                self.presentation_engine
                    .update_opaque_region(&surface_id, opaque_rect);

                // No input-region push here on purpose. Confining pointer input
                // to the content rect used to live at this call site, guarded by
                // the `last_region_state` cache above — and every way that cache
                // could be warm while the compositor object was not (a surface
                // recreated for a role swap, a window destroyed on hide, a
                // present that returned before the region was flushed) brought
                // back the dead zone under the bar. The reserve now travels with
                // the size that created it (`SurfacePadding` in the surface
                // config) and the region is re-derived on every commit.

                let blur_regions = self.components[index]
                    .component
                    .display_list_blur_regions()
                    .to_vec();
                self.presentation_engine
                    .update_blur_regions(&surface_id, blur_regions);
                self.components[index].target_mut(target).last_region_state = Some(region_state);
            }
        }

        let mut present_damage: Vec<DamageRect> = match target {
            TargetRef::Parent => self.components[index].component.take_present_damage(),
            TargetRef::Child(child_index) => std::mem::take(
                &mut self.components[index].children[child_index].pending_present_damage,
            ),
        };
        present_damage.extend(std::mem::take(
            &mut self.components[index]
                .target_mut(target)
                .pending_present_damage,
        ));
        // Scale change or explicit force-full triggers full-buffer present (per HDPI-04)
        let mut force_full = false;
        let mut hud_restore = None;
        if visible
            && self
                .presentation_engine
                .surface_needs_full_redraw(&surface_id)
        {
            force_full = true;
            self.presentation_engine
                .clear_surface_needs_full_redraw(&surface_id);
            tracing::debug!(
                surface_id = surface_id.as_str(),
                "scale change triggered full-buffer present"
            );
        }
        if visible && self.components[index].target(target).force_full_present {
            force_full = true;
            self.components[index].target_mut(target).force_full_present = false;
        }
        if force_full {
            // Emit full damage in logical coordinates (attach_shm_buffer scales to physical)
            present_damage = vec![DamageRect {
                x: 0,
                y: 0,
                width: width.max(1),
                height: height.max(1),
            }];
        }
        if visible
            && is_parent
            && self.debug.enabled
            && self.debug.profiling_enabled
            && surface_id != DEBUG_INSPECTOR_SURFACE_ID
        {
            let hud_snapshot = self.profiling.perf_hud_snapshot(&surface_id);
            let flashed_damage = present_damage.clone();
            let buffer = self.components[index]
                .target_mut(target)
                .paint_buffer
                .as_mut()
                .expect("paint buffer initialised");
            hud_restore = Some(self.debug_overlay.paint_performance_hud(
                buffer,
                scale,
                &hud_snapshot,
                &flashed_damage,
            ));
            let hud_unit = scale.round().max(1.0);
            present_damage.push(DamageRect {
                x: 0,
                y: 0,
                width: ((184.0 * hud_unit) / scale.max(f32::EPSILON)).ceil() as u32,
                height: ((70.0 * hud_unit) / scale.max(f32::EPSILON)).ceil() as u32,
            });
        }
        if visible && self.debug.show_layout_bounds {
            let debug_tree = match target {
                TargetRef::Parent => self.components[index].component.last_widget_tree().cloned(),
                TargetRef::Child(child_index) => {
                    let node_key = self.components[index].children[child_index]
                        .node_key
                        .clone();
                    let (pad_left, pad_top, _, _) =
                        self.components[index].children[child_index].content_padding;
                    self.components[index]
                        .component
                        .child_surface_debug_tree(&node_key, (pad_left as f32, pad_top as f32))
                }
            };
            if let Some(tree) = debug_tree {
                let buffer = self.components[index]
                    .target_mut(target)
                    .paint_buffer
                    .as_mut()
                    .expect("paint buffer initialised");
                self.debug_overlay.paint_layout_bounds(&tree, buffer, scale);
                present_damage = vec![DamageRect {
                    x: 0,
                    y: 0,
                    width: width.max(1),
                    height: height.max(1),
                }];
            }
        }
        if visible
            && let Some(element) = self.debug.inspected_element.as_ref()
            && element.get("surface_id").and_then(|value| value.as_str())
                == Some(surface_id.as_str())
            && let Some(bounds) = element.get("bounds")
        {
            let number = |name: &str| {
                bounds
                    .get(name)
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.0) as f32
            };
            let buffer = self.components[index]
                .target_mut(target)
                .paint_buffer
                .as_mut()
                .expect("paint buffer initialised");
            self.debug_overlay.paint_element_highlight(
                buffer,
                scale,
                (number("x"), number("y"), number("width"), number("height")),
            );
            present_damage = vec![DamageRect {
                x: 0,
                y: 0,
                width: width.max(1),
                height: height.max(1),
            }];
        }

        let mut presented = false;
        let present_started = self.profiling_enabled().then(std::time::Instant::now);
        // An empty `present_damage` means paint produced no changed pixels, so
        // skip the present entirely.
        if !visible || !present_damage.is_empty() {
            let present_result = self.presentation_engine.present_with_damage(
                &surface_id,
                self.components[index].component.id(),
                visible,
                self.components[index]
                    .target(target)
                    .paint_buffer
                    .as_ref()
                    .expect("paint buffer initialised"),
                &present_damage,
            );
            if let Some(restore) = hud_restore.take() {
                restore.restore(
                    self.components[index]
                        .target_mut(target)
                        .paint_buffer
                        .as_mut()
                        .expect("paint buffer initialised"),
                );
            }
            match present_result.map_err(ShellRunError::Presentation)? {
                PresentStatus::Presented => presented = true,
                PresentStatus::NotReady => {
                    self.components[index]
                        .target_mut(target)
                        .pending_present_damage = present_damage;
                    self.components[index].component.request_paint();
                }
            }
        }
        let allocation_counters = allocation_started
            .map(|started| mesh_core_debug::allocation::snapshot().saturating_delta(started));
        if let Some(started) = present_started
            && presented
        {
            self.record_surface_profiling_stage(
                &surface_id,
                Some(component_id),
                mesh_core_debug::ProfilingStage::PresentCommit,
                started.elapsed(),
                Some("present"),
            );
        }
        if let Some(started) = total_render_started {
            self.record_surface_profiling_stage(
                &surface_id,
                Some(component_id),
                mesh_core_debug::ProfilingStage::TotalSurfaceRender,
                started.elapsed(),
                Some("rebuild"),
            );
        }
        if let Some(counters) = allocation_counters {
            self.record_surface_allocations(&surface_id, Some(component_id), counters);
        }
        if visible && presented {
            self.record_surface_redraw(&surface_id, Some(component_id), Some("present"));
        }
        Ok(presented)
    }

    fn resolve_dynamic_surface_size(
        &mut self,
        index: usize,
        surface_id: &str,
    ) -> Result<Option<(u32, u32)>, ShellRunError> {
        if let Some(size) = self.presentation_engine.surface_size_if_known(surface_id) {
            self.components[index].parent.known_surface_size = Some(size);
            return Ok(Some(size));
        }
        if let Some(size) = self.components[index].parent.known_surface_size {
            return Ok(Some(size));
        }
        let size = self
            .presentation_engine
            .surface_size(surface_id)
            .map_err(ShellRunError::Presentation)?;
        if let Some(size) = size {
            self.components[index].parent.known_surface_size = Some(size);
        }
        Ok(size)
    }
}
