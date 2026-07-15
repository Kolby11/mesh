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

            // Element picker input is handled by the shell before the target
            // component sees it. This mirrors browser devtools: moving updates
            // the highlight and clicking freezes the current selection instead
            // of activating the inspected control.
            if self.debug.element_picker_enabled
                && target_surface_id != "@mesh/debug-inspector"
                && let Some((x, y, selected)) = match &event {
                    RoutedWindowEvent::PointerMove { x, y } => Some((*x, *y, false)),
                    RoutedWindowEvent::PointerButton {
                        x,
                        y,
                        pressed: true,
                    } => Some((*x, *y, true)),
                    _ => None,
                }
            {
                let debug_tree = match target {
                    TargetRef::Parent => {
                        self.components[index].component.last_widget_tree().cloned()
                    }
                    TargetRef::Child(child_index) => {
                        let child = &self.components[index].children[child_index];
                        self.components[index].component.child_surface_debug_tree(
                            &child.node_key,
                            (
                                child.content_padding.0 as f32,
                                child.content_padding.1 as f32,
                            ),
                        )
                    }
                };
                if let Some(tree) = debug_tree
                    && let Some(hit) = mesh_core_interaction::inspect_hit_test(&tree, x, y)
                {
                    let fallback_source = self.components[index]
                        .component
                        .source_path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default();
                    let mut inspected = inspected_element_json(
                        target_surface_id,
                        hit.node,
                        hit.bounds,
                        &fallback_source,
                    );
                    if selected
                        && let Some(source_file) = inspected
                            .get("source_file")
                            .and_then(|value| value.as_str())
                        && let Some(line) = source_line_for_node(source_file, hit.node)
                    {
                        inspected["source_line"] = serde_json::json!(line);
                    }
                    self.debug.inspected_element = Some(inspected);
                }
                if selected {
                    self.debug.element_picker_enabled = false;
                }
                self.invalidate_debug_layout_bounds_targets();
                continue;
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

fn edge_json(edges: mesh_core_elements::style::Edges) -> serde_json::Value {
    serde_json::json!({
        "top": edges.top,
        "right": edges.right,
        "bottom": edges.bottom,
        "left": edges.left,
    })
}

fn inspected_element_json(
    surface_id: &str,
    node: &mesh_core_elements::WidgetNode,
    bounds: mesh_core_interaction::ContentBounds,
    fallback_source: &str,
) -> serde_json::Value {
    let style = &node.computed_style;
    let source_file = node
        .attributes
        .get("_mesh_source_file")
        .map(String::as_str)
        .unwrap_or(fallback_source);
    let source_tag = mesh_core_interaction::source_element_tag(node);
    serde_json::json!({
        "surface_id": surface_id,
        "key": node.mesh_key().unwrap_or(""),
        "tag": source_tag,
        "runtime_tag": node.tag,
        "module_id": node.module_id().unwrap_or(""),
        "source_file": source_file,
        "source_line": 1,
        "id": node.attributes.get("id").cloned().unwrap_or_default(),
        "classes": node.attributes.get("class").cloned().unwrap_or_default(),
        "content": node.attributes.get("content").cloned().unwrap_or_default(),
        "bounds": {
            "x": bounds.0,
            "y": bounds.1,
            "width": (bounds.2 - bounds.0).max(0.0),
            "height": (bounds.3 - bounds.1).max(0.0),
        },
        "box_model": {
            "margin": edge_json(style.margin),
            "border": edge_json(style.border_width),
            "padding": edge_json(style.padding),
        },
        "style": {
            "display": format!("{:?}", style.display).to_lowercase(),
            "position": format!("{:?}", style.position).to_lowercase(),
            "opacity": style.opacity,
            "gap": style.gap,
            "font_family": style.font_family.as_ref(),
            "font_size": style.font_size,
            "font_weight": style.font_weight,
            "z_index": style.z_index,
            "overflow_x": format!("{:?}", style.overflow_x).to_lowercase(),
            "overflow_y": format!("{:?}", style.overflow_y).to_lowercase(),
        }
    })
}

/// Best-effort source lookup for the selected runtime node. Template parsing
/// does not yet retain spans, so score matching opening tags by id/classes.
/// Repeated loop instances intentionally resolve to their shared template line.
fn source_line_for_node(path: &str, node: &mesh_core_elements::WidgetNode) -> Option<u32> {
    let source = std::fs::read_to_string(path).ok()?;
    let lines = source.lines().collect::<Vec<_>>();
    let tag = mesh_core_interaction::source_element_tag(node);
    let opening = format!("<{tag}");
    let id = node.attributes.get("id").filter(|value| !value.is_empty());
    let classes = node
        .attributes
        .get("class")
        .map(|value| value.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut best: Option<(usize, u32)> = None;

    for (index, line) in lines.iter().enumerate() {
        let Some(column) = line.find(&opening) else {
            continue;
        };
        let mut markup = line[column..].to_string();
        for continuation in lines.iter().skip(index + 1).take(7) {
            if markup.contains('>') {
                break;
            }
            markup.push(' ');
            markup.push_str(continuation.trim());
        }
        let mut score = 1usize;
        if let Some(id) = id
            && (markup.contains(&format!("id=\"{id}\"")) || markup.contains(&format!("id='{id}'")))
        {
            score += 100;
        }
        for class in &classes {
            if markup.contains(class) {
                score += 10;
            }
        }
        if best.is_none_or(|(best_score, _)| score > best_score) {
            best = Some((score, (index + 1) as u32));
        }
    }

    best.map(|(_, line)| line)
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
    fn inspected_node_source_line_prefers_matching_class() {
        let source = "<template>\n  <row class=\"other\" />\n  <row\n    class=\"right-cluster\"\n  />\n</template>\n";
        let file = tempfile::Builder::new().suffix(".mesh").tempfile().unwrap();
        std::fs::write(file.path(), source).unwrap();
        let mut node = mesh_core_elements::WidgetNode::new("row");
        node.attributes
            .insert("data-mesh-element".into(), "row".into());
        node.attributes
            .insert("class".into(), "right-cluster".into());

        assert_eq!(
            source_line_for_node(file.path().to_str().unwrap(), &node),
            Some(3)
        );
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
