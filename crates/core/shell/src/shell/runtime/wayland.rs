use super::super::*;
use mesh_core_elements::Prop;

const MAX_WAYLAND_EVENTS_PER_FRAME: usize = 32;

impl Shell {
    pub(in crate::shell) fn sync_text_input_state(&mut self) -> Result<(), ShellRunError> {
        let Some(surface_id) = self.keyboard_focus_surface.clone() else {
            self.presentation_engine
                .set_text_input_state(None, None)
                .map_err(ShellRunError::Presentation)?;
            return Ok(());
        };
        let Some(index) = self.component_index_for_surface(&surface_id) else {
            self.presentation_engine
                .set_text_input_state(None, None)
                .map_err(ShellRunError::Presentation)?;
            return Ok(());
        };
        let state = self.components[index]
            .component
            .text_input_state()
            .map(|state| {
                mesh_core_presentation::TextInputState::new(
                    state.surrounding_text,
                    state.cursor,
                    state.anchor,
                )
            });
        match state {
            Some(state) => self
                .presentation_engine
                .set_text_input_state(Some(&surface_id), Some(state))
                .map_err(ShellRunError::Presentation),
            None => self
                .presentation_engine
                .set_text_input_state(None, None)
                .map_err(ShellRunError::Presentation),
        }
    }

    fn remember_popup_grab(
        &mut self,
        surface_id: &str,
        identity: mesh_core_presentation::PointerButtonIdentity,
    ) {
        self.pending_popup_grab = Some(PendingPopupGrab {
            surface_id: surface_id.to_string(),
            identity,
            released_at: None,
        });
    }

    fn release_popup_grab(&mut self, surface_id: &str) {
        if let Some(pending) = self.pending_popup_grab.as_mut()
            && pending.surface_id == surface_id
            && pending.released_at.is_none()
        {
            pending.released_at = Some(std::time::Instant::now());
        }
    }

    pub(super) fn take_pending_popup_grab(
        &mut self,
        parent_surface_id: &str,
    ) -> Option<mesh_core_presentation::PointerButtonIdentity> {
        take_pending_popup_grab_at(
            &mut self.pending_popup_grab,
            parent_surface_id,
            std::time::Instant::now(),
        )
    }

    #[cfg(test)]
    pub(in crate::shell) fn dispatch_wayland(&mut self) -> Result<(), ShellRunError> {
        self.dispatch_wayland_inner()?;
        self.process_effects();
        Ok(())
    }

    pub(in crate::shell) fn dispatch_wayland_inner(&mut self) -> Result<(), ShellRunError> {
        let events = match self.presentation_engine.poll_events() {
            Ok(events) => coalesce_input_events(events),
            Err(error) => {
                // Connection loss tears down compositor identities before the
                // typed error reaches the shell. Consume those lifecycle
                // events now so retained targets cannot keep stale accepted
                // configuration while the caller decides whether to exit or
                // restart the presentation backend.
                self.drain_surface_lifecycle_events()?;
                return Err(ShellRunError::Presentation(error));
            }
        };
        self.pending_wayland_events.extend(events);

        for _ in 0..MAX_WAYLAND_EVENTS_PER_FRAME {
            let Some(event) = self.pending_wayland_events.pop_front() else {
                break;
            };
            if let WindowEvent::KeyboardEnter { surface_id } = &event {
                self.follow_compositor_keyboard_focus(surface_id);
                continue;
            }

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
            // A key that opens a surface which takes the keyboard moves
            // routing between its press and its release. Delivering that
            // release to the new surface would activate whatever it focuses,
            // so a release goes to the surface that saw the press.
            let release_surface = match &event {
                RoutedWindowEvent::KeyReleased { key } => self
                    .held_key_surfaces
                    .remove(key)
                    .filter(|surface_id| self.component_index_for_surface(surface_id).is_some()),
                _ => None,
            };
            let route_surface_id = if let Some(surface_id) = release_surface.as_deref() {
                surface_id
            } else if is_keyboard_event {
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

            if let RoutedWindowEvent::KeyPressed { key, .. } = &event {
                tracing::debug!(
                    physical = &*physical_surface_id,
                    routed = route_surface_id,
                    key,
                    "[focus] key routed"
                );
                self.held_key_surfaces
                    .insert(key.clone(), route_surface_id.to_string());
            }

            let Some((index, target)) = self.component_target_for_surface(route_surface_id) else {
                tracing::debug!(
                    "[hover] dispatch_wayland: no component target for surface_id={route_surface_id}"
                );
                continue;
            };
            if self.component_is_quarantined(index) {
                continue;
            }
            // A tooltip popup commits an empty input region; anything a
            // compositor still routes to it belongs to nothing.
            if let TargetRef::Child(child_index) = target
                && self.components[index].children[child_index].kind
                    == crate::shell::types::ChildSurfaceKind::Tooltip
            {
                continue;
            }

            let target_surface_id = route_surface_id;
            let target_is_popover = match target {
                TargetRef::Parent => self.components[index].parent.popup_parent_surface.is_some(),
                TargetRef::Child(child_index) => {
                    self.components[index].children[child_index].kind
                        == crate::shell::types::ChildSurfaceKind::Popover
                }
            };
            if let RoutedWindowEvent::PointerButtonWithIdentity {
                button,
                identity,
                pressed: true,
                ..
            } = &event
                && *button == mesh_core_presentation::PRIMARY_POINTER_BUTTON
            {
                self.remember_popup_grab(target_surface_id, *identity);
            }
            if let RoutedWindowEvent::PointerButton {
                button,
                pressed: false,
                ..
            }
            | RoutedWindowEvent::PointerButtonWithIdentity {
                button,
                pressed: false,
                ..
            } = &event
                && *button == mesh_core_presentation::PRIMARY_POINTER_BUTTON
            {
                self.release_popup_grab(target_surface_id);
            }
            if matches!(
                event,
                RoutedWindowEvent::PointerMove { .. }
                    | RoutedWindowEvent::PointerButton { .. }
                    | RoutedWindowEvent::PointerButtonWithIdentity { .. }
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
            } else if matches!(event, RoutedWindowEvent::PointerLeave) && target_is_popover {
                self.enqueue_effects(std::iter::once(CoreRequest::HidePopover {
                    surface_id: target_surface_id.to_string(),
                    defer_for_hover_bridge: true,
                }));
            } else if matches!(event, RoutedWindowEvent::PointerLeave)
                && matches!(target, TargetRef::Parent)
            {
                self.defer_child_popover_hides_for_parent(target_surface_id);
            }
            let Some(surface) = self.surfaces.get(target_surface_id) else {
                continue;
            };
            let fixed_surface_size = if matches!(target, TargetRef::Child(child_index)
                if self.components[index].children[child_index].kind
                    == crate::shell::types::ChildSurfaceKind::Window)
            {
                self.presentation_engine
                    .window_configured_size(target_surface_id)
            } else if surface.width == 0 || surface.height == 0 {
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
                        .map(|buffer| (buffer.width().max(1), buffer.height().max(1)))
                })
                .or_else(|| {
                    self.presentation_engine
                        .surface_size_if_known(target_surface_id)
                        .map(|size| self.content_size_for_target(index, target, size))
                })
                .unwrap_or((1, 1));
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
            // A spanning layer surface reports width 0 in the shell-side
            // surface record, while its compositor buffer can be larger than
            // the component content because it includes tooltip padding. Do
            // not let pointer input cache that padded buffer as the parent's
            // layout size: the following interaction repaint would lay the
            // navigation tree out against the wrong geometry. Child popup
            // sizes remain local to their own target.
            self.components[index].target_mut(target).known_surface_size =
                Some(if matches!(target, TargetRef::Parent) {
                    component_surface_size
                } else {
                    target_surface_size
                });

            if let RoutedWindowEvent::KeyPressed { key, mods } = &event {
                if let Some(request) = shell_global_shortcut_request(key, mods.ctrl, mods.shift) {
                    self.enqueue_effects(std::iter::once(request));
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
                        ..
                    } => Some((*x, *y, true)),
                    RoutedWindowEvent::PointerButtonWithIdentity {
                        x,
                        y,
                        pressed: true,
                        ..
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
                    self.invalidate_debug_snapshot_cache();
                    self.debug.inspected_element = Some(inspected);
                }
                if selected {
                    self.debug.element_picker_enabled = false;
                }
                self.invalidate_debug_layout_bounds_targets();
                continue;
            }

            // A compositor gives an exclusive layer every pointer event, even
            // ones outside it, so a press beyond its bounds is the user
            // clicking away: dismiss it instead of delivering a click that
            // hits nothing.
            if matches!(target, TargetRef::Parent)
                && let RoutedWindowEvent::PointerButton { x, y, button, pressed: true }
                | RoutedWindowEvent::PointerButtonWithIdentity { x, y, button, pressed: true, .. } =
                    &event
                && *button == mesh_core_presentation::PRIMARY_POINTER_BUTTON
                && self.components[index].component.surface_keyboard_mode()
                    == mesh_core_wayland::KeyboardMode::Exclusive
                && (*x < 0.0
                    || *y < 0.0
                    || *x >= component_surface_size.0 as f32
                    || *y >= component_surface_size.1 as f32)
            {
                self.enqueue_effects(std::iter::once(CoreRequest::HideSurface {
                    surface_id: target_surface_id.to_string(),
                }));
                continue;
            }

            let is_user_gesture = match &event {
                RoutedWindowEvent::PointerButton { button, pressed: true, .. }
                | RoutedWindowEvent::PointerButtonWithIdentity { button, pressed: true, .. } => {
                    *button == mesh_core_presentation::PRIMARY_POINTER_BUTTON
                }
                RoutedWindowEvent::KeyPressed { .. } | RoutedWindowEvent::TouchDown { .. } => true,
                _ => false,
            };
            if is_user_gesture {
                self.user_activation = Some(crate::shell::runtime::UserActivation {
                    source: crate::shell::runtime::UserActivationSource::Surface(
                        self.components[index].surface_id.clone(),
                    ),
                    at: std::time::Instant::now(),
                });
            }

            let input = match event {
                RoutedWindowEvent::PointerButtonWithIdentity {
                    x,
                    y,
                    button,
                    pressed: true,
                    ..
                } if button == mesh_core_presentation::PRIMARY_POINTER_BUTTON => {
                    let focus_surface_id =
                        child_focus_owner_surface(self, index, target, target_surface_id);
                    self.claim_keyboard_focus_for_surface(&focus_surface_id);
                    ComponentInput::PointerButton {
                        x,
                        y,
                        button,
                        pressed: true,
                    }
                }
                RoutedWindowEvent::PointerButton {
                    x,
                    y,
                    button,
                    pressed: true,
                } if button == mesh_core_presentation::PRIMARY_POINTER_BUTTON => {
                    let focus_surface_id =
                        child_focus_owner_surface(self, index, target, target_surface_id);
                    self.claim_keyboard_focus_for_surface(&focus_surface_id);
                    ComponentInput::PointerButton {
                        x,
                        y,
                        button,
                        pressed: true,
                    }
                }
                RoutedWindowEvent::PointerMove { x, y } => ComponentInput::PointerMove { x, y },
                RoutedWindowEvent::PointerLeave => ComponentInput::PointerLeave,
                RoutedWindowEvent::PointerButtonWithIdentity {
                    x,
                    y,
                    button,
                    pressed,
                    ..
                } => ComponentInput::PointerButton {
                    x,
                    y,
                    button,
                    pressed,
                },
                RoutedWindowEvent::PointerButton {
                    x,
                    y,
                    button,
                    pressed,
                } => ComponentInput::PointerButton {
                    x,
                    y,
                    button,
                    pressed,
                },
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
                RoutedWindowEvent::TextInput { text } => ComponentInput::TextInput { text },
                RoutedWindowEvent::TextDelete {
                    before_bytes,
                    after_bytes,
                } => ComponentInput::TextDelete {
                    before_bytes,
                    after_bytes,
                },
                RoutedWindowEvent::TextInputEdit {
                    preedit_present,
                    preedit,
                    preedit_cursor_begin,
                    preedit_cursor_end,
                    commit,
                    before_bytes,
                    after_bytes,
                } => ComponentInput::TextInputEdit {
                    preedit_present,
                    preedit,
                    preedit_cursor_begin,
                    preedit_cursor_end,
                    commit,
                    before_bytes,
                    after_bytes,
                },
            };
            tracing::trace!(
                "[hover] dispatch_wayland: routing event to surface_id={}",
                target_surface_id
            );
            let emitted = {
                let runtime = &mut self.components[index];
                // A child xdg_popup has its own surface dimensions. Feeding
                // those dimensions into the shared component runtime resizes
                // the parent tree to the popup-sized buffer until the next
                // parent frame restores the real size. Only parent-surface
                // input may update the component's authoritative layout size.
                if matches!(target, TargetRef::Parent)
                    && runtime.component.content_input_size() != Some(component_surface_size)
                {
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
                    ),
                    TargetRef::Child(child_index) => {
                        let node_key = runtime.children[child_index].node_key.clone();
                        let content_padding = runtime.children[child_index].content_padding;
                        runtime.component.handle_child_surface_input(
                            &node_key,
                            self.theme.active(),
                            component_surface_size.0,
                            component_surface_size.1,
                            (content_padding.0 as f32, content_padding.1 as f32),
                            input,
                        )
                    }
                };
                let interactive = runtime.component.hovered_target_is_interactive();
                self.presentation_engine
                    .set_pointer_interactive(interactive);
                emitted
            };
            let emitted = match emitted {
                Ok(emitted) => emitted,
                Err(error) => {
                    self.contain_component_failure(index, "callback", &error);
                    continue;
                }
            };

            self.sync_text_input_state()?;

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
                let origin = self.components[index].effect_origin();
                self.enqueue_effects(origin.attribute(emitted));
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

impl Shell {
    /// Key routing follows the compositor whenever it moves keyboard focus:
    /// a surface it focuses on map, by click or by a focus grab owns the
    /// keys from then on, until the shell itself transfers them (Tab into a
    /// popover) or the compositor moves focus again.
    fn follow_compositor_keyboard_focus(&mut self, surface_id: &str) {
        let Some((index, target)) = self.component_target_for_surface(surface_id) else {
            return;
        };
        if let TargetRef::Child(child_index) = target
            && self.components[index].children[child_index].kind
                == crate::shell::types::ChildSurfaceKind::Tooltip
        {
            return;
        }
        let owner = child_focus_owner_surface(self, index, target, surface_id);
        if self.keyboard_focus_surface.as_deref() != Some(owner.as_str()) {
            tracing::debug!(surface_id = owner, "[focus] key routing follows compositor focus");
            self.claim_keyboard_focus_for_surface(&owner);
        }
    }
}

fn child_focus_owner_surface(
    shell: &Shell,
    index: usize,
    target: TargetRef,
    target_surface_id: &str,
) -> String {
    match target {
        TargetRef::Child(child_index)
            if shell.components[index].children[child_index].kind
                == crate::shell::types::ChildSurfaceKind::Overflow =>
        {
            // Overflow is a geometry-owned view of the parent, not an
            // independent focus surface. Keep keyboard delivery on the
            // parent after a click while the child handler still receives the
            // pointer event that established the component's focused node.
            shell.components[index].surface_id.clone()
        }
        TargetRef::Parent | TargetRef::Child(_) => target_surface_id.to_string(),
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
    let source_file = node.source_file().unwrap_or(fallback_source);
    let source_tag = mesh_core_interaction::source_element_tag(node);
    serde_json::json!({
        "surface_id": surface_id,
        "key": node.mesh_key().unwrap_or(""),
        "tag": source_tag,
        "runtime_tag": node.tag,
        "module_id": node.module_id().unwrap_or(""),
        "source_file": source_file,
        "source_line": 1,
        "id": node.attributes.get(Prop::Id).cloned().unwrap_or_default(),
        "classes": node.attributes.get(Prop::Class).cloned().unwrap_or_default(),
        "content": node.attributes.get(Prop::Content).cloned().unwrap_or_default(),
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
    let id = node
        .attributes
        .get(Prop::Id)
        .filter(|value| !value.is_empty());
    let classes = node
        .attributes
        .get(Prop::Class)
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
        button: u32,
        pressed: bool,
    },
    PointerButtonWithIdentity {
        x: f32,
        y: f32,
        button: u32,
        pressed: bool,
        identity: mesh_core_presentation::PointerButtonIdentity,
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
    TextInput {
        text: String,
    },
    TextDelete {
        before_bytes: usize,
        after_bytes: usize,
    },
    TextInputEdit {
        preedit_present: bool,
        preedit: Option<String>,
        preedit_cursor_begin: i32,
        preedit_cursor_end: i32,
        commit: Option<String>,
        before_bytes: usize,
        after_bytes: usize,
    },
}

impl RoutedWindowEvent {
    fn is_keyboard(&self) -> bool {
        matches!(
            self,
            Self::KeyPressed { .. }
                | Self::KeyReleased { .. }
                | Self::Char { .. }
                | Self::TextInput { .. }
                | Self::TextDelete { .. }
                | Self::TextInputEdit { .. }
        )
    }
}

fn split_window_event(event: WindowEvent) -> (std::sync::Arc<str>, RoutedWindowEvent) {
    match event {
        WindowEvent::PointerMove { surface_id, x, y } => {
            (surface_id, RoutedWindowEvent::PointerMove { x, y })
        }
        WindowEvent::PointerLeave { surface_id } => (surface_id, RoutedWindowEvent::PointerLeave),
        WindowEvent::KeyboardEnter { .. } => {
            unreachable!("keyboard enter is consumed before routing")
        }
        WindowEvent::PointerButton {
            surface_id,
            x,
            y,
            button,
            pressed,
        } => (
            surface_id,
            RoutedWindowEvent::PointerButton {
                x,
                y,
                button,
                pressed,
            },
        ),
        WindowEvent::PointerButtonWithIdentity {
            surface_id,
            x,
            y,
            button,
            pressed,
            identity,
        } => (
            surface_id,
            RoutedWindowEvent::PointerButtonWithIdentity {
                x,
                y,
                button,
                pressed,
                identity,
            },
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
        WindowEvent::TextInput { surface_id, text } => (
            surface_id,
            RoutedWindowEvent::TextInput {
                text: text.to_string(),
            },
        ),
        WindowEvent::TextDelete {
            surface_id,
            before_bytes,
            after_bytes,
        } => (
            surface_id,
            RoutedWindowEvent::TextDelete {
                before_bytes,
                after_bytes,
            },
        ),
        WindowEvent::TextInputEdit {
            surface_id,
            preedit_present,
            preedit,
            preedit_cursor_begin,
            preedit_cursor_end,
            commit,
            before_bytes,
            after_bytes,
        } => (
            surface_id,
            RoutedWindowEvent::TextInputEdit {
                preedit_present,
                preedit: preedit.map(|text| text.to_string()),
                preedit_cursor_begin,
                preedit_cursor_end,
                commit: commit.map(|text| text.to_string()),
                before_bytes,
                after_bytes,
            },
        ),
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
        WindowEvent::KeyboardEnter { .. } => "keyboard_enter",
        WindowEvent::PointerButton { .. } => "pointer_button",
        WindowEvent::PointerButtonWithIdentity { .. } => "pointer_button",
        WindowEvent::Scroll { .. } => "scroll",
        WindowEvent::TwoFingerScroll { .. } => "two_finger_scroll",
        WindowEvent::Key { .. } => "key",
        WindowEvent::Char { .. } => "char",
        WindowEvent::TextInput { .. } => "text_input",
        WindowEvent::TextDelete { .. } => "text_delete",
        WindowEvent::TextInputEdit { .. } => "text_input_edit",
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

fn take_pending_popup_grab_at(
    pending: &mut Option<PendingPopupGrab>,
    parent_surface_id: &str,
    now: std::time::Instant,
) -> Option<mesh_core_presentation::PointerButtonIdentity> {
    if pending
        .as_ref()
        .is_none_or(|pending| pending.surface_id != parent_surface_id)
    {
        return None;
    }
    let pending = pending.take()?;
    let live = pending.released_at.is_none_or(|released_at| {
        now.saturating_duration_since(released_at) <= POPUP_GRAB_CLICK_WINDOW
    });
    live.then_some(pending.identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_window_event_preserves_surface_and_payload() {
        let (surface_id, event) = split_window_event(WindowEvent::PointerButton {
            surface_id: "panel".into(),
            x: 12.0,
            y: 24.0,
            button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
            pressed: true,
        });
        assert_eq!(surface_id.as_ref(), "panel");
        assert!(matches!(
            event,
            RoutedWindowEvent::PointerButton {
                x: 12.0,
                y: 24.0,
                button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
                pressed: true
            }
        ));

        let identity = mesh_core_presentation::PointerButtonIdentity {
            seat_id: 7,
            serial: 42,
        };
        let (surface_id, event) = split_window_event(WindowEvent::PointerButtonWithIdentity {
            surface_id: "panel".into(),
            x: 12.0,
            y: 24.0,
            button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
            pressed: true,
            identity,
        });
        assert_eq!(surface_id.as_ref(), "panel");
        assert!(matches!(
            event,
            RoutedWindowEvent::PointerButtonWithIdentity {
                x: 12.0,
                y: 24.0,
                button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
                pressed: true,
                identity: actual,
            } if actual == identity
        ));

        let (surface_id, event) = split_window_event(WindowEvent::PointerButton {
            surface_id: "panel".into(),
            x: 4.0,
            y: 5.0,
            button: 0x111,
            pressed: false,
        });
        assert_eq!(surface_id.as_ref(), "panel");
        assert!(matches!(
            event,
            RoutedWindowEvent::PointerButton {
                x: 4.0,
                y: 5.0,
                button: 0x111,
                pressed: false,
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

        let (surface_id, event) = split_window_event(WindowEvent::TextInput {
            surface_id: "launcher".into(),
            text: "A🙂B".into(),
        });
        assert_eq!(surface_id.as_ref(), "launcher");
        assert!(event.is_keyboard());
        assert!(matches!(
            event,
            RoutedWindowEvent::TextInput { ref text } if text == "A🙂B"
        ));

        let (surface_id, event) = split_window_event(WindowEvent::TextDelete {
            surface_id: "launcher".into(),
            before_bytes: 5,
            after_bytes: 4,
        });
        assert_eq!(surface_id.as_ref(), "launcher");
        assert!(event.is_keyboard());
        assert!(matches!(
            event,
            RoutedWindowEvent::TextDelete {
                before_bytes: 5,
                after_bytes: 4,
            }
        ));

        let (surface_id, event) = split_window_event(WindowEvent::TextInputEdit {
            surface_id: "launcher".into(),
            preedit_present: true,
            preedit: Some("候".into()),
            preedit_cursor_begin: 0,
            preedit_cursor_end: 3,
            commit: Some("候".into()),
            before_bytes: 4,
            after_bytes: 1,
        });
        assert_eq!(surface_id.as_ref(), "launcher");
        assert!(event.is_keyboard());
        assert!(matches!(
            event,
            RoutedWindowEvent::TextInputEdit {
                preedit_present: true,
                ref preedit,
                preedit_cursor_begin: 0,
                preedit_cursor_end: 3,
                ref commit,
                before_bytes: 4,
                after_bytes: 1,
            } if preedit.as_deref() == Some("候") && commit.as_deref() == Some("候")
        ));
    }

    #[test]
    fn popup_grab_identity_is_one_shot_and_bound_to_its_click() {
        let identity = mesh_core_presentation::PointerButtonIdentity {
            seat_id: 3,
            serial: 19,
        };
        let pressed = |released_at| {
            Some(PendingPopupGrab {
                surface_id: "panel".into(),
                identity,
                released_at,
            })
        };
        let now = std::time::Instant::now();

        let mut pending = pressed(None);
        assert_eq!(
            take_pending_popup_grab_at(&mut pending, "other", now),
            None,
            "a press on one surface cannot authorize another surface's popup"
        );
        assert_eq!(
            take_pending_popup_grab_at(&mut pending, "panel", now),
            Some(identity),
            "a held press authorizes a popup created on a later frame"
        );
        assert_eq!(
            take_pending_popup_grab_at(&mut pending, "panel", now),
            None,
            "a popup creation consumes its click credential"
        );

        let mut pending = pressed(Some(now));
        assert_eq!(
            take_pending_popup_grab_at(&mut pending, "panel", now + POPUP_GRAB_CLICK_WINDOW),
            Some(identity),
            "a click-opened popup staged for a frame keeps its credential"
        );

        let mut pending = pressed(Some(now));
        assert_eq!(
            take_pending_popup_grab_at(
                &mut pending,
                "panel",
                now + POPUP_GRAB_CLICK_WINDOW + std::time::Duration::from_millis(1),
            ),
            None,
            "a stale click cannot authorize a later popup"
        );
        assert!(pending.is_none());
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
        node.attributes.insert(Prop::DataMeshElement, "row".into());
        node.attributes.insert(Prop::Class, "right-cluster".into());

        assert_eq!(
            source_line_for_node(file.path().to_str().unwrap(), &node),
            Some(3)
        );
    }
}
