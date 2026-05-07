use super::super::*;

impl Shell {
    pub(in crate::shell) fn dispatch_wayland(&mut self) -> Result<(), ShellRunError> {
        let events = coalesce_pointer_moves(self.render_engine.poll_events());
        for event in events {
            tracing::trace!(
                "[hover] dispatch_wayland: got event {:?}",
                std::mem::discriminant(&event)
            );
            let surface_id = event_surface_id(&event);

            let Some(index) = self
                .components
                .iter()
                .position(|runtime| runtime.surface_id == *surface_id)
            else {
                continue;
            };

            let runtime_surface_id = self.components[index].surface_id.clone();
            let Some(surface) = self.surfaces.get(&runtime_surface_id) else {
                continue;
            };
            let fixed_surface_size = if surface.width == 0 || surface.height == 0 {
                None
            } else {
                Some((surface.width.max(1), surface.height.max(1)))
            };
            let _ = surface;
            let surface_size = fixed_surface_size
                .or(self.render_engine.surface_size(&runtime_surface_id)?)
                .unwrap_or((1, 1));

            if let WindowEvent::Key {
                event: WindowKeyEvent::Pressed(key, mods),
                ..
            } = &event
            {
                if let Some(request) =
                    shell_global_shortcut_request(key, mods.ctrl, mods.shift, self.debug.enabled)
                {
                    let mut pending = VecDeque::from([request]);
                    self.drain_requests(&mut pending)?;
                    continue;
                }
            }

            let input = match event {
                WindowEvent::PointerMove { x, y, .. } => ComponentInput::PointerMove { x, y },
                WindowEvent::PointerButton { x, y, pressed, .. } => {
                    ComponentInput::PointerButton { x, y, pressed }
                }
                WindowEvent::Scroll { x, y, dx, dy, .. } => ComponentInput::Scroll { x, y, dx, dy },
                WindowEvent::Key {
                    event: WindowKeyEvent::Pressed(key, mods),
                    ..
                } => {
                    self.active_key_modifiers = KeyModifiers {
                        ctrl: mods.ctrl,
                        shift: mods.shift,
                        alt: mods.alt,
                    };
                    component_key_pressed_input(key, mods.ctrl, mods.shift, mods.alt)
                }
                WindowEvent::Key {
                    event: WindowKeyEvent::Released(key),
                    ..
                } => {
                    update_modifiers_for_key_release(&mut self.active_key_modifiers, &key);
                    component_key_released_input(key, self.active_key_modifiers)
                }
                WindowEvent::Char { ch, .. } => ComponentInput::Char { ch },
            };

            tracing::trace!(
                "[hover] dispatch_wayland: routing event to surface_id={}",
                runtime_surface_id
            );
            let emitted = {
                let runtime = &mut self.components[index];
                runtime
                    .component
                    .surface_size_changed(surface_size.0, surface_size.1);
                runtime.component.handle_input(
                    self.theme.active(),
                    surface_size.0,
                    surface_size.1,
                    input,
                )
            }
            .map_err(ShellRunError::Component)?;

            for request in emitted {
                let mut pending = VecDeque::from([request]);
                self.drain_requests(&mut pending)?;
            }
        }

        Ok(())
    }

    pub(in crate::shell) fn flush_wayland(&mut self) -> Result<(), ShellRunError> {
        for runtime in &self.components {
            let surface = self
                .surfaces
                .get(&runtime.surface_id)
                .ok_or_else(|| ShellRunError::MissingSurface(runtime.surface_id.clone()))?;
            tracing::trace!(
                "flushing surface '{}' size={}x{} visible={}",
                runtime.surface_id,
                surface.width,
                surface.height,
                surface.visible
            );
        }
        Ok(())
    }
}
