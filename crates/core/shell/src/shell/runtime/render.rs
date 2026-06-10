use super::super::*;
use mesh_core_elements::style::BackgroundPaint;
use mesh_core_render::{DamageRect, DisplayPaintCommand};

impl Shell {
    pub(in crate::shell) fn render_components(&mut self) -> Result<(), ShellRunError> {
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
            let visible = self
                .core
                .surfaces
                .get(&surface_id)
                .map(|state| state.visible)
                .unwrap_or_else(|| {
                    self.surfaces
                        .get(&surface_id)
                        .map(|surface| surface.visible)
                        .unwrap_or(true)
                });
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
                if self.components[index].known_surface_size != Some(resolved_size) {
                    self.components[index].known_surface_size = Some(resolved_size);
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
                        .paint_buffer
                        .as_ref()
                        .map(|buffer| buffer.width != 1 || buffer.height != 1)
                        .unwrap_or(true)
                    {
                        runtime.paint_buffer = Some(PixelBuffer::new(1, 1));
                    }
                    runtime.known_surface_size = None;
                    runtime.last_surface_config = None;
                    break;
                }

                // Compare all copy fields before cloning namespace (the only heap field).
                let size_policy = self.components[index].surface_size_policy;
                let layer = surface.layer.unwrap_or(Layer::Top);
                let config_changed =
                    self.components[index]
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
                if config_changed {
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
                    self.components[index].last_surface_config = Some(cfg);
                }

                let requested_width = surface.width;
                let requested_height = surface.height;
                let dynamic_size = if requested_width == 0 || requested_height == 0 {
                    self.resolve_dynamic_surface_size(index, &surface_id)?
                } else {
                    None
                };
                let width = if requested_width == 0 {
                    dynamic_size.map(|(width, _)| width).unwrap_or(1)
                } else {
                    requested_width.max(1)
                };
                let height = if requested_height == 0 {
                    dynamic_size.map(|(_, height)| height).unwrap_or(1)
                } else {
                    requested_height.max(1)
                };
                let resolved_size = (width, height);
                if self.components[index].known_surface_size != Some(resolved_size) {
                    self.components[index].known_surface_size = Some(resolved_size);
                    self.components[index]
                        .component
                        .surface_size_changed(width, height);
                }

                let runtime = &mut self.components[index];
                if runtime
                    .paint_buffer
                    .as_ref()
                    .map(|buffer| buffer.width != width || buffer.height != height)
                    .unwrap_or(true)
                {
                    runtime.paint_buffer = Some(PixelBuffer::new(width, height));
                }
                runtime
                    .component
                    .paint(
                        self.theme.active(),
                        width,
                        height,
                        runtime
                            .paint_buffer
                            .as_mut()
                            .expect("paint buffer initialised"),
                    )
                    .map_err(ShellRunError::Component)?;
                component_stage_records.extend(runtime.component.take_profiling_records());

                if !self.components[index].component.wants_immediate_rerender()
                    || rerender_attempts >= 1
                {
                    break;
                }

                rerender_attempts += 1;
            }

            let visible = self
                .core
                .surfaces
                .get(&surface_id)
                .map(|state| state.visible)
                .unwrap_or_else(|| {
                    self.surfaces
                        .get(&surface_id)
                        .map(|surface| surface.visible)
                        .unwrap_or(true)
                });

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

            if visible {
                let commands = self.components[index]
                    .component
                    .display_list_paint_commands();
                if let Some((surface_w, surface_h)) = self.components[index].known_surface_size {
                    let opaque_rect = compute_opaque_rect_for_root(commands, surface_w, surface_h);
                    self.presentation_engine
                        .update_opaque_region(&surface_id, opaque_rect);
                }
            }

            let mut present_damage: Vec<DamageRect> =
                self.components[index].component.take_present_damage();
            if visible && self.components[index].force_full_present {
                if let Some(buffer) = self.components[index].paint_buffer.as_ref() {
                    present_damage = vec![full_buffer_damage(buffer)];
                }
                self.components[index].force_full_present = false;
            }
            if visible && self.debug.show_layout_bounds {
                let runtime = &mut self.components[index];
                if let Some(tree) = runtime.component.last_widget_tree() {
                    let buffer = runtime
                        .paint_buffer
                        .as_mut()
                        .expect("paint buffer initialised");
                    self.debug_overlay.paint_layout_bounds(tree, buffer, 1.0);
                    present_damage = vec![full_buffer_damage(buffer)];
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
                            .paint_buffer
                            .as_ref()
                            .expect("paint buffer initialised"),
                        &present_damage,
                    )
                    .map_err(ShellRunError::Presentation)?;
                presented = true;
                any_component_presented = true;
            }
            if let Some(started) = present_started
                && presented
            {
                self.record_surface_profiling_stage(
                    &surface_id,
                    Some(component_id.as_str()),
                    mesh_core_debug::ProfilingStage::PresentCommit,
                    started.elapsed(),
                    Some("present"),
                );
            }
            if let Some(started) = total_render_started {
                self.record_surface_profiling_stage(
                    &surface_id,
                    Some(component_id.as_str()),
                    mesh_core_debug::ProfilingStage::TotalSurfaceRender,
                    started.elapsed(),
                    Some("rebuild"),
                );
            }
            if visible && presented {
                self.record_surface_redraw(
                    &surface_id,
                    Some(component_id.as_str()),
                    Some("present"),
                );
            }
            if presented {
                components_want_render_after_frame |=
                    self.components[index].component.wants_render();
            }
        }
        self.components_want_render = components_want_render_after_frame;
        self.presented_last_frame = any_component_presented;
        Ok(())
    }

    fn resolve_dynamic_surface_size(
        &mut self,
        index: usize,
        surface_id: &str,
    ) -> Result<Option<(u32, u32)>, ShellRunError> {
        if let Some(size) = self.presentation_engine.surface_size_if_known(surface_id) {
            self.components[index].known_surface_size = Some(size);
            return Ok(Some(size));
        }
        if let Some(size) = self.components[index].known_surface_size {
            return Ok(Some(size));
        }
        let size = self
            .presentation_engine
            .surface_size(surface_id)
            .map_err(ShellRunError::Presentation)?;
        if let Some(size) = size {
            self.components[index].known_surface_size = Some(size);
        }
        Ok(size)
    }
}

fn full_buffer_damage(buffer: &PixelBuffer) -> DamageRect {
    DamageRect {
        x: 0,
        y: 0,
        width: buffer.width.max(1),
        height: buffer.height.max(1),
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
