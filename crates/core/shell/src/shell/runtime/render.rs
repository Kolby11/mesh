use super::super::*;
use mesh_core_render::surface::LayerSurfaceSizePolicy;

impl Shell {
    pub(in crate::shell) fn render_components(&mut self) -> Result<(), ShellRunError> {
        let debug_snapshot = self.debug.enabled.then(|| self.build_debug_snapshot());

        for index in 0..self.components.len() {
            let surface_id = self.components[index].surface_id.clone();
            let surface_size = {
                let surface = self
                    .surfaces
                    .get(&surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(surface_id.clone()))?;
                if surface.width == 0 || surface.height == 0 {
                    self.render_engine.surface_size(&surface_id)?
                } else {
                    Some((surface.width.max(1), surface.height.max(1)))
                }
            };
            if let Some((width, height)) = surface_size {
                self.components[index]
                    .component
                    .surface_size_changed(width, height);
            }
            if !self.components[index].component.wants_render() {
                continue;
            }

            let total_render_started = self.profiling_enabled().then(std::time::Instant::now);
            let profiling_enabled = self.profiling_enabled();
            let mut rerender_attempts = 0;
            let mut component_stage_records = Vec::new();
            let mut component_id = String::new();
            let mut buffer = loop {
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
                component_id = self.components[index].component.id().to_string();

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
                self.render_engine.configure(&surface_id, cfg);

                if !visible {
                    break PixelBuffer::new(1, 1);
                }

                let configured_size = if surface.width == 0 || surface.height == 0 {
                    self.render_engine.surface_size(&surface_id)?
                } else {
                    None
                };
                let width = if surface.width == 0 {
                    configured_size.map(|(width, _)| width).unwrap_or(1)
                } else {
                    surface.width.max(1)
                };
                let height = if surface.height == 0 {
                    configured_size.map(|(_, height)| height).unwrap_or(1)
                } else {
                    surface.height.max(1)
                };
                let mut buffer = PixelBuffer::new(width, height);
                self.components[index]
                    .component
                    .set_profiling_enabled(profiling_enabled);
                self.components[index]
                    .component
                    .paint(self.theme.active(), width, height, &mut buffer)
                    .map_err(ShellRunError::Component)?;
                component_stage_records
                    .extend(self.components[index].component.take_profiling_records());

                if !self.components[index].component.wants_render() || rerender_attempts >= 1 {
                    break buffer;
                }

                rerender_attempts += 1;
            };

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

            if visible && let Some(snapshot) = &debug_snapshot {
                if self.debug.show_layout_bounds {
                    if let Some(tree) = self.components[index].component.last_widget_tree() {
                        self.debug_overlay
                            .paint_layout_bounds(tree, &mut buffer, 1.0);
                    }
                }
                self.debug_overlay
                    .paint_panel(snapshot, self.debug.active_tab, &mut buffer, 1.0);
            }

            let present_started = self.profiling_enabled().then(std::time::Instant::now);
            self.render_engine
                .present(
                    &surface_id,
                    self.components[index].component.id(),
                    visible,
                    &buffer,
                )
                .map_err(ShellRunError::Render)?;
            if let Some(started) = present_started {
                self.record_surface_profiling_stage(
                    &surface_id,
                    Some(component_id.as_str()),
                    mesh_core_debug::ProfilingStage::PresentCommit,
                    started.elapsed(),
                    Some("present"),
                );
            }
            if visible {
                self.record_surface_redraw(
                    &surface_id,
                    Some(component_id.as_str()),
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
        }
        Ok(())
    }
}
