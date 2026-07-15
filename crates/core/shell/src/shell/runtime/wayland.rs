use super::super::*;

const MAX_WAYLAND_EVENTS_PER_FRAME: usize = 32;

impl Shell {
    pub(in crate::shell) fn dispatch_wayland(&mut self) -> Result<(), ShellRunError> {
        let events = coalesce_input_events(self.presentation_engine.poll_events());
        if !events.is_empty() {
            self.presented_last_frame = true;
        }
        self.pending_wayland_events.extend(events);

        for _ in 0..MAX_WAYLAND_EVENTS_PER_FRAME {
            let Some(event) = self.pending_wayland_events.pop_front() else {
                break;
            };

            let input_started = self.profiling_enabled().then(std::time::Instant::now);
            let trigger_kind = profiling_trigger_for_event(&event);
            tracing::trace!(
                "[hover] dispatch_wayland: got event {:?}",
                std::mem::discriminant(&event)
            );
            let (physical_surface_id, event) = split_window_event(event);
            let is_keyboard_event = event.is_keyboard();
            let keyboard_focus_surface = if is_keyboard_event {
                self.keyboard_focus_surface.clone()
            } else {
                None
            };
            let route_surface_id = if is_keyboard_event {
                if let Some(surface_id) = keyboard_focus_surface.as_deref() {
                    let focused_surface_visible = self
                        .core
                        .surfaces
                        .get(surface_id)
                        .map(|state| state.visible)
                        .unwrap_or(true);
                    if focused_surface_visible
                        && self.component_index_for_surface(surface_id).is_some()
                    {
                        surface_id
                    } else {
                        &physical_surface_id
                    }
                } else {
                    &physical_surface_id
                }
            } else {
                &physical_surface_id
            };

            let Some((index, target)) = self.component_target_for_surface(route_surface_id) else {
                continue;
            };

            let target_surface_id = route_surface_id;
            if matches!(
                event,
                RoutedWindowEvent::PointerMove { .. }
                    | RoutedWindowEvent::PointerButton { .. }
                    | RoutedWindowEvent::Scroll { .. }
                    | RoutedWindowEvent::TwoFingerScroll { .. }
                    | RoutedWindowEvent::GestureSwipeBegin { .. }
                    | RoutedWindowEvent::GestureSwipeUpdate { .. }
                    | RoutedWindowEvent::GestureSwipeEnd { .. }
                    | RoutedWindowEvent::GesturePinchBegin { .. }
                    | RoutedWindowEvent::GesturePinchUpdate { .. }
                    | RoutedWindowEvent::GesturePinchEnd { .. }
                    | RoutedWindowEvent::GestureHoldBegin { .. }
                    | RoutedWindowEvent::GestureHoldEnd { .. }
                    | RoutedWindowEvent::TouchDown { .. }
                    | RoutedWindowEvent::TouchMove { .. }
                    | RoutedWindowEvent::TouchUp { .. }
                    | RoutedWindowEvent::TouchCancel
            ) {
                self.cancel_pending_popover_hide(target_surface_id);
                if let RoutedWindowEvent::PointerMove { x, y } = &event {
                    self.cancel_pending_child_popover_hides_at(target_surface_id, *x, *y);
                }
            } else if matches!(event, RoutedWindowEvent::PointerLeave)
                && self.components[index]
                    .target(target)
                    .popup_parent_surface
                    .is_some()
            {
                self.drain_request(CoreRequest::HidePopover {
                    surface_id: target_surface_id.to_string(),
                    defer_for_hover_bridge: true,
                })?;
            } else if matches!(event, RoutedWindowEvent::PointerLeave)
                && matches!(target, TargetRef::Parent)
            {
                self.defer_child_popover_hides_for_parent(target_surface_id);
            }
            let Some(surface) = self.surfaces.get(target_surface_id) else {
                continue;
            };
            let fixed_surface_size = if surface.width == 0 || surface.height == 0 {
                None
            } else {
                Some((surface.width.max(1), surface.height.max(1)))
            };
            let _ = surface;
            let target_surface_size = fixed_surface_size
                .or(self.components[index].target(target).known_surface_size)
                .or_else(|| {
                    self.components[index]
                        .target(target)
                        .paint_buffer
                        .as_ref()
                        .map(|buffer| (buffer.width.max(1), buffer.height.max(1)))
                })
                .or_else(|| {
                    self.presentation_engine
                        .surface_size_if_known(target_surface_id)
                })
                .unwrap_or((1, 1));
            self.components[index].target_mut(target).known_surface_size =
                Some(target_surface_size);
            let component_surface_size = match target {
                TargetRef::Parent => self.components[index]
                    .component
                    .content_input_size()
                    .unwrap_or(target_surface_size),
                TargetRef::Child(_) => self.components[index]
                    .parent
                    .known_surface_size
                    .or_else(|| {
                        self.surfaces
                            .get(&self.components[index].surface_id)
                            .map(|surface| (surface.width.max(1), surface.height.max(1)))
                    })
                    .unwrap_or(target_surface_size),
            };

            if let RoutedWindowEvent::KeyPressed { key, mods } = &event {
                if let Some(request) =
                    shell_global_shortcut_request(key, mods.ctrl, mods.shift, self.debug.enabled)
                {
                    self.drain_request(request)?;
                    continue;
                }
            }

            let input = match event {
                RoutedWindowEvent::PointerButton {
                    x,
                    y,
                    pressed: true,
                } => {
                    self.claim_keyboard_focus_for_surface(target_surface_id);
                    ComponentInput::PointerButton {
                        x,
                        y,
                        pressed: true,
                    }
                }
                RoutedWindowEvent::PointerMove { x, y } => ComponentInput::PointerMove { x, y },
                RoutedWindowEvent::PointerLeave => ComponentInput::PointerLeave,
                RoutedWindowEvent::PointerButton { x, y, pressed } => {
                    ComponentInput::PointerButton { x, y, pressed }
                }
                RoutedWindowEvent::Scroll { x, y, dx, dy } => {
                    ComponentInput::Scroll { x, y, dx, dy }
                }
                RoutedWindowEvent::TwoFingerScroll { x, y, dx, dy } => {
                    ComponentInput::TwoFingerScroll { x, y, dx, dy }
                }
                RoutedWindowEvent::GestureSwipeBegin { fingers } => {
                    ComponentInput::GestureSwipeBegin { fingers }
                }
                RoutedWindowEvent::GestureSwipeUpdate { dx, dy } => {
                    ComponentInput::GestureSwipeUpdate { dx, dy }
                }
                RoutedWindowEvent::GestureSwipeEnd { cancelled } => {
                    ComponentInput::GestureSwipeEnd { cancelled }
                }
                RoutedWindowEvent::GesturePinchBegin { fingers } => {
                    ComponentInput::GesturePinchBegin { fingers }
                }
                RoutedWindowEvent::GesturePinchUpdate {
                    dx,
                    dy,
                    scale,
                    rotation,
                } => ComponentInput::GesturePinchUpdate {
                    dx,
                    dy,
                    scale,
                    rotation,
                },
                RoutedWindowEvent::GesturePinchEnd { cancelled } => {
                    ComponentInput::GesturePinchEnd { cancelled }
                }
                RoutedWindowEvent::GestureHoldBegin { fingers } => {
                    ComponentInput::GestureHoldBegin { fingers }
                }
                RoutedWindowEvent::GestureHoldEnd { cancelled } => {
                    ComponentInput::GestureHoldEnd { cancelled }
                }
                RoutedWindowEvent::TouchDown { id, x, y } => ComponentInput::TouchDown { id, x, y },
                RoutedWindowEvent::TouchMove { id, x, y } => ComponentInput::TouchMove { id, x, y },
                RoutedWindowEvent::TouchUp { id } => ComponentInput::TouchUp { id },
                RoutedWindowEvent::TouchCancel => ComponentInput::TouchCancel,
                RoutedWindowEvent::KeyPressed { key, mods } => {
                    self.active_key_modifiers = KeyModifiers {
                        ctrl: mods.ctrl,
                        shift: mods.shift,
                        alt: mods.alt,
                    };
                    component_key_pressed_input(key, mods.ctrl, mods.shift, mods.alt)
                }
                RoutedWindowEvent::KeyReleased { key } => {
                    update_modifiers_for_key_release(&mut self.active_key_modifiers, &key);
                    component_key_released_input(key, self.active_key_modifiers)
                }
                RoutedWindowEvent::Char { ch } => ComponentInput::Char { ch },
            };
            tracing::trace!(
                "[hover] dispatch_wayland: routing event to surface_id={}",
                target_surface_id
            );
            let emitted = {
                let runtime = &mut self.components[index];
                if runtime.component.content_input_size() != Some(component_surface_size) {
                    runtime
                        .component
                        .surface_size_changed(component_surface_size.0, component_surface_size.1);
                }
                let emitted = match target {
                    TargetRef::Parent => runtime.component.handle_input(
                        self.theme.active(),
                        component_surface_size.0,
                        component_surface_size.1,
                        input,
                    )?,
                    TargetRef::Child(child_index) => {
                        let node_key = runtime.children[child_index].node_key.clone();
                        runtime.component.handle_child_surface_input(
                            &node_key,
                            self.theme.active(),
                            component_surface_size.0,
                            component_surface_size.1,
                            input,
                        )?
                    }
                };
                let interactive = runtime.component.hovered_target_is_interactive();
                self.presentation_engine
                    .set_pointer_interactive(interactive);
                Ok(emitted)
            }
            .map_err(ShellRunError::Component)?;

            if let Some(started) = input_started {
                let component_id = self.components[index].component.id().to_string();
                self.record_surface_profiling_stage(
                    target_surface_id,
                    Some(component_id.as_str()),
                    mesh_core_debug::ProfilingStage::InputHandling,
                    started.elapsed(),
                    Some(trigger_kind),
                );
            }
            if !emitted.is_empty() {
                let mut pending = VecDeque::from(emitted);
                self.drain_requests(&mut pending)?;
            }
        }

        Ok(())
    }

    pub(in crate::shell) fn flush_wayland(&mut self) -> Result<(), ShellRunError> {
        if !tracing::enabled!(tracing::Level::TRACE) {
            return Ok(());
        }

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

#[derive(Debug)]
enum RoutedWindowEvent {
    PointerMove {
        x: f32,
        y: f32,
    },
    PointerLeave,
    PointerButton {
        x: f32,
        y: f32,
        pressed: bool,
    },
    Scroll {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    TwoFingerScroll {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    GestureSwipeBegin {
        fingers: u32,
    },
    GestureSwipeUpdate {
        dx: f32,
        dy: f32,
    },
    GestureSwipeEnd {
        cancelled: bool,
    },
    GesturePinchBegin {
        fingers: u32,
    },
    GesturePinchUpdate {
        dx: f32,
        dy: f32,
        scale: f32,
        rotation: f32,
    },
    GesturePinchEnd {
        cancelled: bool,
    },
    GestureHoldBegin {
        fingers: u32,
    },
    GestureHoldEnd {
        cancelled: bool,
    },
    TouchDown {
        id: i32,
        x: f32,
        y: f32,
    },
    TouchMove {
        id: i32,
        x: f32,
        y: f32,
    },
    TouchUp {
        id: i32,
    },
    TouchCancel,
    KeyPressed {
        key: String,
        mods: mesh_core_presentation::KeyMods,
    },
    KeyReleased {
        key: String,
    },
    Char {
        ch: char,
    },
}

impl RoutedWindowEvent {
    fn is_keyboard(&self) -> bool {
        matches!(
            self,
            Self::KeyPressed { .. } | Self::KeyReleased { .. } | Self::Char { .. }
        )
    }
}

fn split_window_event(event: WindowEvent) -> (std::sync::Arc<str>, RoutedWindowEvent) {
    match event {
        WindowEvent::PointerMove { surface_id, x, y } => {
            (surface_id, RoutedWindowEvent::PointerMove { x, y })
        }
        WindowEvent::PointerLeave { surface_id } => (surface_id, RoutedWindowEvent::PointerLeave),
        WindowEvent::PointerButton {
            surface_id,
            x,
            y,
            pressed,
        } => (
            surface_id,
            RoutedWindowEvent::PointerButton { x, y, pressed },
        ),
        WindowEvent::Scroll {
            surface_id,
            x,
            y,
            dx,
            dy,
        } => (surface_id, RoutedWindowEvent::Scroll { x, y, dx, dy }),
        WindowEvent::TwoFingerScroll {
            surface_id,
            x,
            y,
            dx,
            dy,
        } => (
            surface_id,
            RoutedWindowEvent::TwoFingerScroll { x, y, dx, dy },
        ),
        WindowEvent::Key { surface_id, event } => match event {
            WindowKeyEvent::Pressed(key, mods) => {
                (surface_id, RoutedWindowEvent::KeyPressed { key, mods })
            }
            WindowKeyEvent::Released(key) => (surface_id, RoutedWindowEvent::KeyReleased { key }),
        },
        WindowEvent::Char { surface_id, ch } => (surface_id, RoutedWindowEvent::Char { ch }),
        WindowEvent::GestureSwipeBegin {
            surface_id,
            fingers,
        } => (surface_id, RoutedWindowEvent::GestureSwipeBegin { fingers }),
        WindowEvent::GestureSwipeUpdate { surface_id, dx, dy } => {
            (surface_id, RoutedWindowEvent::GestureSwipeUpdate { dx, dy })
        }
        WindowEvent::GestureSwipeEnd {
            surface_id,
            cancelled,
        } => (surface_id, RoutedWindowEvent::GestureSwipeEnd { cancelled }),
        WindowEvent::GesturePinchBegin {
            surface_id,
            fingers,
        } => (surface_id, RoutedWindowEvent::GesturePinchBegin { fingers }),
        WindowEvent::GesturePinchUpdate {
            surface_id,
            dx,
            dy,
            scale,
            rotation,
        } => (
            surface_id,
            RoutedWindowEvent::GesturePinchUpdate {
                dx,
                dy,
                scale,
                rotation,
            },
        ),
        WindowEvent::GesturePinchEnd {
            surface_id,
            cancelled,
        } => (surface_id, RoutedWindowEvent::GesturePinchEnd { cancelled }),
        WindowEvent::GestureHoldBegin {
            surface_id,
            fingers,
        } => (surface_id, RoutedWindowEvent::GestureHoldBegin { fingers }),
        WindowEvent::GestureHoldEnd {
            surface_id,
            cancelled,
        } => (surface_id, RoutedWindowEvent::GestureHoldEnd { cancelled }),
        WindowEvent::TouchDown {
            surface_id,
            id,
            x,
            y,
        } => (surface_id, RoutedWindowEvent::TouchDown { id, x, y }),
        WindowEvent::TouchMove {
            surface_id,
            id,
            x,
            y,
        } => (surface_id, RoutedWindowEvent::TouchMove { id, x, y }),
        WindowEvent::TouchUp { surface_id, id } => (surface_id, RoutedWindowEvent::TouchUp { id }),
        WindowEvent::TouchCancel { surface_id } => (surface_id, RoutedWindowEvent::TouchCancel),
    }
}

fn profiling_trigger_for_event(event: &WindowEvent) -> &'static str {
    match event {
        WindowEvent::PointerMove { .. } => "pointer_move",
        WindowEvent::PointerLeave { .. } => "pointer_leave",
        WindowEvent::PointerButton { .. } => "pointer_button",
        WindowEvent::Scroll { .. } => "scroll",
        WindowEvent::TwoFingerScroll { .. } => "two_finger_scroll",
        WindowEvent::Key { .. } => "key",
        WindowEvent::Char { .. } => "char",
        WindowEvent::GestureSwipeBegin { .. } => "gesture_swipe_begin",
        WindowEvent::GestureSwipeUpdate { .. } => "gesture_swipe_update",
        WindowEvent::GestureSwipeEnd { .. } => "gesture_swipe_end",
        WindowEvent::GesturePinchBegin { .. } => "gesture_pinch_begin",
        WindowEvent::GesturePinchUpdate { .. } => "gesture_pinch_update",
        WindowEvent::GesturePinchEnd { .. } => "gesture_pinch_end",
        WindowEvent::GestureHoldBegin { .. } => "gesture_hold_begin",
        WindowEvent::GestureHoldEnd { .. } => "gesture_hold_end",
        WindowEvent::TouchDown { .. } => "touch_down",
        WindowEvent::TouchMove { .. } => "touch_move",
        WindowEvent::TouchUp { .. } => "touch_up",
        WindowEvent::TouchCancel { .. } => "touch_cancel",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    #[test]
    fn split_window_event_preserves_surface_and_payload() {
        let (surface_id, event) = split_window_event(WindowEvent::PointerButton {
            surface_id: "panel".into(),
            x: 12.0,
            y: 24.0,
            pressed: true,
        });
        assert_eq!(surface_id.as_ref(), "panel");
        assert!(matches!(
            event,
            RoutedWindowEvent::PointerButton {
                x: 12.0,
                y: 24.0,
                pressed: true
            }
        ));

        let (surface_id, event) = split_window_event(WindowEvent::Key {
            surface_id: "launcher".into(),
            event: WindowKeyEvent::Pressed(
                "Enter".to_string(),
                mesh_core_presentation::KeyMods {
                    ctrl: true,
                    shift: false,
                    alt: true,
                },
            ),
        });
        assert_eq!(surface_id.as_ref(), "launcher");
        assert!(event.is_keyboard());
        assert!(matches!(
            event,
            RoutedWindowEvent::KeyPressed {
                ref key,
                ref mods
            } if key == "Enter" && mods.ctrl && !mods.shift && mods.alt
        ));
    }

    #[test]
    fn split_window_event_routes_gesture_and_touch_payloads() {
        let (surface_id, event) = split_window_event(WindowEvent::GesturePinchUpdate {
            surface_id: "panel".into(),
            dx: 1.0,
            dy: -2.0,
            scale: 1.25,
            rotation: 18.0,
        });
        assert_eq!(surface_id.as_ref(), "panel");
        assert!(matches!(
            event,
            RoutedWindowEvent::GesturePinchUpdate {
                dx: 1.0,
                dy: -2.0,
                scale: 1.25,
                rotation: 18.0,
            }
        ));

        let (surface_id, event) = split_window_event(WindowEvent::TouchDown {
            surface_id: "popover".into(),
            id: 7,
            x: 14.0,
            y: 22.0,
        });
        assert_eq!(surface_id.as_ref(), "popover");
        assert!(matches!(
            event,
            RoutedWindowEvent::TouchDown {
                id: 7,
                x: 14.0,
                y: 22.0,
            }
        ));
    }

    #[test]
    #[ignore = "release-only dispatch surface-id split microbenchmark"]
    fn dispatch_surface_id_split_benchmark() {
        const ITERATIONS: usize = 500_000;
        let surface_id: std::sync::Arc<str> =
            "@mesh/benchmark-panel/with/a/long/child-surface-id".into();
        let old_events: Vec<_> = (0..ITERATIONS)
            .map(|index| WindowEvent::PointerMove {
                surface_id: surface_id.clone(),
                x: (index % 256) as f32,
                y: (index % 128) as f32,
            })
            .collect();
        let new_events = old_events.clone();

        let started = Instant::now();
        for event in old_events {
            let target_surface_id = mesh_core_presentation::event_surface_id(&event).to_string();
            black_box(target_surface_id);
            black_box(event);
        }
        let old_elapsed = started.elapsed();

        let started = Instant::now();
        for event in new_events {
            let (target_surface_id, routed) = split_window_event(event);
            black_box(target_surface_id);
            black_box(routed);
        }
        let new_elapsed = started.elapsed();

        eprintln!("dispatch surface-id target clone: old={old_elapsed:?} split={new_elapsed:?}");
        assert!(
            new_elapsed < old_elapsed,
            "split path should avoid the old per-event target surface id clone"
        );
    }
}
