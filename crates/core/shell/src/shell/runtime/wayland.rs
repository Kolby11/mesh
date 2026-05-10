use super::super::*;

impl Shell {
    pub(in crate::shell) fn dispatch_wayland(&mut self) -> Result<(), ShellRunError> {
        let events = coalesce_pointer_moves(self.presentation_engine.poll_events());
        for event in events {
            let input_started = self.profiling_enabled().then(std::time::Instant::now);
            let trigger_kind = profiling_trigger_for_event(&event);
            tracing::trace!(
                "[hover] dispatch_wayland: got event {:?}",
                std::mem::discriminant(&event)
            );
            let physical_surface_id = event_surface_id(&event).to_string();
            let is_keyboard_event =
                matches!(&event, WindowEvent::Key { .. } | WindowEvent::Char { .. });
            let route_surface_id = if is_keyboard_event {
                self.keyboard_focus_surface
                    .as_ref()
                    .filter(|surface_id| {
                        self.core
                            .surfaces
                            .get(*surface_id)
                            .map(|state| state.visible)
                            .unwrap_or(true)
                            && self
                                .components
                                .iter()
                                .any(|runtime| runtime.surface_id == **surface_id)
                    })
                    .cloned()
                    .unwrap_or_else(|| physical_surface_id.clone())
            } else {
                physical_surface_id
            };

            let Some(index) = self
                .components
                .iter()
                .position(|runtime| runtime.surface_id == route_surface_id)
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
                .or(self.components[index].known_surface_size)
                .or_else(|| {
                    self.components[index]
                        .paint_buffer
                        .as_ref()
                        .map(|buffer| (buffer.width.max(1), buffer.height.max(1)))
                })
                .or_else(|| {
                    self.presentation_engine
                        .surface_size_if_known(&runtime_surface_id)
                })
                .unwrap_or((1, 1));
            self.components[index].known_surface_size = Some(surface_size);

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
                WindowEvent::PointerButton {
                    x,
                    y,
                    pressed: true,
                    ..
                } => {
                    self.claim_keyboard_focus_for_surface(&runtime_surface_id);
                    ComponentInput::PointerButton {
                        x,
                        y,
                        pressed: true,
                    }
                }
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
            if let Some(started) = input_started {
                self.record_shell_profiling_stage(
                    mesh_core_debug::ProfilingStage::InputHandling,
                    started.elapsed(),
                    Some(trigger_kind),
                );
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

fn profiling_trigger_for_event(event: &WindowEvent) -> &'static str {
    match event {
        WindowEvent::PointerMove { .. } => "pointer_move",
        WindowEvent::PointerButton { .. } => "pointer_button",
        WindowEvent::Scroll { .. } => "scroll",
        WindowEvent::Key { .. } => "key",
        WindowEvent::Char { .. } => "char",
    }
}
