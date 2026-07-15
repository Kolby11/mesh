use super::super::*;
use crate::shell::types::{ChildSurface, ChildSurfaceKind, SurfaceTarget};
use mesh_core_elements::style::BackgroundPaint;
use mesh_core_elements::{PopoverAnchor, PopoverConstraintAdjustment, PopoverGrab, PopoverGravity};
use mesh_core_presentation::{
    LayerSurfaceSizePolicy, PopupAnchor, PopupConfig, PopupConstraint, PopupGravity, PopupPlacement,
};
use mesh_core_render::{DamageRect, DisplayPaintCommand};
use smallvec::SmallVec;

const DEBUG_INSPECTOR_SURFACE_ID: &str = "@mesh/debug-inspector";

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
            if !self.components[index].component.wants_render() {
                continue;
            }
            let surface_id = self.components[index].surface_id.clone();
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
                // component sees) stay content-sized; pointer input is
                // confined back to content in `present_surface_target`.
                let (tooltip_extra_w, tooltip_extra_h) =
                    tooltip_overlay_extra_for_surface(&surface_id, surface.width, surface.height);
                let configured_width = surface.width.saturating_add(tooltip_extra_w);
                let configured_height = surface.height.saturating_add(tooltip_extra_h);
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
                        width: configured_width,
                        height: configured_height,
                        exclusive_zone: surface.exclusive_zone,
                        keyboard_mode: surface.keyboard_mode,
                        namespace: surface_id.clone(),
                        margin_top: surface.margin_top,
                        margin_right: surface.margin_right,
                        margin_bottom: surface.margin_bottom,
                        margin_left: surface.margin_left,
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
                // component-facing notifications and popup config above.
                (paint_width, paint_height) = if is_popup {
                    (width, height)
                } else {
                    let (extra_w, extra_h) =
                        tooltip_overlay_extra_for_surface(&surface_id, width, height);
                    (
                        width.saturating_add(extra_w),
                        height.saturating_add(extra_h),
                    )
                };
                let physical_w = ((paint_width as f32 * scale).ceil() as u32).max(1);
                let physical_h = ((paint_height as f32 * scale).ceil() as u32).max(1);

                // Buffer size cap (T-102-05): prevent allocation beyond 512 MB
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
                        paint_width,
                        paint_height,
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
        self.service_delivery_index.mark_dirty();
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
        if requests.is_empty() && self.components[index].children.is_empty() {
            // Common no-popup frame: avoid allocating key sets and running
            // reconciliation bookkeeping. A dismissed key only suppresses
            // recreation until its authored request disappears for one frame.
            self.components[index].dismissed_child_node_keys.clear();
            self.components[index].entering_child_node_keys.clear();
            self.components[index]
                .component
                .set_entering_child_keys_from_slice(&[]);
            return Ok(false);
        }
        let requested_keys: SmallVec<[&str; 4]> = requests
            .iter()
            .map(|request| request.node_key.as_str())
            .collect();
        self.components[index]
            .entering_child_node_keys
            .retain(|node_key| requested_keys.contains(&node_key.as_str()));
        self.components[index]
            .dismissed_child_node_keys
            .retain(|node_key| requested_keys.contains(&node_key.as_str()));
        if !self.presentation_engine.popup_supported() {
            self.destroy_all_child_surfaces(index);
            self.components[index].entering_child_node_keys.clear();
            self.components[index]
                .component
                .set_closing_child_keys_from_slice(&[]);
            self.components[index]
                .component
                .set_entering_child_keys_from_slice(&[]);
            return Ok(false);
        }

        let now = std::time::Instant::now();
        let mut child_index = 0;
        while child_index < self.components[index].children.len() {
            if requested_keys.contains(
                &self.components[index].children[child_index]
                    .node_key
                    .as_str(),
            ) {
                self.components[index].children[child_index].closing_until = None;
                child_index += 1;
                continue;
            }
            let closing_until = self.components[index].children[child_index].closing_until;
            match closing_until {
                Some(until) if until > now => {
                    // Still playing its exit transition; keep the surface
                    // alive and let the closing-repaint pass below animate it.
                    child_index += 1;
                }
                Some(_) => {
                    // Grace period elapsed.
                    self.destroy_child_surface_at(index, child_index);
                }
                None => {
                    let duration = {
                        let node_key = self.components[index].children[child_index]
                            .node_key
                            .as_str();
                        self.components[index]
                            .component
                            .child_hide_transition_ms(node_key)
                    };
                    if duration == 0 {
                        self.destroy_child_surface_at(index, child_index);
                    } else {
                        self.components[index].children[child_index].closing_until =
                            Some(now + std::time::Duration::from_millis(duration));
                        child_index += 1;
                    }
                }
            }
        }

        {
            let runtime = &mut self.components[index];
            let closing_keys: SmallVec<[&str; 4]> = runtime
                .children
                .iter()
                .filter(|child| child.closing_until.is_some())
                .map(|child| child.node_key.as_str())
                .collect();
            runtime
                .component
                .set_closing_child_keys_from_slice(&closing_keys);
        }

        let mut any_presented = false;
        for request in &requests {
            if !matches!(request.kind, ChildSurfaceKind::Popover) {
                continue;
            }
            if self.components[index]
                .dismissed_child_node_keys
                .contains(&request.node_key)
            {
                continue;
            }
            let existing_child = self.components[index]
                .children
                .iter()
                .position(|child| child.node_key == request.node_key);
            if existing_child.is_none()
                && !self.components[index]
                    .entering_child_node_keys
                    .contains(&request.node_key)
            {
                self.components[index]
                    .entering_child_node_keys
                    .insert(request.node_key.clone());
                let entering = self.components[index].entering_child_node_keys.clone();
                self.components[index]
                    .component
                    .set_entering_child_keys(entering);
                // Defer mapping until the component has rebuilt this subtree
                // with mesh-surface-entering applied. Otherwise the compositor
                // exposes one resting frame and there is nothing to animate.
                continue;
            }
            let child_surface_id = existing_child
                .map(|existing| {
                    self.components[index].children[existing]
                        .target
                        .surface_id
                        .clone()
                })
                .unwrap_or_else(|| child_surface_id(parent_surface_id, &request.node_key));
            let child_ref = if let Some(existing) = existing_child {
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
                    content_padding: request.content_padding,
                    closing_until: None,
                    last_paint_generation: None,
                    last_paint_exiting: None,
                    last_paint_scale_bits: None,
                    last_paint_content_offset: None,
                    pending_present_damage: Vec::new(),
                });
                self.rebuild_component_surface_index();
                TargetRef::Child(self.components[index].children.len() - 1)
            };

            let TargetRef::Child(child_index) = child_ref else {
                unreachable!("child reconcile only creates child targets");
            };
            self.components[index].children[child_index].anchor_rect = request.anchor_rect;
            self.components[index].children[child_index].content_padding = request.content_padding;
            if self
                .presentation_engine
                .surface_waiting_for_frame_callback(&child_surface_id)
            {
                continue;
            }

            // The buffer is padded (pad_left/top/right/bottom) beyond the
            // measured popover content so descendant `box-shadow`/`filter`
            // overshoot has pixels to paint into instead of clipping at the
            // buffer edge; the positioner offset is adjusted so the *visible*
            // content — not the padded buffer — lands where it would with a
            // zero-padding buffer. The correction depends on how the
            // positioner anchors this axis: an edge-pinned anchor needs the
            // padding on that edge subtracted back out, while a center-based
            // anchor already centers the padded buffer (which centers the
            // visible content too when padding is symmetric).
            let (pad_left, pad_top, pad_right, pad_bottom) = request.content_padding;
            let padded_size = (
                request.content_size.0 + pad_left + pad_right,
                request.content_size.1 + pad_top + pad_bottom,
            );
            let offset_x = request.placement.offset_x
                + axis_padding_compensation(
                    popover_gravity_horizontal_alignment(request.placement.gravity),
                    pad_left,
                    pad_right,
                );
            let offset_y = request.placement.offset_y
                + axis_padding_compensation(
                    popover_gravity_vertical_alignment(request.placement.gravity),
                    pad_top,
                    pad_bottom,
                );

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
            surface.width = padded_size.0.max(1);
            surface.height = padded_size.1.max(1);

            let popup_config = PopupConfig {
                parent_surface_id: parent_surface_id.to_string(),
                placement: PopupPlacement {
                    anchor_rect: request.anchor_rect,
                    size: padded_size,
                    anchor: map_popover_anchor(request.placement.anchor),
                    gravity: map_popover_gravity(request.placement.gravity),
                    constraint: map_popover_constraint(request.placement.constraint_adjustment),
                    offset: (offset_x, offset_y),
                },
                grab: request.placement.grab == PopoverGrab::Click,
                grab_serial: None,
            };

            let popup_config_changed = {
                let child = &mut self.components[index].children[child_index];
                let changed = child.target.popup_config.as_ref() != Some(&popup_config);
                if changed {
                    child.target.popup_config = Some(popup_config.clone());
                }
                child.target.known_surface_size = Some(padded_size);
                if child.target.last_popup_size != Some(padded_size) {
                    child.target.last_popup_size = Some(padded_size);
                }
                changed
            };
            if popup_config_changed
                && let Err(error) = self
                    .presentation_engine
                    .configure_popup(&child_surface_id, popup_config)
            {
                tracing::warn!("configure_popup for child {child_surface_id} failed: {error}");
                self.destroy_child_surface_at(index, child_index);
                continue;
            }

            let width = padded_size.0.max(1);
            let height = padded_size.1.max(1);
            let presented = self.paint_and_present_child_surface(
                index,
                child_index,
                component_id,
                width,
                height,
                parent_scale,
                total_render_started,
                false,
            )?;
            any_presented |= presented;

            if self.components[index]
                .entering_child_node_keys
                .remove(&request.node_key)
            {
                let entering = self.components[index].entering_child_node_keys.clone();
                self.components[index]
                    .component
                    .set_entering_child_keys(entering);
            }
        }

        // Popovers whose node dropped out of the open requests this frame but
        // still have exit-transition time left: keep painting/presenting them
        // with the exiting class applied so their CSS exit animation runs
        // before `destroy_child_surface_at` tears the popup down above.
        let closing_indices: Vec<usize> = self.components[index]
            .children
            .iter()
            .enumerate()
            .filter(|(_, child)| {
                child.closing_until.is_some() && !requested_keys.contains(&child.node_key.as_str())
            })
            .map(|(child_index, _)| child_index)
            .collect();
        for child_index in closing_indices {
            let (width, height) = self.components[index].children[child_index]
                .target
                .known_surface_size
                .unwrap_or((1, 1));
            let presented = self.paint_and_present_child_surface(
                index,
                child_index,
                component_id,
                width,
                height,
                parent_scale,
                total_render_started,
                true,
            )?;
            any_presented |= presented;
        }

        Ok(any_presented)
    }

    /// Shared paint+present tail for a child popup surface, used both for
    /// actively open popovers and for ones playing their exit transition
    /// (`exiting = true` appends `mesh-surface-exiting` to the painted
    /// subtree so its CSS transition animates before teardown).
    #[allow(clippy::too_many_arguments)]
    fn paint_and_present_child_surface(
        &mut self,
        index: usize,
        child_index: usize,
        component_id: &str,
        width: u32,
        height: u32,
        parent_scale: f32,
        total_render_started: Option<std::time::Instant>,
        exiting: bool,
    ) -> Result<bool, ShellRunError> {
        let child_surface_id = self.components[index].children[child_index]
            .target
            .surface_id
            .clone();
        let node_key = self.components[index].children[child_index]
            .node_key
            .clone();

        let scale = self.presentation_engine.surface_scale(&child_surface_id);
        let scale = if scale > 0.0 { scale } else { parent_scale };
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
                child.last_paint_generation = None;
                child.last_paint_exiting = None;
                child.last_paint_scale_bits = None;
                child.last_paint_content_offset = None;
            }
        }

        let (pad_left, pad_top, pad_right, pad_bottom) =
            self.components[index].children[child_index].content_padding;
        // `paint_child_surface`'s offset is in the same logical layout units
        // as `-bounds.0`/`-bounds.1` (the renderer applies `scale` to layout
        // + offset together), so this is unscaled padding, not physical px.
        let content_offset = (pad_left, pad_top);
        let paint_generation = self.components[index]
            .component
            .child_surface_paint_generation(&node_key);
        if child_surface_paint_cache_matches(
            paint_generation,
            self.components[index].children[child_index].last_paint_generation,
            exiting,
            self.components[index].children[child_index].last_paint_exiting,
            scale.to_bits(),
            self.components[index].children[child_index].last_paint_scale_bits,
            content_offset,
            self.components[index].children[child_index].last_paint_content_offset,
        ) {
            return Ok(false);
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
                .paint_child_surface(&node_key, buffer, scale, content_offset, exiting)
                .map_err(ShellRunError::Component)?
        };
        if !painted {
            self.destroy_child_surface_at(index, child_index);
            return Ok(false);
        }
        let child_damage = self.components[index]
            .component
            .child_surface_present_damage(&node_key);
        self.components[index].children[child_index].last_paint_generation = paint_generation;
        self.components[index].children[child_index].last_paint_exiting = Some(exiting);
        self.components[index].children[child_index].last_paint_scale_bits = Some(scale.to_bits());
        self.components[index].children[child_index].last_paint_content_offset =
            Some(content_offset);
        let child = &mut self.components[index].children[child_index];
        match child_damage {
            Some(damage) => child.pending_present_damage = damage,
            None => {
                child.pending_present_damage.clear();
                child.target.force_full_present = true;
            }
        }
        // Restrict pointer input to the true (unpadded) content rect, same
        // pattern as the parent tooltip surface: the padding exists so
        // shadow/filter overshoot can paint, not to receive input.
        self.presentation_engine.update_input_region(
            &self.components[index].children[child_index]
                .target
                .surface_id,
            Some(DamageRect {
                x: pad_left,
                y: pad_top,
                width: width.saturating_sub(pad_left + pad_right),
                height: height.saturating_sub(pad_top + pad_bottom),
            }),
        );
        // Frosted popover content declares `backdrop-filter`; hand the region
        // to the compositor blur protocol like the parent surface path does.
        let child_blur_region = self.components[index]
            .component
            .child_surface_blur_region(&node_key);
        self.presentation_engine.update_blur_region(
            &self.components[index].children[child_index]
                .target
                .surface_id,
            child_blur_region,
        );
        match self.present_surface_target(
            index,
            TargetRef::Child(child_index),
            component_id,
            width,
            height,
            scale,
            total_render_started,
        ) {
            Ok(presented) => Ok(presented),
            Err(ShellRunError::Presentation(error)) => {
                tracing::warn!(
                    "presenting child popup {child_surface_id} failed; destroying popup and keeping parent surface alive: {error}"
                );
                self.destroy_child_surface_at(index, child_index);
                Ok(false)
            }
            Err(error) => Err(error),
        }
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

                // Restrict pointer input to true content rather than tooltip padding.
                let input_rect = content_size.map(|(content_w, content_h)| DamageRect {
                    x: 0,
                    y: 0,
                    width: content_w,
                    height: content_h,
                });
                self.presentation_engine
                    .update_input_region(&surface_id, input_rect);

                let blur_region = compute_blur_region(commands);
                self.presentation_engine
                    .update_blur_region(&surface_id, blur_region);
                self.components[index].target_mut(target).last_region_state = Some(region_state);
            }
        }

        let mut present_damage: Vec<DamageRect> = match target {
            TargetRef::Parent => self.components[index].component.take_present_damage(),
            TargetRef::Child(child_index) => std::mem::take(
                &mut self.components[index].children[child_index].pending_present_damage,
            ),
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

fn tooltip_overlay_extra_for_surface(surface_id: &str, width: u32, height: u32) -> (u32, u32) {
    if surface_id == DEBUG_INSPECTOR_SURFACE_ID {
        return (0, 0);
    }
    component::tooltip_overlay_extra_for_content(width, height)
}

fn child_surface_id(parent_surface_id: &str, node_key: &str) -> String {
    let mut encoded = String::with_capacity(node_key.len() * 2);
    for byte in node_key.as_bytes() {
        use std::fmt::Write;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    format!("{parent_surface_id}::child::{encoded}")
}

/// Padding compensation for the popup positioner offset along one axis.
///
/// `xdg_positioner` places the popup buffer so that the edge/corner named by
/// *gravity* touches the anchor point plus `offset`, sized to the full
/// *padded* buffer — gravity "bottom" pins the popup's TOP edge to the anchor
/// point and grows downward, "top" pins the BOTTOM edge and grows upward,
/// "left"/"right" are the mirrored horizontal cases, and no horizontal/
/// vertical gravity component centers that axis on the anchor point. The
/// buffer's visible content sits inset by `(pad_leading, pad_trailing)`. Only
/// an edge-pinned gravity needs the offset shifted back by the padding on
/// that pinned edge so the visible content — not the padded buffer — lands
/// where the caller asked; a center-based gravity already centers the padded
/// buffer, which centers the visible content too when padding is symmetric
/// (and splits the difference otherwise).
fn axis_padding_compensation(alignment: AxisAlignment, pad_leading: u32, pad_trailing: u32) -> i32 {
    match alignment {
        AxisAlignment::Leading => -(pad_leading as i32),
        AxisAlignment::Trailing => pad_trailing as i32,
        AxisAlignment::Center => (pad_leading as i32 - pad_trailing as i32) / 2,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AxisAlignment {
    Leading,
    Center,
    Trailing,
}

/// Gravity "left"/"right" pins the popup's RIGHT/LEFT edge to the anchor
/// point (the popup body extends away from that edge), which is the mirror
/// image of the anchor-side `Leading`/`Trailing` naming — hence the swap here.
fn popover_gravity_horizontal_alignment(gravity: PopoverGravity) -> AxisAlignment {
    match gravity {
        PopoverGravity::Left | PopoverGravity::TopLeft | PopoverGravity::BottomLeft => {
            AxisAlignment::Trailing
        }
        PopoverGravity::Right | PopoverGravity::TopRight | PopoverGravity::BottomRight => {
            AxisAlignment::Leading
        }
        PopoverGravity::Center | PopoverGravity::Top | PopoverGravity::Bottom => {
            AxisAlignment::Center
        }
    }
}

/// Gravity "top"/"bottom" pins the popup's BOTTOM/TOP edge to the anchor
/// point (the popup body extends away from that edge), mirroring the
/// anchor-side naming — hence the swap here.
fn popover_gravity_vertical_alignment(gravity: PopoverGravity) -> AxisAlignment {
    match gravity {
        PopoverGravity::Top | PopoverGravity::TopLeft | PopoverGravity::TopRight => {
            AxisAlignment::Trailing
        }
        PopoverGravity::Bottom | PopoverGravity::BottomLeft | PopoverGravity::BottomRight => {
            AxisAlignment::Leading
        }
        PopoverGravity::Center | PopoverGravity::Left | PopoverGravity::Right => {
            AxisAlignment::Center
        }
    }
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

#[inline]
fn child_surface_paint_cache_matches(
    generation: Option<u64>,
    cached_generation: Option<u64>,
    exiting: bool,
    cached_exiting: Option<bool>,
    scale_bits: u32,
    cached_scale_bits: Option<u32>,
    content_offset: (u32, u32),
    cached_content_offset: Option<(u32, u32)>,
) -> bool {
    generation.is_some()
        && cached_generation == generation
        && cached_exiting == Some(exiting)
        && cached_scale_bits == Some(scale_bits)
        && cached_content_offset == Some(content_offset)
}

#[cfg(test)]
mod performance_tests {
    use super::{child_surface_id, child_surface_paint_cache_matches};
    use mesh_core_elements::style::Color;
    use mesh_core_render::PixelBuffer;
    use smallvec::SmallVec;
    use std::collections::HashSet;
    use std::hint::black_box;
    use std::time::Instant;

    #[test]
    fn child_paint_cache_requires_every_raster_input_to_match() {
        let matches = |generation, exiting, scale, offset| {
            child_surface_paint_cache_matches(
                generation,
                Some(7),
                exiting,
                Some(false),
                scale,
                Some(1.0_f32.to_bits()),
                offset,
                Some((4, 4)),
            )
        };
        assert!(matches(Some(7), false, 1.0_f32.to_bits(), (4, 4)));
        assert!(!matches(None, false, 1.0_f32.to_bits(), (4, 4)));
        assert!(!matches(Some(8), false, 1.0_f32.to_bits(), (4, 4)));
        assert!(!matches(Some(7), true, 1.0_f32.to_bits(), (4, 4)));
        assert!(!matches(Some(7), false, 2.0_f32.to_bits(), (4, 4)));
        assert!(!matches(Some(7), false, 1.0_f32.to_bits(), (8, 4)));
    }

    // cargo test -p mesh-core-shell --release -- cached_child_paint_generation_beats_eager_buffer_clear --ignored --nocapture
    #[test]
    #[ignore = "release-only stable child-surface paint benchmark"]
    fn cached_child_paint_generation_beats_eager_buffer_clear() {
        let iterations = 10_000;
        let mut buffer = PixelBuffer::new(160, 90);

        let eager_started = Instant::now();
        for _ in 0..iterations {
            black_box(&mut buffer).clear(Color::TRANSPARENT);
        }
        let eager_time = eager_started.elapsed();

        let cached_started = Instant::now();
        let mut cache_hits = 0usize;
        for _ in 0..iterations {
            cache_hits += usize::from(child_surface_paint_cache_matches(
                black_box(Some(7)),
                black_box(Some(7)),
                black_box(false),
                black_box(Some(false)),
                black_box(1.0_f32.to_bits()),
                black_box(Some(1.0_f32.to_bits())),
                black_box((4, 4)),
                black_box(Some((4, 4))),
            ));
        }
        let cached_time = cached_started.elapsed();

        eprintln!(
            "stable child paint: eager clear {eager_time:?}; generation cache {cached_time:?}; ratio {:.1}x; hits={cache_hits}",
            eager_time.as_secs_f64() / cached_time.as_secs_f64()
        );
        assert_eq!(cache_hits, iterations);
        assert!(cached_time * 10 < eager_time);
    }

    // cargo test -p mesh-core-shell --release -- cached_child_surface_id_beats_reencoding --ignored --nocapture
    #[test]
    #[ignore = "release-only child-surface id microbenchmark"]
    fn cached_child_surface_id_beats_reencoding() {
        let parent = "@mesh/navigation-bar";
        let key = "root/0/2/1/5/language-popover";
        let cached = child_surface_id(parent, key);
        let iterations = 100_000;

        let encode_started = Instant::now();
        for _ in 0..iterations {
            black_box(child_surface_id(black_box(parent), black_box(key)));
        }
        let encode = encode_started.elapsed();

        let clone_started = Instant::now();
        for _ in 0..iterations {
            black_box(black_box(&cached).clone());
        }
        let clone = clone_started.elapsed();

        eprintln!(
            "child id re-encode: {encode:?}; cached clone: {clone:?}; ratio: {:.1}x",
            encode.as_secs_f64() / clone.as_secs_f64()
        );
        assert!(clone < encode);
    }

    // cargo test -p mesh-core-shell --release -- render_component_id_borrow_beats_extra_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only render component id allocation microbenchmark"]
    fn render_component_id_borrow_beats_extra_clone() {
        let surface_id = "@mesh/navigation-bar".to_string();
        let iterations = 2_000_000;

        let clone_started = Instant::now();
        let mut clone_len = 0usize;
        for _ in 0..iterations {
            let component_id = black_box(&surface_id).clone();
            clone_len += black_box(component_id.len());
        }
        let clone_time = clone_started.elapsed();

        let borrow_started = Instant::now();
        let mut borrow_len = 0usize;
        for _ in 0..iterations {
            let component_id = black_box(&surface_id).as_str();
            borrow_len += black_box(component_id.len());
        }
        let borrow_time = borrow_started.elapsed();

        eprintln!(
            "render component id: extra clone {clone_time:?}; borrow existing surface id {borrow_time:?}; ratio {:.1}x; lens={clone_len}/{borrow_len}",
            clone_time.as_secs_f64() / borrow_time.as_secs_f64()
        );
        assert_eq!(clone_len, borrow_len);
        assert!(borrow_time < clone_time);
    }

    // cargo test -p mesh-core-shell --release -- requested_child_keys_smallvec_beats_hashset_for_popovers --ignored --nocapture
    #[test]
    #[ignore = "release-only child requested-key membership microbenchmark"]
    fn requested_child_keys_smallvec_beats_hashset_for_popovers() {
        let requested = [
            "root/0/language-popover",
            "root/1/theme-popover",
            "root/2/audio-popover",
        ];
        let retained = [
            "root/0/language-popover".to_string(),
            "root/1/theme-popover".to_string(),
            "root/4/stale-popover".to_string(),
        ];
        let iterations = 500_000;

        let hash_started = Instant::now();
        let mut hash_count = 0usize;
        for _ in 0..iterations {
            let requested_keys: HashSet<&str> = requested.iter().copied().collect();
            hash_count += retained
                .iter()
                .filter(|key| requested_keys.contains(key.as_str()))
                .count();
        }
        let hash_time = hash_started.elapsed();

        let small_started = Instant::now();
        let mut small_count = 0usize;
        for _ in 0..iterations {
            let requested_keys: SmallVec<[&str; 4]> = requested.iter().copied().collect();
            small_count += retained
                .iter()
                .filter(|key| requested_keys.contains(&key.as_str()))
                .count();
        }
        let small_time = small_started.elapsed();

        eprintln!(
            "requested child keys: HashSet {hash_time:?}; SmallVec {small_time:?}; ratio {:.1}x; counts={hash_count}/{small_count}",
            hash_time.as_secs_f64() / small_time.as_secs_f64()
        );
        assert_eq!(hash_count, small_count);
        assert!(small_time < hash_time);
    }

    // cargo test -p mesh-core-shell --release -- closing_child_keys_borrowed_compare_beats_owned_hashset --ignored --nocapture
    #[test]
    #[ignore = "release-only child closing-key allocation microbenchmark"]
    fn closing_child_keys_borrowed_compare_beats_owned_hashset() {
        let closing = [
            "root/0/language-popover".to_string(),
            "root/1/theme-popover".to_string(),
            "root/2/audio-popover".to_string(),
        ];
        let existing: HashSet<String> = closing.iter().cloned().collect();
        let iterations = 500_000;

        let hash_started = Instant::now();
        let mut hash_count = 0usize;
        for _ in 0..iterations {
            let candidate: HashSet<String> =
                closing.iter().map(|key| black_box(key).clone()).collect();
            if candidate == existing {
                hash_count = hash_count.wrapping_add(candidate.len());
            }
        }
        let hash_time = hash_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_count = 0usize;
        for _ in 0..iterations {
            let candidate: SmallVec<[&str; 4]> =
                closing.iter().map(|key| black_box(key.as_str())).collect();
            if existing.len() == candidate.len()
                && candidate.iter().all(|key| existing.contains(*key))
            {
                borrowed_count = borrowed_count.wrapping_add(candidate.len());
            }
        }
        let borrowed_time = borrowed_started.elapsed();

        eprintln!(
            "closing child keys: owned HashSet {hash_time:?}; borrowed SmallVec compare {borrowed_time:?}; ratio {:.1}x; counts={hash_count}/{borrowed_count}",
            hash_time.as_secs_f64() / borrowed_time.as_secs_f64()
        );
        assert_eq!(hash_count, borrowed_count);
        assert!(borrowed_time < hash_time);
    }

    // cargo test -p mesh-core-shell --release -- child_reconcile_borrowed_key_check_beats_clone_per_child --ignored --nocapture
    #[test]
    #[ignore = "release-only child reconcile key microbenchmark"]
    fn child_reconcile_borrowed_key_check_beats_clone_per_child() {
        let children: Vec<String> = (0..16)
            .map(|index| format!("root/{index}/popover"))
            .collect();
        let requested: SmallVec<[&str; 4]> = [
            "root/0/popover",
            "root/4/popover",
            "root/8/popover",
            "root/12/popover",
        ]
        .into_iter()
        .collect();
        let iterations = 500_000usize;

        let clone_started = Instant::now();
        let mut clone_count = 0usize;
        for _ in 0..iterations {
            for child in &children {
                let node_key = black_box(child).clone();
                if requested.contains(&node_key.as_str()) {
                    clone_count += 1;
                }
            }
        }
        let clone_time = clone_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_count = 0usize;
        for _ in 0..iterations {
            for child in &children {
                if requested.contains(&black_box(child).as_str()) {
                    borrowed_count += 1;
                }
            }
        }
        let borrowed_time = borrowed_started.elapsed();

        eprintln!(
            "child reconcile key check: clone {clone_time:?}; borrowed {borrowed_time:?}; ratio {:.1}x; counts={clone_count}/{borrowed_count}",
            clone_time.as_secs_f64() / borrowed_time.as_secs_f64()
        );
        assert_eq!(clone_count, borrowed_count);
        assert!(borrowed_time < clone_time);
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
    mesh_core_render::display_list::backdrop_blur_region_union(commands)
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
            node: Arc::new(DisplayPaintNode {
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
            }),
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

    // cargo test -p mesh-core-shell --release -- cached_region_state_beats_command_scan --ignored --nocapture
    #[test]
    #[ignore = "release-only derived-region microbenchmark"]
    fn cached_region_state_beats_command_scan() {
        use std::hint::black_box;
        use std::time::Instant;

        let commands: Vec<_> = (0..500)
            .map(|index| {
                make_cmd(
                    index as f32,
                    index as f32,
                    20.0,
                    20.0,
                    if index % 50 == 0 { 4.0 } else { 0.0 },
                )
            })
            .collect();
        let cached = (7_u64, Some((1920, 1080)), Some((1920, 56)));
        let iterations = 20_000;

        let scan_started = Instant::now();
        for _ in 0..iterations {
            black_box(compute_blur_region(black_box(&commands)));
        }
        let scan = scan_started.elapsed();

        let cache_started = Instant::now();
        for _ in 0..iterations {
            black_box(black_box(Some(cached)) == Some(cached));
        }
        let cache = cache_started.elapsed();

        eprintln!(
            "region command scan: {scan:?}; generation/geometry cache check: {cache:?}; ratio: {:.1}x",
            scan.as_secs_f64() / cache.as_secs_f64()
        );
        assert!(cache < scan);
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
