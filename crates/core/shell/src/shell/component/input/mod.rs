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
                            if find_node_by_key(&tree, &node_key).is_some_and(|node| {
                                matches!(node.tag.as_str(), "switch" | "checkbox")
                            }) {
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
                        if let Some(handler) = find_click_handler(&tree, &node_key) {
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
                    self.hovered_path = new_path;
                    // Preserve an already-running tooltip when moving between a
                    // tooltip owner and descendants that inherit that tooltip.
                    if same_tooltip_owner {
                        if self.hover_start.is_none() {
                            self.hover_start = Some(std::time::Instant::now());
                        }
                    } else {
                        self.hover_start = next_tooltip.map(|_| std::time::Instant::now());
                    }
                    // Hover changes don't mutate script state — flag the surface
                    // for a style-only repaint so paint() can reuse the cached
                    // widget tree instead of re-running Luau scripts.
                    self.invalidate_interaction_restyle();
                }
            }
            ComponentInput::PointerLeave => {
                if self.hovered_key.is_some()
                    || !self.hovered_path.is_empty()
                    || self.hover_start.is_some()
                {
                    self.hovered_key = None;
                    self.hovered_path.clear();
                    self.hover_start = None;
                    self.invalidate_interaction_restyle();
                }
            }
            ComponentInput::Scroll { x, y, dx, dy } => {
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
                        return self.call_node_handler(
                            &tree,
                            &focused_key,
                            "change",
                            &[serde_json::json!(current)],
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
                let keyboard_settings = self.current_keyboard_settings();
                if matches!(key.as_str(), "Tab") && !modifiers.ctrl && !modifiers.alt {
                    self.clear_selection();
                    self.invalidate_interaction_restyle();
                    return self.handle_tab_with_cross_surface(&tree, modifiers.shift);
                }
                if matches!(key.as_str(), "Escape") && !modifiers.ctrl && !modifiers.alt {
                    if let Some(requests) = self.handle_escape_with_cross_surface()? {
                        self.clear_selection();
                        self.invalidate_interaction_restyle();
                        return Ok(requests);
                    }
                }
                if modifiers.ctrl
                    && key.eq_ignore_ascii_case("c")
                    && let Some(text) = self.selection_copy_payload(&tree)
                {
                    return Ok(vec![CoreRequest::WriteClipboard { text }]);
                }

                let focused_key = self.normalized_focused_key(&tree);
                let focused_text_input_has_bare_printable_key = focused_key
                    .as_deref()
                    .is_some_and(|focused_key| is_input_key(&tree, focused_key))
                    && is_bare_printable_key(&key, modifiers);
                if !focused_text_input_has_bare_printable_key {
                    if let Some(requests) =
                        self.dispatch_surface_shortcut(&tree, &key, modifiers, &keyboard_settings)?
                    {
                        return Ok(requests);
                    }
                }
                self.focus_visible_key = self.focused_key.clone();
                if let Some(focused_key) = focused_key {
                    let mut requests = self.dispatch_focused_keyboard_handler(
                        &tree,
                        &focused_key,
                        "keydown",
                        "keydown",
                        &key,
                        modifiers,
                    )?;
                    if is_input_key(&tree, &focused_key) {
                        self.clear_selection();
                        let value = self.input_values.entry(focused_key.clone()).or_default();
                        match key.as_str() {
                            "Backspace" => {
                                value.pop();
                                let current = value.clone();
                                self.invalidate_text_state();
                                requests.extend(self.call_node_handler(
                                    &tree,
                                    &focused_key,
                                    "change",
                                    &[serde_json::json!(current)],
                                )?);
                                return Ok(requests);
                            }
                            _ => {}
                        }
                    } else if (Self::key_matches_any_binding(
                        &key,
                        &keyboard_settings.slider_decrement_keys,
                    ) || Self::key_matches_any_binding(
                        &key,
                        &keyboard_settings.slider_increment_keys,
                    )) && is_slider_key(&tree, &focused_key)
                    {
                        self.clear_selection();
                        let delta = if Self::key_matches_any_binding(
                            &key,
                            &keyboard_settings.slider_decrement_keys,
                        ) {
                            -1.0
                        } else {
                            1.0
                        };
                        if let Some(value) = self.slider_step_value(&tree, &focused_key, delta) {
                            self.preserve_slider_value(&tree, &focused_key, value);
                            self.invalidate_interaction_restyle();
                            requests.extend(self.call_node_handler(
                                &tree,
                                &focused_key,
                                "change",
                                &[serde_json::json!(value)],
                            )?);
                            return Ok(requests);
                        }
                    }

                    if !requests.is_empty() {
                        return Ok(requests);
                    }
                }
            }
            ComponentInput::KeyReleased { key, modifiers } => {
                let keyboard_settings = self.current_keyboard_settings();
                self.focus_visible_key = self.focused_key.clone();
                if let Some(focused_key) = self.normalized_focused_key(&tree) {
                    let mut requests = self.dispatch_focused_keyboard_handler(
                        &tree,
                        &focused_key,
                        "keyup",
                        "keyup",
                        &key,
                        modifiers,
                    )?;

                    if find_node_by_key(&tree, &focused_key)
                        .is_some_and(|node| node.tag == "button")
                        && Self::key_matches_any_binding(
                            &key,
                            &keyboard_settings.button_activation_keys,
                        )
                    {
                        self.clear_selection();
                        requests.extend(self.dispatch_keyboard_button_activation(
                            &tree,
                            &focused_key,
                            &key,
                        )?);
                        return Ok(requests);
                    }

                    if Self::key_matches_any_binding(
                        &key,
                        &keyboard_settings.toggle_activation_keys,
                    ) && find_node_by_key(&tree, &focused_key)
                        .is_some_and(|node| matches!(node.tag.as_str(), "switch" | "checkbox"))
                    {
                        self.clear_selection();
                        let value = self.toggle_checked_value(&tree, &focused_key);
                        self.invalidate_interaction_restyle();
                        requests.extend(self.call_node_handler(
                            &tree,
                            &focused_key,
                            "change",
                            &[serde_json::json!(value)],
                        )?);
                        return Ok(requests);
                    }

                    if !requests.is_empty() {
                        return Ok(requests);
                    }
                }
            }
        }

        Ok(Vec::new())
    }
}

fn is_bare_printable_key(key: &str, modifiers: KeyModifiers) -> bool {
    !modifiers.ctrl
        && !modifiers.alt
        && key.chars().count() == 1
        && key.chars().all(|ch| !ch.is_control())
}
