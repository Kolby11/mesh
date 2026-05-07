use super::*;

mod focus;
mod keyboard;
mod widgets;

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
                        self.active_slider_key = None;
                        self.begin_text_selection(selection_key, x, y);
                        self.dirty = true;
                        return Ok(requests);
                    }

                    self.clear_selection();
                    if let Some(node_key) = self.pointer_event_target_key(&tree, x, y) {
                        self.pointer_down_key = Some(node_key.clone());
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

                        self.dirty = true;
                        if !requests.is_empty() {
                            return Ok(requests);
                        }
                    } else {
                        let requests = self.set_focus_target(&tree, None, false)?;
                        self.pointer_down_key = None;
                        self.active_slider_key = None;
                        self.dirty = true;
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
                    }

                    self.end_text_selection_drag();

                    if self.selection.is_some() && self.pointer_down_key.is_none() {
                        self.dirty = true;
                        return Ok(requests);
                    }

                    if let Some(node_key) = self.pointer_event_target_key(&tree, x, y) {
                        if self.pointer_down_key.as_deref() == Some(node_key.as_str()) {
                            if let Some(handler) = find_click_handler(&tree, &node_key) {
                                let click_event = self.build_click_event(&tree, &node_key, x, y);
                                requests.extend(
                                    self.call_namespaced_handler(&handler, &[click_event])?,
                                );
                            }
                        }
                    }
                    self.pointer_down_key = None;
                    self.active_slider_key = None;
                    self.dirty = true;
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
                    self.dirty = true;
                    if !requests.is_empty() {
                        return Ok(requests);
                    }
                }

                if self.selection.is_some() {
                    self.update_text_selection_focus(x, y);
                    self.dirty = true;
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
                    self.hovered_key = new_key.clone();
                    self.hovered_path = new_path;
                    // Only start the tooltip timer when hovering a node with tooltip content.
                    self.hover_start = new_key
                        .as_ref()
                        .and_then(|k| find_node_by_key(&tree, k))
                        .and_then(|n| node_tooltip_text(n))
                        .map(|_| std::time::Instant::now());
                    self.dirty = true;
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
                            self.dirty = true;
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
                        self.dirty = true;
                        return self.call_node_handler(
                            &tree,
                            &focused_key,
                            "change",
                            &[serde_json::json!(current)],
                        );
                    }
                }
            }
            ComponentInput::KeyPressed { key, modifiers } => {
                let keyboard_settings = self.current_keyboard_settings();
                if matches!(key.as_str(), "Tab") && !modifiers.ctrl && !modifiers.alt {
                    self.clear_selection();
                    self.dirty = true;
                    return self.advance_keyboard_focus(&tree, modifiers.shift);
                }
                if modifiers.ctrl
                    && key.eq_ignore_ascii_case("c")
                    && let Some(text) = self.selection_copy_payload(&tree)
                {
                    return Ok(vec![CoreRequest::WriteClipboard { text }]);
                }
                if let Some(requests) =
                    self.dispatch_surface_shortcut(&tree, &key, modifiers, &keyboard_settings)?
                {
                    return Ok(requests);
                }
                self.focus_visible_key = self.focused_key.clone();
                if let Some(focused_key) = self.normalized_focused_key(&tree) {
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
                                self.dirty = true;
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
                            self.slider_values.insert(focused_key.clone(), value);
                            self.dirty = true;
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
                        self.dirty = true;
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
