use super::*;

impl FrontendSurfaceComponent {
    pub(super) fn update_slider_from_position(
        &mut self,
        tree: &WidgetNode,
        slider_key: &str,
        x: f32,
        y: f32,
    ) -> Option<CoreRequest> {
        let Some(node) = find_node_by_key(tree, slider_key) else {
            return None;
        };
        let action = node.attributes.get("mesh-action").cloned();
        let is_vertical = node
            .attributes
            .get("orient")
            .map(|v| v == "vertical")
            .unwrap_or(false);
        let Some((left, top, right, bottom)) = find_node_bounds_by_key(tree, slider_key, 0.0, 0.0)
        else {
            return None;
        };

        let min = node
            .attributes
            .get("min")
            .and_then(|value: &String| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let max = node
            .attributes
            .get("max")
            .and_then(|value: &String| value.parse::<f32>().ok())
            .unwrap_or(100.0);

        if max <= min {
            return None;
        }

        let pct = if is_vertical {
            // Vertical: top = 100%, bottom = 0% (inverted Y axis).
            let height = (bottom - top).max(1.0);
            let local_y = (y - top).clamp(0.0, height);
            1.0 - (local_y / height).clamp(0.0, 1.0)
        } else {
            let width = (right - left).max(1.0);
            let local_x = (x - left).clamp(0.0, width);
            (local_x / width).clamp(0.0, 1.0)
        };
        let value = min + (max - min) * pct;
        self.slider_values.insert(slider_key.to_string(), value);
        if action.as_deref() == Some("audio-volume") {
            let percent = value.round().clamp(0.0, 100.0) as u32;
            self.update_local_audio_percent(percent);
            if self.last_audio_slider_percent != Some(percent) {
                self.last_audio_slider_percent = Some(percent);
                return Some(CoreRequest::ServiceCommand {
                    interface: "mesh.audio".to_string(),
                    command: "set_volume".to_string(),
                    payload: serde_json::json!({
                        "device_id": "default",
                        "volume": percent as f64 / 100.0,
                    }),
                    source_module_id: self.id().to_string(),
                    source_capabilities: self.source_capabilities(),
                });
            }
        }
        None
    }

    pub(super) fn slider_value(&self, tree: &WidgetNode, slider_key: &str) -> Option<f32> {
        self.slider_values.get(slider_key).copied().or_else(|| {
            find_node_by_key(tree, slider_key).and_then(|node| {
                node.attributes
                    .get("value")
                    .and_then(|value| value.parse::<f32>().ok())
            })
        })
    }

    pub(super) fn current_checked_value(&self, tree: &WidgetNode, key: &str) -> bool {
        self.checked_values.get(key).copied().unwrap_or_else(|| {
            find_node_by_key(tree, key)
                .and_then(|node| node.attributes.get("checked"))
                .is_some_and(|value| matches!(value.as_str(), "true" | "1" | "checked"))
        })
    }

    pub(super) fn toggle_checked_value(&mut self, tree: &WidgetNode, key: &str) -> bool {
        let next = !self.current_checked_value(tree, key);
        self.checked_values.insert(key.to_string(), next);
        next
    }

    pub(super) fn pointer_event_target_key(
        &self,
        tree: &WidgetNode,
        x: f32,
        y: f32,
    ) -> Option<String> {
        find_focusable_at(tree, x, y).or_else(|| {
            find_node_path_at(tree, x, y).and_then(|path| {
                path.into_iter()
                    .rev()
                    .find(|key| find_event_handler(tree, key, "click").is_some())
            })
        })
    }

    pub(super) fn slider_release_request(
        &self,
        tree: &WidgetNode,
        slider_key: &str,
    ) -> Option<CoreRequest> {
        let node = find_node_by_key(tree, slider_key)?;
        match node.attributes.get("mesh-action").map(String::as_str) {
            Some("audio-volume") => {
                let value = self
                    .slider_values
                    .get(slider_key)
                    .copied()
                    .or_else(|| {
                        node.attributes
                            .get("value")
                            .and_then(|value| value.parse::<f32>().ok())
                    })
                    .unwrap_or(0.0);
                let percent = value.round().clamp(0.0, 100.0) as u32;
                Some(CoreRequest::ServiceCommand {
                    interface: "mesh.audio".to_string(),
                    command: "set_volume".to_string(),
                    payload: serde_json::json!({
                        "device_id": "default",
                        "volume": percent as f64 / 100.0,
                    }),
                    source_module_id: self.id().to_string(),
                    source_capabilities: self.source_capabilities(),
                })
            }
            _ => None,
        }
    }

    pub(super) fn build_click_event(
        &self,
        tree: &WidgetNode,
        node_key: &str,
        x: f32,
        y: f32,
    ) -> serde_json::Value {
        let target = find_node_by_key(tree, node_key);
        let (left, top, right, bottom) =
            find_node_bounds_by_key(tree, node_key, 0.0, 0.0).unwrap_or((0.0, 0.0, 0.0, 0.0));
        let width = (right - left).max(0.0);
        let height = (bottom - top).max(0.0);
        let bounds = serde_json::json!({
            "left": left,
            "top": top,
            "right": right,
            "bottom": bottom,
            "width": width,
            "height": height,
        });
        let position = serde_json::json!({
            "margin_left": left.round() as i32,
            "margin_top": bottom.round() as i32,
        });
        let tag = target.map(|node| node.tag.clone()).unwrap_or_default();
        let mut current_target = target
            .map(|node| element_snapshot_json(node, left - node.layout.x, top - node.layout.y))
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(object) = current_target.as_object_mut() {
            object.insert(
                "key".into(),
                serde_json::Value::String(node_key.to_string()),
            );
            object.insert("tag".into(), serde_json::Value::String(tag.clone()));
            object.insert("bounds".into(), bounds.clone());
            object.insert("position".into(), position.clone());
        }

        serde_json::json!({
            "type": "click",
            "pointer": {
                "x": x,
                "y": y,
            },
            "surface": {
                "id": self.surface_id(),
                "width": tree.layout.width,
                "height": tree.layout.height,
            },
            "current": {
                "key": node_key,
                "tag": tag,
                "bounds": bounds,
                "position": position,
            },
            "current_target": current_target
        })
    }
}

impl FrontendSurfaceComponent {
    pub(super) fn handle_component_input(
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
                    if let Some(node_key) = self.pointer_event_target_key(&tree, x, y) {
                        self.pointer_down_key = Some(node_key.clone());
                        let mut requests = Vec::new();

                        if let Some(focused_key) = find_focusable_at(&tree, x, y) {
                            let focus_changed =
                                self.focused_key.as_deref() != Some(focused_key.as_str());
                            self.focused_key = Some(focused_key.clone());
                            if focus_changed {
                                requests.extend(self.call_node_handler(
                                    &tree,
                                    &focused_key,
                                    "focus",
                                    &[],
                                )?);
                            }
                        } else {
                            self.focused_key = None;
                        }

                        if is_slider_key(&tree, &node_key) {
                            self.active_slider_key = Some(node_key.clone());
                            self.last_audio_slider_percent = None;
                            if let Some(request) =
                                self.update_slider_from_position(&tree, &node_key, x, y)
                            {
                                requests.push(request);
                            }
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
                            self.last_audio_slider_percent = None;
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
                        self.focused_key = None;
                        self.pointer_down_key = None;
                        self.active_slider_key = None;
                        self.last_audio_slider_percent = None;
                        self.dirty = true;
                    }
                } else {
                    let mut requests = Vec::new();
                    let slider_request = self
                        .active_slider_key
                        .as_ref()
                        .and_then(|slider_key| self.slider_release_request(&tree, slider_key));

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
                    self.last_audio_slider_percent = None;
                    if let Some(request) = slider_request {
                        requests.push(request);
                    }
                    if !requests.is_empty() {
                        self.dirty = true;
                        return Ok(requests);
                    }
                }
            }
            ComponentInput::PointerMove { x, y } => {
                if let Some(slider_key) = self.active_slider_key.clone() {
                    let request = self.update_slider_from_position(&tree, &slider_key, x, y);
                    let mut requests = Vec::new();
                    if let Some(request) = request {
                        requests.push(request);
                    }
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
            ComponentInput::KeyPressed { key } => {
                if let Some(focused_key) = self.focused_key.clone() {
                    if is_input_key(&tree, &focused_key) {
                        let value = self.input_values.entry(focused_key.clone()).or_default();
                        match key.as_str() {
                            "Backspace" => {
                                value.pop();
                                let current = value.clone();
                                self.dirty = true;
                                return self.call_node_handler(
                                    &tree,
                                    &focused_key,
                                    "change",
                                    &[serde_json::json!(current)],
                                );
                            }
                            _ => {}
                        }
                    } else if matches!(key.as_str(), "Enter" | " " | "Space")
                        && find_node_by_key(&tree, &focused_key)
                            .is_some_and(|node| matches!(node.tag.as_str(), "switch" | "checkbox"))
                    {
                        let value = self.toggle_checked_value(&tree, &focused_key);
                        self.dirty = true;
                        return self.call_node_handler(
                            &tree,
                            &focused_key,
                            "change",
                            &[serde_json::json!(value)],
                        );
                    }
                }
            }
            ComponentInput::KeyReleased { .. } => {}
        }

        Ok(Vec::new())
    }
}
