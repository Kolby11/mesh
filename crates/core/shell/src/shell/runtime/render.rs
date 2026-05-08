use super::super::*;

impl Shell {
    pub(in crate::shell) fn render_components(&mut self) -> Result<(), ShellRunError> {
        let debug_snapshot = self.debug.enabled.then(|| self.build_debug_snapshot());

        for runtime in &mut self.components {
            let surface_size = {
                let surface = self
                    .surfaces
                    .get(&runtime.surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(runtime.surface_id.clone()))?;
                if surface.width == 0 || surface.height == 0 {
                    self.render_engine.surface_size(&runtime.surface_id)?
                } else {
                    Some((surface.width.max(1), surface.height.max(1)))
                }
            };
            if let Some((width, height)) = surface_size {
                runtime.component.surface_size_changed(width, height);
            }
            if !runtime.component.wants_render() {
                continue;
            }

            let mut rerender_attempts = 0;
            let mut buffer = loop {
                let surface = self
                    .surfaces
                    .get_mut(&runtime.surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(runtime.surface_id.clone()))?;
                runtime
                    .component
                    .render(surface)
                    .map_err(ShellRunError::Component)?;

                let visible = self
                    .core
                    .surfaces
                    .get(&runtime.surface_id)
                    .map(|state| state.visible)
                    .unwrap_or(surface.visible);
                let cfg = if visible {
                    LayerSurfaceConfig {
                        edge: surface.edge,
                        layer: surface.layer.unwrap_or(Layer::Top),
                        width: surface.width,
                        height: surface.height,
                        exclusive_zone: surface.exclusive_zone,
                        keyboard_mode: surface.keyboard_mode,
                        namespace: runtime.surface_id.clone(),
                        margin_top: surface.margin_top,
                        margin_right: surface.margin_right,
                        margin_bottom: surface.margin_bottom,
                        margin_left: surface.margin_left,
                    }
                } else {
                    LayerSurfaceConfig {
                        edge: surface.edge,
                        layer: surface.layer.unwrap_or(Layer::Top),
                        width: 1,
                        height: 1,
                        exclusive_zone: 0,
                        // Hidden surfaces never want keyboard — even if the
                        // configured mode is exclusive, we don't want to
                        // steal input while the popover is collapsed to 1×1.
                        keyboard_mode: mesh_core_wayland::KeyboardMode::None,
                        namespace: runtime.surface_id.clone(),
                        margin_top: 0,
                        margin_right: 0,
                        margin_bottom: 0,
                        margin_left: 0,
                    }
                };
                self.render_engine.configure(&runtime.surface_id, cfg);

                if !visible {
                    break PixelBuffer::new(1, 1);
                }

                let configured_size = if surface.width == 0 || surface.height == 0 {
                    self.render_engine.surface_size(&runtime.surface_id)?
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
                runtime
                    .component
                    .paint(self.theme.active(), width, height, &mut buffer)
                    .map_err(ShellRunError::Component)?;

                if !runtime.component.wants_render() || rerender_attempts >= 1 {
                    break buffer;
                }

                rerender_attempts += 1;
            };

            let visible = self
                .core
                .surfaces
                .get(&runtime.surface_id)
                .map(|state| state.visible)
                .unwrap_or_else(|| {
                    self.surfaces
                        .get(&runtime.surface_id)
                        .map(|surface| surface.visible)
                        .unwrap_or(true)
                });

            if visible && let Some(snapshot) = &debug_snapshot {
                if self.debug.show_layout_bounds {
                    if let Some(tree) = runtime.component.last_widget_tree() {
                        self.debug_overlay
                            .paint_layout_bounds(tree, &mut buffer, 1.0);
                    }
                }
                self.debug_overlay
                    .paint_panel(snapshot, self.debug.active_tab, &mut buffer, 1.0);
            }

            self.render_engine
                .present(
                    &runtime.surface_id,
                    runtime.component.id(),
                    visible,
                    &buffer,
                )
                .map_err(ShellRunError::Render)?;
        }
        Ok(())
    }
}
