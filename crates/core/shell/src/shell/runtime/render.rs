use super::super::*;
use mesh_core_presentation::LayerSurfaceSizePolicy;
use mesh_core_render::DamageRect;

impl Shell {
    pub(in crate::shell) fn render_components(&mut self) -> Result<(), ShellRunError> {
        if self.debug.enabled {
            let mut debug_requests = self.publish_debug_snapshot()?;
            self.drain_requests(&mut debug_requests)?;
        }

        for index in 0..self.components.len() {
            let surface_id = self.components[index].surface_id.clone();
            if !self.components[index].component.wants_render() {
                continue;
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
                let cfg = if visible {
                    LayerSurfaceConfig {
                        edge: surface.edge,
                        layer: surface.layer.unwrap_or(Layer::Top),
                        size_policy: if self.components[index].component.allows_shrink_to_fit() {
                            LayerSurfaceSizePolicy::Flexible
                        } else {
                            LayerSurfaceSizePolicy::Fixed
                        },
                        width: surface.width,
                        height: surface.height,
                        exclusive_zone: surface.exclusive_zone,
                        keyboard_mode: surface.keyboard_mode,
                        namespace: surface_id.clone(),
                        margin_top: surface.margin_top,
                        margin_right: surface.margin_right,
                        margin_bottom: surface.margin_bottom,
                        margin_left: surface.margin_left,
                    }
                } else {
                    LayerSurfaceConfig {
                        edge: surface.edge,
                        layer: surface.layer.unwrap_or(Layer::Top),
                        size_policy: LayerSurfaceSizePolicy::Fixed,
                        width: 1,
                        height: 1,
                        exclusive_zone: 0,
                        // Hidden surfaces never want keyboard — even if the
                        // configured mode is exclusive, we don't want to
                        // steal input while the popover is collapsed to 1×1.
                        keyboard_mode: mesh_core_wayland::KeyboardMode::None,
                        namespace: surface_id.clone(),
                        margin_top: 0,
                        margin_right: 0,
                        margin_bottom: 0,
                        margin_left: 0,
                    }
                };
                self.presentation_engine.configure(&surface_id, cfg);

                if !visible {
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
                    break;
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
                self.components[index].known_surface_size = Some((width, height));
                self.components[index]
                    .component
                    .surface_size_changed(width, height);

                let runtime = &mut self.components[index];
                if runtime
                    .paint_buffer
                    .as_ref()
                    .map(|buffer| buffer.width != width || buffer.height != height)
                    .unwrap_or(true)
                {
                    runtime.paint_buffer = Some(PixelBuffer::new(width, height));
                }
                runtime.component.set_profiling_enabled(profiling_enabled);
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

            let mut present_damage = self.components[index].component.take_present_damage();
            if visible && self.debug.show_layout_bounds {
                let runtime = &mut self.components[index];
                if let Some(tree) = runtime.component.last_widget_tree() {
                    let buffer = runtime
                        .paint_buffer
                        .as_mut()
                        .expect("paint buffer initialised");
                    self.debug_overlay.paint_layout_bounds(tree, buffer, 1.0);
                    present_damage = Some(full_buffer_damage(buffer));
                }
            }

            let mut presented = false;
            let present_started = self.profiling_enabled().then(std::time::Instant::now);
            // `take_present_damage == None` means paint produced no changed pixels,
            // so skip the present entirely. `present_with_damage(None)` is reserved
            // for legacy callers that do not know their damage and need a full
            // buffer upload/damage.
            if !visible || present_damage.is_some() {
                self.presentation_engine
                    .present_with_damage(
                        &surface_id,
                        self.components[index].component.id(),
                        visible,
                        self.components[index]
                            .paint_buffer
                            .as_ref()
                            .expect("paint buffer initialised"),
                        present_damage,
                    )
                    .map_err(ShellRunError::Presentation)?;
                presented = true;
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
        }
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
