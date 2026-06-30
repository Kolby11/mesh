use super::super::*;
use crate::shell::types::{ChildSurface, ChildSurfaceKind, SurfaceTarget};
use mesh_core_elements::style::BackgroundPaint;
use mesh_core_elements::{PopoverAnchor, PopoverConstraintAdjustment, PopoverGrab, PopoverGravity};
use mesh_core_presentation::{
    LayerSurfaceSizePolicy, PopupAnchor, PopupConfig, PopupConstraint, PopupGravity, PopupPlacement,
};
use mesh_core_render::{DamageRect, DisplayPaintCommand};
use std::collections::HashSet;

impl Shell {
    pub(in crate::shell) fn render_components(&mut self) -> Result<(), ShellRunError> {
        self.drain_dismissed_popups()?;

        if self.debug.enabled {
            let mut debug_requests = self.publish_debug_snapshot()?;
            self.drain_requests(&mut debug_requests)?;
        }

        let mut components_want_render_after_frame = false;
        let mut any_component_presented = false;
        for index in 0..self.components.len() {
            let surface_id = self.components[index].surface_id.clone();
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
            let total_render_started = self.profiling_enabled().then(std::time::Instant::now);
            let profiling_enabled = self.profiling_enabled();
            let mut rerender_attempts = 0;
            let mut component_stage_records = Vec::new();
            let component_id = self.components[index].component.id().to_string();
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
                let scale = self.presentation_engine.surface_scale(&surface_id);
                (width, height, scale)
            };
            let mut width = width;
            let mut height = height;
            let mut scale = scale;
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

                // Compare all copy fields before cloning namespace (the only heap field).
                let size_policy = self.components[index].parent.surface_size_policy;
                let layer = surface.layer.unwrap_or(Layer::Top);
                let config_changed = self.components[index]
                    .parent
                    .last_surface_config
                    .as_ref()
                    .map_or(true, |last| {
                        last.edge != surface.edge
                            || last.layer != layer
                            || last.size_policy != size_policy
                            || last.width != surface.width
                            || last.height != surface.height
                            || last.exclusive_zone != surface.exclusive_zone
                            || last.keyboard_mode != surface.keyboard_mode
                            || last.margin_top != surface.margin_top
                            || last.margin_right != surface.margin_right
                            || last.margin_bottom != surface.margin_bottom
                            || last.margin_left != surface.margin_left
                    });
                if config_changed && !is_popup {
                    let cfg = LayerSurfaceConfig {
                        edge: surface.edge,
                        layer,
                        size_policy,
                        width: surface.width,
                        height: surface.height,
                        exclusive_zone: surface.exclusive_zone,
                        keyboard_mode: surface.keyboard_mode,
                        namespace: surface_id.clone(),
                        margin_top: surface.margin_top,
                        margin_right: surface.margin_right,
                        margin_bottom: surface.margin_bottom,
                        margin_left: surface.margin_left,
                    };
                    self.presentation_engine.configure(&surface_id, cfg.clone());
                    self.components[index].parent.last_surface_config = Some(cfg);
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
                    width = if inner_requested_width == 0 {
                        dynamic_size.map(|(w, _)| w).unwrap_or(1)
                    } else {
                        inner_requested_width.max(1)
                    };
                    height = if inner_requested_height == 0 {
                        dynamic_size.map(|(_, h)| h).unwrap_or(1)
                    } else {
                        inner_requested_height.max(1)
                    };
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
                let physical_w = ((width as f32 * scale).ceil() as u32).max(1);
                let physical_h = ((height as f32 * scale).ceil() as u32).max(1);

                // Buffer size cap (T-102-05): prevent allocation beyond 512 MB
                const MAX_BUFFER_BYTES: u64 = 512 * 1024 * 1024;
                let requested_bytes = (physical_w as u64) * (physical_h as u64) * 4;
                if requested_bytes > MAX_BUFFER_BYTES {
                    return Err(ShellRunError::BufferAlloc {
                        surface_id: surface_id.clone(),
                        logical_w: width,
                        logical_h: height,
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
                }
                runtime
                    .component
                    .paint(
                        self.theme.active(),
                        width,
                        height,
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
                // surface-config-only change).
                if (!self.components[index].component.wants_immediate_rerender()
                    && !defer_popup_create)
                    || rerender_attempts >= 1
                {
                    break;
                }

                rerender_attempts += 1;
            }

            // Component(VM)-level profiling + invalidation are recorded once,
            // regardless of how many surface targets the component drives.
            for record in component_stage_records {
                let module_id = record
                    .module_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                    .or(Some(component_id.as_str()));
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
                self.record_surface_invalidation(
                    &surface_id,
                    Some(component_id.as_str()),
                    invalidation,
                );
            }

            // Present the component's parent surface. Child popup targets paint
            // their own subtree and are presented separately during reconcile.
            let presented = self.present_surface_target(
                index,
                TargetRef::Parent,
                &component_id,
                width,
                height,
                scale,
                total_render_started,
            )?;
            any_component_presented |= presented;
            if presented {
                components_want_render_after_frame |=
                    self.components[index].component.wants_render();
            }

            let child_presented = self.reconcile_child_surface_requests(
                index,
                &component_id,
                &surface_id,
                scale,
                total_render_started,
            )?;
            any_component_presented |= child_presented;
            if child_presented {
                components_want_render_after_frame |=
                    self.components[index].component.wants_render();
            }
        }
        self.components_want_render = components_want_render_after_frame;
        self.presented_last_frame = any_component_presented;
        Ok(())
    }

    fn reconcile_child_surface_requests(
        &mut self,
        index: usize,
        component_id: &str,
        parent_surface_id: &str,
        parent_scale: f32,
        total_render_started: Option<std::time::Instant>,
    ) -> Result<bool, ShellRunError> {
        let requests = self.components[index].component.child_surface_requests();
        let requested_keys: HashSet<String> = requests
            .iter()
            .map(|request| request.node_key.clone())
            .collect();
        self.components[index]
            .dismissed_child_node_keys
            .retain(|node_key| requested_keys.contains(node_key));
        if requests.is_empty() || !self.presentation_engine.popup_supported() {
            self.destroy_all_child_surfaces(index);
            return Ok(false);
        }

        let mut child_index = 0;
        while child_index < self.components[index].children.len() {
            if requested_keys.contains(
                self.components[index].children[child_index]
                    .node_key
                    .as_str(),
            ) {
                child_index += 1;
            } else {
                self.destroy_child_surface_at(index, child_index);
            }
        }

        let mut any_presented = false;
        for request in requests {
            if !matches!(request.kind, ChildSurfaceKind::Popover) {
                continue;
            }
            if self.components[index]
                .dismissed_child_node_keys
                .contains(&request.node_key)
            {
                continue;
            }
            let child_surface_id = child_surface_id(parent_surface_id, &request.node_key);
            let child_ref = if let Some(existing) = self.components[index]
                .children
                .iter()
                .position(|child| child.node_key == request.node_key)
            {
                TargetRef::Child(existing)
            } else {
                let mut target =
                    SurfaceTarget::new(child_surface_id.clone(), LayerSurfaceSizePolicy::Flexible);
                target.popup_parent_surface = Some(parent_surface_id.to_string());
                target.force_full_present = true;
                self.components[index].children.push(ChildSurface {
                    target,
                    node_key: request.node_key.clone(),
                    anchor_rect: request.anchor_rect,
                });
                self.rebuild_component_surface_index();
                TargetRef::Child(self.components[index].children.len() - 1)
            };

            let TargetRef::Child(child_index) = child_ref else {
                unreachable!("child reconcile only creates child targets");
            };
            self.components[index].children[child_index].anchor_rect = request.anchor_rect;
            if self
                .presentation_engine
                .surface_waiting_for_frame_callback(&child_surface_id)
            {
                continue;
            }

            self.core
                .surfaces
                .entry(child_surface_id.clone())
                .and_modify(|state| {
                    state.visible = true;
                    state.closing_until = None;
                })
                .or_insert(SurfaceState {
                    visible: true,
                    closing_until: None,
                });
            let surface = self.surfaces.entry(child_surface_id.clone()).or_default();
            surface.visible = true;
            surface.width = request.content_size.0.max(1);
            surface.height = request.content_size.1.max(1);

            let popup_config = PopupConfig {
                parent_surface_id: parent_surface_id.to_string(),
                placement: PopupPlacement {
                    anchor_rect: request.anchor_rect,
                    size: request.content_size,
                    anchor: map_popover_anchor(request.placement.anchor),
                    gravity: map_popover_gravity(request.placement.gravity),
                    constraint: map_popover_constraint(request.placement.constraint_adjustment),
                    offset: (request.placement.offset_x, request.placement.offset_y),
                },
                grab: request.placement.grab == PopoverGrab::Click,
                grab_serial: None,
            };

            {
                let child = &mut self.components[index].children[child_index];
                child.target.popup_config = Some(popup_config.clone());
                child.target.known_surface_size = Some(request.content_size);
                if child.target.last_popup_size != Some(request.content_size) {
                    child.target.last_popup_size = Some(request.content_size);
                }
            }
            if let Err(error) = self
                .presentation_engine
                .configure_popup(&child_surface_id, popup_config)
            {
                tracing::warn!("configure_popup for child {child_surface_id} failed: {error}");
                self.destroy_child_surface_at(index, child_index);
                continue;
            }

            let scale = self.presentation_engine.surface_scale(&child_surface_id);
            let scale = if scale > 0.0 { scale } else { parent_scale };
            let width = request.content_size.0.max(1);
            let height = request.content_size.1.max(1);
            let physical_w = ((width as f32 * scale).ceil() as u32).max(1);
            let physical_h = ((height as f32 * scale).ceil() as u32).max(1);

            const MAX_BUFFER_BYTES: u64 = 512 * 1024 * 1024;
            let requested_bytes = (physical_w as u64) * (physical_h as u64) * 4;
            if requested_bytes > MAX_BUFFER_BYTES {
                return Err(ShellRunError::BufferAlloc {
                    surface_id: child_surface_id,
                    logical_w: width,
                    logical_h: height,
                    physical_w,
                    physical_h,
                    scale,
                    requested_bytes,
                    max_bytes: MAX_BUFFER_BYTES,
                });
            }

            {
                let child = &mut self.components[index].children[child_index];
                if child
                    .target
                    .paint_buffer
                    .as_ref()
                    .map(|buffer| buffer.width != physical_w || buffer.height != physical_h)
                    .unwrap_or(true)
                {
                    child.target.paint_buffer = Some(PixelBuffer::new(physical_w, physical_h));
                }
            }

            let painted = {
                let runtime = &mut self.components[index];
                let buffer = runtime.children[child_index]
                    .target
                    .paint_buffer
                    .as_mut()
                    .expect("child paint buffer initialised");
                runtime
                    .component
                    .paint_child_surface(&request.node_key, buffer, scale)
                    .map_err(ShellRunError::Component)?
            };
            if !painted {
                self.destroy_child_surface_at(index, child_index);
                continue;
            }
            self.components[index].children[child_index]
                .target
                .force_full_present = true;
            self.presentation_engine.update_input_region(
                &self.components[index].children[child_index]
                    .target
                    .surface_id,
                Some(DamageRect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                }),
            );
            let presented = self.present_surface_target(
                index,
                TargetRef::Child(child_index),
                component_id,
                width,
                height,
                scale,
                total_render_started,
            )?;
            any_presented |= presented;
        }

        Ok(any_presented)
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
    ) -> Result<bool, ShellRunError> {
        let surface_id = self.components[index].target(target).surface_id.clone();
        let visible = self.surface_is_effectively_visible(&surface_id);
        let is_parent = matches!(target, TargetRef::Parent);

        if visible && is_parent {
            let commands = self.components[index]
                .component
                .display_list_paint_commands();
            if let Some((surface_w, surface_h)) =
                self.components[index].target(target).known_surface_size
            {
                let opaque_rect = compute_opaque_rect_for_root(commands, surface_w, surface_h);
                self.presentation_engine
                    .update_opaque_region(&surface_id, opaque_rect);
            }
            // The surface buffer is padded (TOOLTIP_OVERLAY_*) so tooltips can render
            // outside the content box, but `known_surface_size` reflects that inflated
            // size. Restrict pointer input to the component's true content rect so
            // clicks over the padding pass through to the windows beneath instead of
            // hitting a dead zone below the bar.
            if let Some((content_w, content_h)) =
                self.components[index].component.content_input_size()
            {
                self.presentation_engine.update_input_region(
                    &surface_id,
                    Some(DamageRect {
                        x: 0,
                        y: 0,
                        width: content_w,
                        height: content_h,
                    }),
                );
            }
            // Compute and set blur region from display list backdrop-filter nodes
            let blur_region = compute_blur_region(commands);
            self.presentation_engine
                .update_blur_region(&surface_id, blur_region);
        }

        let mut present_damage: Vec<DamageRect> = if is_parent {
            self.components[index].component.take_present_damage()
        } else {
            Vec::new()
        };
        // Scale change or explicit force-full triggers full-buffer present (per HDPI-04)
        let mut force_full = false;
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
        if visible && self.debug.show_layout_bounds {
            let debug_tree = match target {
                TargetRef::Parent => self.components[index].component.last_widget_tree().cloned(),
                TargetRef::Child(child_index) => {
                    let node_key = self.components[index].children[child_index]
                        .node_key
                        .clone();
                    self.components[index]
                        .component
                        .child_surface_debug_tree(&node_key)
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

        let mut presented = false;
        let present_started = self.profiling_enabled().then(std::time::Instant::now);
        // An empty `present_damage` Vec means paint produced no changed pixels,
        // so skip the present entirely. This mirrors the old `is_some()` gate
        // (None -> skip) but works with the multi-rect type.
        if !visible || !present_damage.is_empty() {
            self.presentation_engine
                .present_with_damage(
                    &surface_id,
                    self.components[index].component.id(),
                    visible,
                    self.components[index]
                        .target(target)
                        .paint_buffer
                        .as_ref()
                        .expect("paint buffer initialised"),
                    &present_damage,
                )
                .map_err(ShellRunError::Presentation)?;
            presented = true;
        }
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

fn child_surface_id(parent_surface_id: &str, node_key: &str) -> String {
    let mut encoded = String::with_capacity(node_key.len() * 2);
    for byte in node_key.as_bytes() {
        use std::fmt::Write;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    format!("{parent_surface_id}::child::{encoded}")
}

fn map_popover_anchor(anchor: PopoverAnchor) -> PopupAnchor {
    match anchor {
        PopoverAnchor::Center => PopupAnchor::Center,
        PopoverAnchor::Top => PopupAnchor::Top,
        PopoverAnchor::Bottom => PopupAnchor::Bottom,
        PopoverAnchor::Left => PopupAnchor::Left,
        PopoverAnchor::Right => PopupAnchor::Right,
        PopoverAnchor::TopLeft => PopupAnchor::TopLeft,
        PopoverAnchor::TopRight => PopupAnchor::TopRight,
        PopoverAnchor::BottomLeft => PopupAnchor::BottomLeft,
        PopoverAnchor::BottomRight => PopupAnchor::BottomRight,
    }
}

fn map_popover_gravity(gravity: PopoverGravity) -> PopupGravity {
    match gravity {
        PopoverGravity::Center => PopupGravity::Center,
        PopoverGravity::Top => PopupGravity::Top,
        PopoverGravity::Bottom => PopupGravity::Bottom,
        PopoverGravity::Left => PopupGravity::Left,
        PopoverGravity::Right => PopupGravity::Right,
        PopoverGravity::TopLeft => PopupGravity::TopLeft,
        PopoverGravity::TopRight => PopupGravity::TopRight,
        PopoverGravity::BottomLeft => PopupGravity::BottomLeft,
        PopoverGravity::BottomRight => PopupGravity::BottomRight,
    }
}

fn map_popover_constraint(adjustment: PopoverConstraintAdjustment) -> PopupConstraint {
    PopupConstraint {
        flip_x: adjustment.flip_x,
        flip_y: adjustment.flip_y,
        slide_x: adjustment.slide_x,
        slide_y: adjustment.slide_y,
        resize_x: adjustment.resize_x,
        resize_y: adjustment.resize_y,
    }
}

/// Compute the logical-coordinate union rect of all display list nodes
/// that have an active `backdrop-filter: blur(...)`.
///
/// Returns `None` when no nodes have `backdrop_filter.blur_radius > 0.0`,
/// which means no `kde_blur` protocol calls are emitted (BLUR-04).
fn compute_blur_region(commands: &[DisplayPaintCommand]) -> Option<DamageRect> {
    let mut union: Option<DamageRect> = None;
    for cmd in commands {
        if cmd.node.style.backdrop_filter.blur_radius <= 0.0 {
            continue;
        }
        // Clamp negative origins to 0 and shrink dimensions by the clipped leading
        // edge to avoid silently snapping partially off-screen nodes to (0,0) (CR-02).
        let raw_x = cmd.node.layout.x;
        let raw_y = cmd.node.layout.y;
        let x = raw_x.max(0.0) as u32;
        let y = raw_y.max(0.0) as u32;
        let width = ((cmd.node.layout.width + raw_x.min(0.0)).max(0.0) as u32).max(1);
        let height = ((cmd.node.layout.height + raw_y.min(0.0)).max(0.0) as u32).max(1);
        let rect = DamageRect {
            x,
            y,
            width,
            height,
        };
        union = Some(match union {
            None => rect,
            Some(current) => {
                let left = current.x.min(rect.x);
                let top = current.y.min(rect.y);
                let right = current
                    .x
                    .saturating_add(current.width)
                    .max(rect.x.saturating_add(rect.width));
                let bottom = current
                    .y
                    .saturating_add(current.height)
                    .max(rect.y.saturating_add(rect.height));
                DamageRect {
                    x: left,
                    y: top,
                    width: right.saturating_sub(left),
                    height: bottom.saturating_sub(top),
                }
            }
        });
    }
    union
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::{
        BoxShadow, LayoutRect, VisualFilter,
        style::{BackgroundPaint, Color, Edges, Overflow, TextAlign, TextDirection, TextOverflow},
    };
    use mesh_core_render::{
        DamageRect, DisplayListClip, DisplayPaintCommand, DisplayPaintCommandKind,
        display_list::DisplayPaintNode,
    };
    use std::sync::Arc;

    fn make_cmd(x: f32, y: f32, width: f32, height: f32, blur_radius: f32) -> DisplayPaintCommand {
        use mesh_core_render::display_list::{
            DisplayPaintContent, DisplayPaintStyle, DisplayScrollbars,
        };
        DisplayPaintCommand {
            node: DisplayPaintNode {
                id: 1,
                layout: LayoutRect {
                    x,
                    y,
                    width,
                    height,
                },
                style: DisplayPaintStyle {
                    background_color: Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0,
                    },
                    background_paint: BackgroundPaint::None,
                    border_color: Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0,
                    },
                    border_width: Edges::zero(),
                    border_radius: 0.0,
                    color: Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                    padding: Edges::zero(),
                    overflow_x: Overflow::Visible,
                    overflow_y: Overflow::Visible,
                    font_family: Arc::from(""),
                    font_size: 16.0,
                    font_weight: 400,
                    line_height: 1.0,
                    text_align: TextAlign::Left,
                    text_overflow: TextOverflow::Clip,
                    text_direction: TextDirection::default(),
                    opacity: 1.0,
                    box_shadow: BoxShadow::default(),
                    filter: VisualFilter::NONE,
                    backdrop_filter: VisualFilter { blur_radius },
                    mix_blend_mode: mesh_core_elements::BlendMode::Normal,
                    icon_fill: None,
                    icon_weight: None,
                    icon_grade: None,
                    icon_optical_size: None,
                },
                content: DisplayPaintContent::None,
                scrollbars: DisplayScrollbars::default(),
            },
            clip: DisplayListClip {
                x: 0,
                y: 0,
                width: width as i32,
                height: height as i32,
            },
            kind: DisplayPaintCommandKind::Node,
        }
    }

    #[test]
    fn test_compute_blur_region_single_node() {
        let cmds = vec![make_cmd(10.0, 20.0, 100.0, 50.0, 4.0)];
        assert_eq!(
            compute_blur_region(&cmds),
            Some(DamageRect {
                x: 10,
                y: 20,
                width: 100,
                height: 50
            })
        );
    }

    #[test]
    fn test_compute_blur_region_no_blur_nodes() {
        let cmds = vec![make_cmd(0.0, 0.0, 100.0, 100.0, 0.0)];
        assert_eq!(compute_blur_region(&cmds), None);
    }

    #[test]
    fn test_compute_blur_region_negative_coords() {
        // x=-10, y=-5, w=100, h=80 → x=0, y=0, width=90, height=75
        let cmds = vec![make_cmd(-10.0, -5.0, 100.0, 80.0, 4.0)];
        assert_eq!(
            compute_blur_region(&cmds),
            Some(DamageRect {
                x: 0,
                y: 0,
                width: 90,
                height: 75
            })
        );
    }

    #[test]
    fn test_compute_blur_region_two_disjoint_nodes() {
        // (0,0,50,50) union (100,100,50,50) → (0,0,150,150)
        let cmds = vec![
            make_cmd(0.0, 0.0, 50.0, 50.0, 4.0),
            make_cmd(100.0, 100.0, 50.0, 50.0, 4.0),
        ];
        assert_eq!(
            compute_blur_region(&cmds),
            Some(DamageRect {
                x: 0,
                y: 0,
                width: 150,
                height: 150
            })
        );
    }
}

fn compute_opaque_rect_for_root(
    commands: &[DisplayPaintCommand],
    surface_width: u32,
    surface_height: u32,
) -> Option<DamageRect> {
    let root = commands.first()?;
    let style = &root.node.style;

    if style.background_color.a != 255 {
        return None;
    }
    if style.background_paint != BackgroundPaint::None {
        return None;
    }
    if style.border_radius > 0.0 {
        return None;
    }
    if !style.overflow_x.clips_contents() || !style.overflow_y.clips_contents() {
        return None;
    }

    Some(DamageRect {
        x: 0,
        y: 0,
        width: surface_width.max(1),
        height: surface_height.max(1),
    })
}
