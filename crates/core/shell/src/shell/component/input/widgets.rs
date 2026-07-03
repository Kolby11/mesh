use super::super::*;

impl FrontendSurfaceComponent {
    pub(super) fn dispatch_text_input_value_handlers(
        &mut self,
        tree: &WidgetNode,
        input_key: &str,
        value: &str,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let payload = serde_json::json!(value);
        let mut requests = self.call_node_handler(tree, input_key, "input", &[payload.clone()])?;
        requests.extend(self.call_node_handler(tree, input_key, "change", &[payload])?);
        Ok(requests)
    }

    pub(super) fn preserve_slider_value(
        &mut self,
        tree: &WidgetNode,
        slider_key: &str,
        value: f32,
    ) {
        if let Some(script_value) = find_node_by_key(tree, slider_key).and_then(|node| {
            node.attributes
                .get("value")
                .and_then(|value| value.parse::<f32>().ok())
        }) {
            self.slider_script_values
                .insert(slider_key.to_string(), script_value);
        }
        self.slider_values.insert(slider_key.to_string(), value);
    }

    pub(super) fn update_slider_from_position(
        &mut self,
        tree: &WidgetNode,
        slider_key: &str,
        x: f32,
        y: f32,
    ) {
        let Some(node) = find_node_by_key(tree, slider_key) else {
            return;
        };
        let is_vertical = node
            .attributes
            .get("orient")
            .map(|v| v == "vertical")
            .unwrap_or(false);
        let Some((left, top, right, bottom)) = find_node_bounds_by_key(tree, slider_key, 0.0, 0.0)
        else {
            return;
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
            return;
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
        self.preserve_slider_value(tree, slider_key, value);
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

    pub(super) fn source_tag_for_key<'a>(
        &self,
        tree: &'a WidgetNode,
        key: &str,
    ) -> Option<&'a str> {
        find_node_by_key(tree, key).map(source_element_tag)
    }

    pub(super) fn is_checkable_choice_key(&self, tree: &WidgetNode, key: &str) -> bool {
        find_node_by_key(tree, key).is_some_and(|node| {
            node_is_source(node, &["switch", "checkbox"])
                || matches!(node.tag.as_str(), "switch" | "checkbox")
        })
    }

    pub(super) fn is_radio_key(&self, tree: &WidgetNode, key: &str) -> bool {
        find_node_by_key(tree, key).is_some_and(|node| node_is_source(node, &["radio"]))
    }

    pub(super) fn is_option_key(&self, tree: &WidgetNode, key: &str) -> bool {
        find_node_by_key(tree, key).is_some_and(|node| node_is_source(node, &["option"]))
    }

    pub(in crate::shell::component) fn is_menu_item_key(
        &self,
        tree: &WidgetNode,
        key: &str,
    ) -> bool {
        find_node_by_key(tree, key).is_some_and(|node| {
            node_is_source(node, &["menu-item", "command-item", "preference-row"])
        })
    }

    pub(in crate::shell::component) fn is_container_collection_item_key(
        &self,
        tree: &WidgetNode,
        key: &str,
    ) -> bool {
        find_node_by_key(tree, key).is_some_and(|node| node_is_source(node, &["tab", "list-item"]))
    }

    pub(in crate::shell::component) fn dispatch_activation_handlers(
        &mut self,
        tree: &WidgetNode,
        key: &str,
        click_event: serde_json::Value,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let mut requests = Vec::new();
        if find_click_handler(tree, key).is_some() {
            requests.extend(self.call_node_handler(tree, key, "click", &[click_event.clone()])?);
        }
        requests.extend(self.call_node_handler(tree, key, "activate", &[click_event])?);
        Ok(requests)
    }

    pub(super) fn activate_option_choice(
        &mut self,
        tree: &WidgetNode,
        option_key: &str,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(option) = find_node_by_key(tree, option_key) else {
            return Ok(Vec::new());
        };
        if node_disabled(option) {
            return Ok(Vec::new());
        }
        let value = option
            .attributes
            .get("value")
            .cloned()
            .or_else(|| option.attributes.get("label").cloned())
            .unwrap_or_default();
        self.checked_values.insert(option_key.to_string(), true);

        let Some(select_key) = ancestor_source_key(tree, option_key, &["select"]) else {
            return self.call_node_handler(tree, option_key, "change", &[serde_json::json!(value)]);
        };
        self.input_values.insert(select_key.clone(), value.clone());
        let mut requests = self.call_node_handler(
            tree,
            &select_key,
            "change",
            &[serde_json::json!(value.clone())],
        )?;
        requests.extend(self.call_node_handler(
            tree,
            option_key,
            "change",
            &[serde_json::json!(value)],
        )?);
        self.invalidate_text_state();
        Ok(requests)
    }

    pub(super) fn activate_radio_choice(
        &mut self,
        tree: &WidgetNode,
        radio_key: &str,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(radio) = find_node_by_key(tree, radio_key) else {
            return Ok(Vec::new());
        };
        if node_disabled(radio) {
            return Ok(Vec::new());
        }
        let value = radio.attributes.get("value").cloned().unwrap_or_default();
        if let Some(group_key) = ancestor_source_key(tree, radio_key, &["radio-group"]) {
            for sibling in descendant_source_keys(tree, &group_key, &["radio"]) {
                self.checked_values.insert(sibling, false);
            }
            self.input_values.insert(group_key.clone(), value.clone());
            self.checked_values.insert(radio_key.to_string(), true);
            let mut requests = self.call_node_handler(
                tree,
                &group_key,
                "change",
                &[serde_json::json!(value.clone())],
            )?;
            requests.extend(self.call_node_handler(
                tree,
                radio_key,
                "change",
                &[serde_json::json!(value)],
            )?);
            self.invalidate_interaction_restyle();
            return Ok(requests);
        }

        self.checked_values.insert(radio_key.to_string(), true);
        self.invalidate_interaction_restyle();
        self.call_node_handler(tree, radio_key, "change", &[serde_json::json!(value)])
    }

    pub(super) fn rove_focus_within_parent(
        &mut self,
        tree: &WidgetNode,
        key: &str,
        backward: bool,
    ) -> Option<String> {
        let source = self.source_tag_for_key(tree, key)?;
        let item_tags: &[&str] = if source == "option" {
            &["option"]
        } else if matches!(source, "menu-item" | "command-item" | "preference-row") {
            &["menu-item", "command-item", "preference-row"]
        } else {
            return None;
        };
        sibling_source_key(tree, key, item_tags, backward)
    }

    pub(in crate::shell::component) fn pointer_event_target_key(
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

    pub(in crate::shell::component) fn build_click_event(
        &self,
        tree: &WidgetNode,
        node_key: &str,
        x: f32,
        y: f32,
    ) -> serde_json::Value {
        let target = find_node_by_key(tree, node_key);
        let bounds =
            find_node_bounds_by_key(tree, node_key, 0.0, 0.0).unwrap_or((0.0, 0.0, 0.0, 0.0));
        self.build_click_event_for(tree, node_key, target, bounds, x, y)
    }

    /// Same event shape as `build_click_event`, but takes an already-resolved
    /// node + bounds so a caller holding several keys (e.g. hover-transition
    /// dispatch) doesn't re-walk the tree per key.
    pub(in crate::shell::component) fn build_click_event_for(
        &self,
        tree: &WidgetNode,
        node_key: &str,
        target: Option<&WidgetNode>,
        bounds: (f32, f32, f32, f32),
        x: f32,
        y: f32,
    ) -> serde_json::Value {
        let (left, top, right, bottom) = bounds;
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
            "margin_bottom": bottom.round() as i32,
            "width": width.round() as i32,
            "height": height.round() as i32,
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

    pub(super) fn build_focus_event(
        &self,
        tree: &WidgetNode,
        node_key: &str,
        event_type: &str,
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
            "margin_bottom": bottom.round() as i32,
            "width": width.round() as i32,
            "height": height.round() as i32,
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
            "type": event_type,
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

    pub(super) fn dispatch_scroll_handler(
        &mut self,
        tree: &WidgetNode,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    ) -> Result<Option<Vec<CoreRequest>>, ComponentError> {
        let Some(path) = find_node_path_at(tree, x, y) else {
            return Ok(None);
        };
        let Some(node_key) = path
            .into_iter()
            .rev()
            .find(|key| find_event_handler(tree, key, "scroll").is_some())
        else {
            return Ok(None);
        };
        if find_event_handler(tree, &node_key, "scroll").is_none() {
            return Ok(None);
        }
        let target = find_node_by_key(tree, &node_key);
        let (left, top, right, bottom) =
            find_node_bounds_by_key(tree, &node_key, 0.0, 0.0).unwrap_or((0.0, 0.0, 0.0, 0.0));
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
        let tag = target.map(|node| node.tag.clone()).unwrap_or_default();
        let event = serde_json::json!({
            "type": "scroll",
            "pointer": {
                "x": x,
                "y": y,
            },
            "delta": {
                "x": dx,
                "y": dy,
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
            }
        });
        self.call_node_handler(tree, &node_key, "scroll", &[event])
            .map(Some)
    }

    pub(super) fn slider_step_value(
        &self,
        tree: &WidgetNode,
        slider_key: &str,
        delta: f32,
    ) -> Option<f32> {
        let node = find_node_by_key(tree, slider_key)?;
        let min = node
            .attributes
            .get("min")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let max = node
            .attributes
            .get("max")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(100.0);
        if max <= min {
            return None;
        }

        let current = self
            .slider_value(tree, slider_key)
            .unwrap_or(min)
            .clamp(min, max);
        let step = node
            .attributes
            .get("step")
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| *value > 0.0)
            .unwrap_or_else(|| ((max - min) / 20.0).max(0.01));
        let raw_next = (current + delta * step).clamp(min, max);
        let stepped = (((raw_next - min) / step).round() * step + min).clamp(min, max);
        Some(stepped)
    }
}

fn node_disabled(node: &WidgetNode) -> bool {
    node.attributes
        .get("disabled")
        .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1" | "disabled"))
        || node
            .attributes
            .get("aria-disabled")
            .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1" | "disabled"))
}

fn ancestor_source_key(tree: &WidgetNode, key: &str, source_tags: &[&str]) -> Option<String> {
    let path = key_path(tree, key)?;
    path.into_iter().rev().skip(1).find(|candidate| {
        find_node_by_key(tree, candidate).is_some_and(|node| node_is_source(node, source_tags))
    })
}

fn sibling_source_key(
    tree: &WidgetNode,
    key: &str,
    source_tags: &[&str],
    backward: bool,
) -> Option<String> {
    let path = key_path(tree, key)?;
    let parent_key = path.iter().rev().nth(1)?;
    let siblings = find_node_by_key(tree, parent_key)?;
    let candidates: Vec<String> = siblings
        .children
        .iter()
        .filter(|child| node_is_source(child, source_tags) && !node_disabled(child))
        .filter_map(|child| child.attributes.get("_mesh_key").cloned())
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let index = candidates.iter().position(|candidate| candidate == key)?;
    let next_index = if backward {
        if index == 0 {
            candidates.len() - 1
        } else {
            index - 1
        }
    } else {
        (index + 1) % candidates.len()
    };
    candidates.get(next_index).cloned()
}

fn descendant_source_keys(tree: &WidgetNode, root_key: &str, source_tags: &[&str]) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(root) = find_node_by_key(tree, root_key) {
        collect_descendant_source_keys(root, source_tags, &mut keys);
    }
    keys
}

fn collect_descendant_source_keys(node: &WidgetNode, source_tags: &[&str], keys: &mut Vec<String>) {
    if node_is_source(node, source_tags)
        && let Some(key) = node.attributes.get("_mesh_key")
    {
        keys.push(key.clone());
    }
    for child in &node.children {
        collect_descendant_source_keys(child, source_tags, keys);
    }
}

fn key_path(tree: &WidgetNode, key: &str) -> Option<Vec<String>> {
    let mut path = Vec::new();
    if collect_key_path(tree, key, &mut path) {
        Some(path)
    } else {
        None
    }
}

fn collect_key_path(node: &WidgetNode, key: &str, path: &mut Vec<String>) -> bool {
    if let Some(node_key) = node.attributes.get("_mesh_key") {
        path.push(node_key.clone());
        if node_key == key {
            return true;
        }
    }
    for child in &node.children {
        if collect_key_path(child, key, path) {
            return true;
        }
    }
    if node.attributes.contains_key("_mesh_key") {
        path.pop();
    }
    false
}
