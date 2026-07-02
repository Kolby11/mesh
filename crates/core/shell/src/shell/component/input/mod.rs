use super::*;

mod focus;
mod keyboard;
mod widgets;

#[cfg(test)]
pub(crate) use keyboard::KeybindResolutionSource;

fn point_in_bounds(x: f32, y: f32, (left, top, right, bottom): (f32, f32, f32, f32)) -> bool {
    x >= left && x <= right && y >= top && y <= bottom
}

impl FrontendSurfaceComponent {
    pub(in crate::shell::component) fn handle_component_input(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        tracing::trace!(
            "[hover] handle_input called: id={} visible={} input={:?}",
            self.id(),
            self.visible,
            std::mem::discriminant(&input)
        );
        if !self.visible {
            return Ok(Vec::new());
        }

        let tree = self
            .last_tree
            .clone()
            .unwrap_or_else(|| self.build_tree(theme, width, height));

        match input {
            ComponentInput::PointerButton { x, y, pressed } => {
                if pressed {
                    if let Some(selection_key) = self.selectable_text_target_key(&tree, x, y) {
                        let requests = self.set_focus_target(&tree, None, false)?;
                        self.pointer_down_key = None;
                        self.pointer_down_bounds = None;
                        self.active_slider_key = None;
                        self.begin_text_selection(selection_key, x, y);
                        self.invalidate_paint();
                        return Ok(requests);
                    }

                    self.clear_selection();
                    if let Some(node_key) = self.pointer_event_target_key(&tree, x, y) {
                        self.pointer_down_key = Some(node_key.clone());
                        self.pointer_down_bounds =
                            find_node_bounds_by_key(&tree, &node_key, 0.0, 0.0);
                        let mut requests = if let Some(focused_key) = find_focusable_at(&tree, x, y)
                        {
                            let focus_visible =
                                self.pointer_focus_visible_for_key(&tree, &focused_key);
                            self.set_focus_target(&tree, Some(focused_key), focus_visible)?
                        } else {
                            self.set_focus_target(&tree, None, false)?
                        };

                        if is_slider_key(&tree, &node_key) {
                            self.active_slider_key = Some(node_key.clone());
                            self.update_slider_from_position(&tree, &node_key, x, y);
                            if let Some(value) = self.slider_value(&tree, &node_key) {
                                requests.extend(self.call_node_handler(
                                    &tree,
                                    &node_key,
                                    "change",
                                    &[serde_json::json!(value)],
                                )?);
                            }
                            self.invalidate_script_state();
                        } else {
                            self.active_slider_key = None;
                            if self.is_option_key(&tree, &node_key) {
                                requests.extend(self.activate_option_choice(&tree, &node_key)?);
                            } else if self.is_radio_key(&tree, &node_key) {
                                requests.extend(self.activate_radio_choice(&tree, &node_key)?);
                            } else if self.is_checkable_choice_key(&tree, &node_key) {
                                let value = self.toggle_checked_value(&tree, &node_key);
                                requests.extend(self.call_node_handler(
                                    &tree,
                                    &node_key,
                                    "change",
                                    &[serde_json::json!(value)],
                                )?);
                            }
                        }

                        self.invalidate_interaction_restyle();
                        if !requests.is_empty() {
                            return Ok(requests);
                        }
                    } else {
                        let requests = self.set_focus_target(&tree, None, false)?;
                        self.pointer_down_key = None;
                        self.pointer_down_bounds = None;
                        self.active_slider_key = None;
                        self.invalidate_interaction_restyle();
                        if !requests.is_empty() {
                            return Ok(requests);
                        }
                    }
                } else {
                    let mut requests = Vec::new();
                    if let Some(slider_key) = self.active_slider_key.clone()
                        && let Some(value) = self.slider_value(&tree, &slider_key)
                    {
                        requests.extend(self.call_node_handler(
                            &tree,
                            &slider_key,
                            "release",
                            &[serde_json::json!(value)],
                        )?);
                        self.invalidate_script_state();
                    }

                    self.end_text_selection_drag();

                    if self.selection.is_some() && self.pointer_down_key.is_none() {
                        self.invalidate_paint();
                        return Ok(requests);
                    }

                    let release_key = self.pointer_event_target_key(&tree, x, y);
                    let captured_click_key = self.pointer_down_key.as_ref().and_then(|down_key| {
                        let released_on_same_key =
                            release_key.as_deref() == Some(down_key.as_str());
                        let released_inside_press_bounds = self
                            .pointer_down_bounds
                            .is_some_and(|bounds| point_in_bounds(x, y, bounds));
                        (released_on_same_key || released_inside_press_bounds)
                            .then_some(down_key.clone())
                    });
                    if let Some(node_key) = captured_click_key {
                        if self.is_menu_item_key(&tree, &node_key)
                            || self.is_container_collection_item_key(&tree, &node_key)
                        {
                            let click_event = self.build_click_event(&tree, &node_key, x, y);
                            requests.extend(self.dispatch_activation_handlers(
                                &tree,
                                &node_key,
                                click_event,
                            )?);
                        } else if let Some(handler) = find_click_handler(&tree, &node_key) {
                            let click_event = self.build_click_event(&tree, &node_key, x, y);
                            requests
                                .extend(self.call_namespaced_handler(&handler, &[click_event])?);
                        }
                    }
                    self.pointer_down_key = None;
                    self.pointer_down_bounds = None;
                    self.active_slider_key = None;
                    self.invalidate_interaction_restyle();
                    if !requests.is_empty() {
                        return Ok(requests);
                    }
                }
            }
            ComponentInput::PointerMove { x, y } => {
                if let Some(slider_key) = self.active_slider_key.clone() {
                    self.update_slider_from_position(&tree, &slider_key, x, y);
                    let mut requests = Vec::new();
                    if let Some(value) = self.slider_value(&tree, &slider_key) {
                        requests.extend(self.call_node_handler(
                            &tree,
                            &slider_key,
                            "change",
                            &[serde_json::json!(value)],
                        )?);
                    }
                    // Slider drag must always trigger a full repaint: the knob
                    // moved (slider_values mutated above) and the change handler
                    // typically writes reactive globals (e.g. percent label).
                    // Force a script-state rebuild + full surface repaint
                    // unconditionally; relying on state_dirty detection misses
                    // intermediate frames and the selective-damage path can
                    // skip presents when only text content differs.
                    self.invalidate_script_state();
                    if !requests.is_empty() {
                        return Ok(requests);
                    }
                }

                if self.selection.is_some() {
                    self.update_text_selection_focus(x, y);
                    self.invalidate_paint();
                }

                // Update hover state for CSS :hover and the tooltip system.
                self.hovered_pos = (x, y);
                let new_path = find_node_path_at(&tree, x, y).unwrap_or_default();
                let new_key = new_path.last().cloned();
                tracing::trace!(
                    "[hover] pointer=({x:.1},{y:.1}) path={:?} hit={:?} prev={:?}",
                    new_path,
                    new_key,
                    self.hovered_key
                );
                if new_key != self.hovered_key || new_path != self.hovered_path {
                    let previous_path = self.hovered_path.clone();
                    let previous_tooltip = self
                        .hovered_key
                        .as_ref()
                        .and_then(|key| find_tooltip_by_key(&tree, key));
                    let next_tooltip = new_key
                        .as_ref()
                        .and_then(|key| find_tooltip_by_key(&tree, key));
                    let same_tooltip_owner = previous_tooltip
                        .as_ref()
                        .zip(next_tooltip.as_ref())
                        .is_some_and(|((previous_owner, _), (next_owner, _))| {
                            previous_owner == next_owner
                        });
                    self.hovered_key = new_key.clone();
                    self.hovered_path = new_path.clone();
                    // Store the hovered element's bounds for tooltip positioning.
                    // Use the tooltip owner's bounds when available; fall back to
                    // the hovered node itself.
                    let tooltip_owner_key = next_tooltip.as_ref().map(|(owner, _)| owner.as_str());
                    let bounds_key = tooltip_owner_key.or(new_key.as_deref());
                    self.hovered_element_bounds =
                        bounds_key.and_then(|k| find_node_bounds_by_key(&tree, k, 0.0, 0.0));
                    // Preserve an already-running tooltip when moving between a
                    // tooltip owner and descendants that inherit that tooltip.
                    if same_tooltip_owner {
                        if self.hover_start.is_none() {
                            self.hover_start = Some(std::time::Instant::now());
                            self.tooltip_visible = false;
                        }
                    } else {
                        self.hover_start = next_tooltip.map(|_| std::time::Instant::now());
                        self.tooltip_visible = false;
                        self.tooltip_appeared_at = None;
                    }
                    // Hover changes don't mutate script state — flag the surface
                    // for a style-only repaint so paint() can reuse the cached
                    // widget tree instead of re-running Luau scripts.
                    self.invalidate_interaction_restyle();
                    // Dispatch pointerenter/pointerleave to any script handlers on
                    // the entered/left nodes (e.g. hover-to-open popovers).
                    let hover_requests = self.dispatch_hover_transition_handlers(
                        &tree,
                        &previous_path,
                        &new_path,
                        x,
                        y,
                    )?;
                    if !hover_requests.is_empty() {
                        return Ok(hover_requests);
                    }
                }
            }
            ComponentInput::PointerLeave => {
                let previous_path = self.hovered_path.clone();
                if self.hovered_key.is_some()
                    || !self.hovered_path.is_empty()
                    || self.hover_start.is_some()
                {
                    self.hovered_key = None;
                    self.hovered_path.clear();
                    self.hover_start = None;
                    self.tooltip_visible = false;
                    self.hovered_element_bounds = None;
                    self.tooltip_appeared_at = None;
                    self.invalidate_interaction_restyle();
                }
                // The pointer left the whole surface — fire pointerleave/mouseleave
                // on everything that was hovered so popovers can close themselves.
                let leave_requests =
                    self.dispatch_hover_transition_handlers(&tree, &previous_path, &[], 0.0, 0.0)?;
                if !leave_requests.is_empty() {
                    return Ok(leave_requests);
                }
            }
            ComponentInput::Scroll { x, y, dx, dy } => {
                if let Some(requests) = self.dispatch_scroll_handler(&tree, x, y, dx, dy)? {
                    return Ok(requests);
                }

                if let Some(scroll_key) = find_scrollable_at(&tree, x, y) {
                    if let Some(node) = find_node_by_key(&tree, &scroll_key) {
                        let (max_x, max_y) = scroll_limits(node);
                        let current = self.scroll_offsets.entry(scroll_key).or_default();
                        let next_x = (current.x - dx * 28.0).clamp(0.0, max_x);
                        let next_y = (current.y - dy * 28.0).clamp(0.0, max_y);
                        if (next_x - current.x).abs() > f32::EPSILON
                            || (next_y - current.y).abs() > f32::EPSILON
                        {
                            current.x = next_x;
                            current.y = next_y;
                            self.invalidate(
                                ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS,
                            );
                        }
                    }
                }
            }
            ComponentInput::Char { ch } => {
                if let Some(focused_key) = self.focused_key.clone() {
                    let accepts_char = find_node_by_key(&tree, &focused_key)
                        .is_some_and(|node| input_accepts_char(node, ch));
                    if is_input_key(&tree, &focused_key) && accepts_char {
                        self.clear_selection();
                        let value = self.input_values.entry(focused_key.clone()).or_default();
                        value.push(ch);
                        let current = value.clone();
                        self.invalidate_text_state();
                        return self.dispatch_text_input_value_handlers(
                            &tree,
                            &focused_key,
                            &current,
                        );
                    }
                }

                let keyboard_settings = self.current_keyboard_settings();
                let key = ch.to_string();
                if let Some(requests) = self.dispatch_surface_shortcut(
                    &tree,
                    &key,
                    KeyModifiers::default(),
                    &keyboard_settings,
                )? {
                    return Ok(requests);
                }
            }
            ComponentInput::KeyPressed { key, modifiers } => {
                return self.handle_key_pressed(&tree, key, modifiers);
            }
            ComponentInput::KeyReleased { key, modifiers } => {
                return self.handle_key_released(&tree, key, modifiers);
            }
        }

        Ok(Vec::new())
    }

    /// Dispatch `pointerenter`/`pointerleave` (plus the `mouseenter`/`mouseleave`
    /// aliases) script handlers for the delta between the previously hovered
    /// node path and the new one: nodes only in `previous_path` get leave
    /// handlers, nodes only in `new_path` get enter handlers. The event payload
    /// mirrors a click event so handlers can read `event.current_target.position`
    /// to position popovers, exactly as the `onclick` open path does.
    fn dispatch_hover_transition_handlers(
        &mut self,
        tree: &WidgetNode,
        previous_path: &[String],
        new_path: &[String],
        x: f32,
        y: f32,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let mut requests = Vec::new();
        for key in previous_path {
            if new_path.contains(key) {
                continue;
            }
            if find_event_handler(tree, key, "pointerleave").is_some()
                || find_event_handler(tree, key, "mouseleave").is_some()
            {
                let event = self.build_click_event(tree, key, x, y);
                requests.extend(self.call_node_handler(
                    tree,
                    key,
                    "pointerleave",
                    &[event.clone()],
                )?);
                requests.extend(self.call_node_handler(tree, key, "mouseleave", &[event])?);
            }
        }
        for key in new_path {
            if previous_path.contains(key) {
                continue;
            }
            if find_event_handler(tree, key, "pointerenter").is_some()
                || find_event_handler(tree, key, "mouseenter").is_some()
            {
                let event = self.build_click_event(tree, key, x, y);
                requests.extend(self.call_node_handler(
                    tree,
                    key,
                    "pointerenter",
                    &[event.clone()],
                )?);
                requests.extend(self.call_node_handler(tree, key, "mouseenter", &[event])?);
            }
        }
        Ok(requests)
    }
}

pub(super) fn is_bare_printable_key(key: &str, modifiers: KeyModifiers) -> bool {
    !modifiers.ctrl
        && !modifiers.alt
        && key.chars().count() == 1
        && key.chars().all(|ch| !ch.is_control())
}
