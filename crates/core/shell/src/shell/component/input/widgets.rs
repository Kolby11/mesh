use super::super::*;

#[derive(Debug, Clone, PartialEq)]
pub(in crate::shell::component) struct PressedTargetSnapshot {
    pub(super) key: String,
    pub(super) bounds: (f32, f32, f32, f32),
    pub(super) tag: String,
    pub(super) current_target: serde_json::Value,
    pub(super) activation_item: bool,
    pub(super) click_handler: Option<PressedHandlerSnapshot>,
    activate_handler: Option<PressedHandlerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct PressedHandlerSnapshot {
    handler: String,
    args: Vec<serde_json::Value>,
}

impl PressedHandlerSnapshot {
    fn from_node(node: &WidgetNode, event_name: &str) -> Option<Self> {
        if let Some(call) = node.event_handler_calls.get(event_name) {
            return Some(Self {
                handler: call.handler.clone(),
                args: call.args.clone(),
            });
        }
        node.event_handlers.get(event_name).map(|handler| Self {
            handler: handler.clone(),
            args: Vec::new(),
        })
    }
}

impl PressedTargetSnapshot {
    pub(super) fn from_node(key: &str, node: &WidgetNode, bounds: (f32, f32, f32, f32)) -> Self {
        let (left, top, right, bottom) = bounds;
        let width = (right - left).max(0.0);
        let height = (bottom - top).max(0.0);
        let bounds_json = serde_json::json!({
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
        let mut current_target =
            element_snapshot_json(node, left - node.layout.x, top - node.layout.y);
        if let Some(object) = current_target.as_object_mut() {
            object.insert("key".into(), serde_json::Value::String(key.to_string()));
            object.insert("tag".into(), serde_json::Value::String(node.tag.clone()));
            object.insert("bounds".into(), bounds_json);
            object.insert("position".into(), position);
        }
        Self {
            key: key.to_string(),
            bounds,
            tag: node.tag.clone(),
            current_target,
            activation_item: node_is_source(
                node,
                &[
                    "menu-item",
                    "command-item",
                    "preference-row",
                    "tab",
                    "list-item",
                ],
            ),
            click_handler: PressedHandlerSnapshot::from_node(node, "click"),
            activate_handler: PressedHandlerSnapshot::from_node(node, "activate"),
        }
    }
}

fn add_event_fields(mut event: serde_json::Value, fields: serde_json::Value) -> serde_json::Value {
    let Some(object) = event.as_object_mut() else {
        return event;
    };
    if let serde_json::Value::Object(fields) = fields {
        object.extend(fields);
    }
    event
}

fn dominant_direction(dx: f32, dy: f32) -> Option<&'static str> {
    if dx.abs() <= f32::EPSILON && dy.abs() <= f32::EPSILON {
        return None;
    }
    if dx.abs() >= dy.abs() {
        Some(if dx < 0.0 { "left" } else { "right" })
    } else if dy < 0.0 {
        Some("up")
    } else {
        Some("down")
    }
}

pub(in crate::shell::component) const LONG_PRESS_DELAY: Duration = Duration::from_millis(500);
const TAP_MAX_DURATION: Duration = LONG_PRESS_DELAY;
const DOUBLE_TAP_DELAY: Duration = Duration::from_millis(350);
const TOUCH_SLOP: f32 = 12.0;

fn touch_target_key(tree: &WidgetNode, x: f32, y: f32) -> Option<String> {
    const TOUCH_EVENTS: [&str; 9] = [
        "touchstart",
        "touchmove",
        "touchend",
        "touchcancel",
        "tap",
        "doubletap",
        "longpress",
        "click",
        "activate",
    ];
    find_node_path_at(tree, x, y)?
        .into_iter()
        .rev()
        .find(|key| {
            find_node_by_key(tree, key).is_some_and(|node| {
                TOUCH_EVENTS
                    .iter()
                    .any(|event| node_has_handler(node, event))
                    || node_is_source(
                        node,
                        &[
                            "menu-item",
                            "command-item",
                            "preference-row",
                            "tab",
                            "list-item",
                        ],
                    )
            })
        })
}

fn touch_distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn node_has_handler(node: &WidgetNode, event_name: &str) -> bool {
    node.event_handlers.contains_key(event_name)
        || node.event_handler_calls.contains_key(event_name)
}

impl FrontendSurfaceComponent {
    pub(super) fn dispatch_text_input_value_handlers(
        &mut self,
        tree: &WidgetNode,
        input_key: &str,
        value: &str,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let payload = serde_json::json!(value);
        let args = std::slice::from_ref(&payload);
        let mut requests = self.call_node_handler(tree, input_key, "input", args)?;
        requests.extend(self.call_node_handler(tree, input_key, "change", args)?);
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
        let Some((node, (left, top, right, bottom))) =
            find_node_with_bounds_by_key(tree, slider_key)
        else {
            return;
        };
        let is_vertical = node
            .attributes
            .get("orient")
            .map(|v| v == "vertical")
            .unwrap_or(false);
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

    pub(super) fn update_slider_from_resolved_press(
        &mut self,
        slider_key: &str,
        node: &WidgetNode,
        bounds: (f32, f32, f32, f32),
        x: f32,
        y: f32,
    ) {
        let is_vertical = node
            .attributes
            .get("orient")
            .map(|v| v == "vertical")
            .unwrap_or(false);
        let (left, top, right, bottom) = bounds;

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
            let height = (bottom - top).max(1.0);
            let local_y = (y - top).clamp(0.0, height);
            1.0 - (local_y / height).clamp(0.0, 1.0)
        } else {
            let width = (right - left).max(1.0);
            let local_x = (x - left).clamp(0.0, width);
            (local_x / width).clamp(0.0, 1.0)
        };
        let value = min + (max - min) * pct;
        if let Some(script_value) = node
            .attributes
            .get("value")
            .and_then(|value| value.parse::<f32>().ok())
        {
            self.slider_script_values
                .insert(slider_key.to_string(), script_value);
        }
        self.slider_values.insert(slider_key.to_string(), value);
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

    pub(super) fn toggle_checked_value_for_node(&mut self, key: &str, node: &WidgetNode) -> bool {
        let current = self.checked_values.get(key).copied().unwrap_or_else(|| {
            node.attributes
                .get("checked")
                .is_some_and(|value| matches!(value.as_str(), "true" | "1" | "checked"))
        });
        let next = !current;
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

    pub(in crate::shell::component) fn dispatch_resolved_activation_handlers(
        &mut self,
        node: &WidgetNode,
        click_event: serde_json::Value,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let mut requests = Vec::new();
        requests.extend(self.call_resolved_node_handler(node, "click", &[click_event.clone()])?);
        requests.extend(self.call_resolved_node_handler(node, "activate", &[click_event])?);
        Ok(requests)
    }

    pub(in crate::shell::component) fn dispatch_pressed_activation_handlers(
        &mut self,
        target: &PressedTargetSnapshot,
        click_event: serde_json::Value,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let mut requests = Vec::new();
        if let Some(handler) = &target.click_handler {
            requests.extend(self.call_pressed_handler(handler, click_event.clone())?);
        }
        if let Some(handler) = &target.activate_handler {
            requests.extend(self.call_pressed_handler(handler, click_event)?);
        }
        Ok(requests)
    }

    pub(in crate::shell::component) fn call_pressed_click_handler(
        &mut self,
        target: &PressedTargetSnapshot,
        click_event: serde_json::Value,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        target
            .click_handler
            .as_ref()
            .map(|handler| self.call_pressed_handler(handler, click_event))
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    fn call_pressed_handler(
        &mut self,
        handler: &PressedHandlerSnapshot,
        event_arg: serde_json::Value,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let mut args = handler.args.clone();
        args.push(event_arg);
        self.call_namespaced_handler(&handler.handler, &args)
    }

    pub(super) fn activate_option_choice(
        &mut self,
        tree: &WidgetNode,
        option_key: &str,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(option) = find_node_by_key(tree, option_key) else {
            return Ok(Vec::new());
        };
        self.activate_option_choice_for_node(tree, option_key, option)
    }

    pub(super) fn activate_option_choice_for_node(
        &mut self,
        tree: &WidgetNode,
        option_key: &str,
        option: &WidgetNode,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
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
        self.activate_radio_choice_for_node(tree, radio_key, radio)
    }

    pub(super) fn activate_radio_choice_for_node(
        &mut self,
        tree: &WidgetNode,
        radio_key: &str,
        radio: &WidgetNode,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
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
            return rove_aria_menu_focus(tree, key, backward);
        };
        sibling_source_key(tree, key, item_tags, backward)
    }

    pub(in crate::shell::component) fn pointer_event_target_key(
        &self,
        tree: &WidgetNode,
        x: f32,
        y: f32,
    ) -> Option<String> {
        pointer_press_hit(tree, x, y)
            .target
            .map(|target| target.key.to_owned())
    }

    pub(in crate::shell::component) fn captured_release_key(
        &self,
        tree: &WidgetNode,
        x: f32,
        y: f32,
    ) -> Option<String> {
        captured_release_key(
            tree,
            self.pointer_down_key.as_deref(),
            self.pointer_down_bounds,
            x,
            y,
        )
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

    pub(in crate::shell::component) fn pressed_target_snapshot(
        &self,
        key: &str,
        node: &WidgetNode,
        bounds: (f32, f32, f32, f32),
    ) -> PressedTargetSnapshot {
        PressedTargetSnapshot::from_node(key, node, bounds)
    }

    pub(in crate::shell::component) fn build_click_event_for_pressed_target(
        &self,
        tree: &WidgetNode,
        target: &PressedTargetSnapshot,
        x: f32,
        y: f32,
    ) -> serde_json::Value {
        let (left, top, right, bottom) = target.bounds;
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
                "key": target.key,
                "tag": target.tag,
                "bounds": bounds,
                "position": position,
            },
            "current_target": target.current_target
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
        let Some(hit) = pointer_event_handler_hit(tree, x, y, "scroll") else {
            return Ok(None);
        };
        let (left, top, right, bottom) = hit.bounds;
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
                "key": hit.key,
                "tag": hit.node.tag,
                "bounds": bounds,
            }
        });
        self.call_resolved_node_handler(hit.node, "scroll", &[event])
            .map(Some)
    }

    pub(super) fn dispatch_two_finger_scroll_handler(
        &mut self,
        tree: &WidgetNode,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    ) -> Result<Option<Vec<CoreRequest>>, ComponentError> {
        let Some(hit) = pointer_event_handler_hit(tree, x, y, "twofingerscroll") else {
            return Ok(None);
        };
        let event = add_event_fields(
            self.build_click_event_for(tree, hit.key, Some(hit.node), hit.bounds, x, y),
            serde_json::json!({
                "type": "twofingerscroll",
                "delta": { "x": dx, "y": dy },
            }),
        );
        self.call_resolved_node_handler(hit.node, "twofingerscroll", &[event])
            .map(Some)
    }

    pub(super) fn dispatch_swipe_begin(
        &mut self,
        tree: &WidgetNode,
        fingers: u32,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(target) = self.capture_gesture_target(tree, "swipe", fingers) else {
            self.gesture_capture = None;
            return Ok(Vec::new());
        };
        let Some((node, event)) = self.gesture_event(tree, &target, "swipe") else {
            return Ok(Vec::new());
        };
        let event = add_event_fields(
            event,
            serde_json::json!({
                "phase": "start",
                "fingers": fingers,
                "delta": { "x": 0.0, "y": 0.0 },
                "total_delta": { "x": 0.0, "y": 0.0 },
                "cancelled": false,
            }),
        );
        let requests = self.call_resolved_node_handler(node, "swipe", &[event])?;
        self.gesture_capture = Some(GestureCapture::Swipe {
            target,
            dx: 0.0,
            dy: 0.0,
        });
        Ok(requests)
    }

    pub(super) fn dispatch_swipe_update(
        &mut self,
        tree: &WidgetNode,
        dx: f32,
        dy: f32,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(mut capture) = self.gesture_capture.take() else {
            return Ok(Vec::new());
        };
        let result = match &mut capture {
            GestureCapture::Swipe {
                target,
                dx: total_dx,
                dy: total_dy,
            } => {
                *total_dx += dx;
                *total_dy += dy;
                let Some((node, event)) = self.gesture_event(tree, target, "swipe") else {
                    return Ok(Vec::new());
                };
                let event = add_event_fields(
                    event,
                    serde_json::json!({
                        "phase": "move",
                        "fingers": target.fingers,
                        "delta": { "x": dx, "y": dy },
                        "total_delta": { "x": *total_dx, "y": *total_dy },
                        "cancelled": false,
                    }),
                );
                self.call_resolved_node_handler(node, "swipe", &[event])
            }
            _ => Ok(Vec::new()),
        };
        self.gesture_capture = Some(capture);
        result
    }

    pub(super) fn dispatch_swipe_end(
        &mut self,
        tree: &WidgetNode,
        cancelled: bool,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(GestureCapture::Swipe { target, dx, dy }) = self.gesture_capture.take() else {
            return Ok(Vec::new());
        };
        let Some((node, event)) = self.gesture_event(tree, &target, "swipe") else {
            return Ok(Vec::new());
        };
        let elapsed = target.started_at.elapsed().as_secs_f32().max(0.001);
        let event = add_event_fields(
            event,
            serde_json::json!({
                "phase": "end",
                "fingers": target.fingers,
                "delta": { "x": 0.0, "y": 0.0 },
                "total_delta": { "x": dx, "y": dy },
                "direction": dominant_direction(dx, dy),
                "velocity": { "x": dx / elapsed, "y": dy / elapsed },
                "duration": elapsed,
                "cancelled": cancelled,
            }),
        );
        self.call_resolved_node_handler(node, "swipe", &[event])
    }

    pub(super) fn dispatch_pinch_begin(
        &mut self,
        tree: &WidgetNode,
        fingers: u32,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(target) = self.capture_gesture_target(tree, "pinch", fingers) else {
            self.gesture_capture = None;
            return Ok(Vec::new());
        };
        let Some((node, event)) = self.gesture_event(tree, &target, "pinch") else {
            return Ok(Vec::new());
        };
        let event = add_event_fields(
            event,
            serde_json::json!({
                "phase": "start",
                "fingers": fingers,
                "delta": { "x": 0.0, "y": 0.0 },
                "scale": 1.0,
                "rotation": 0.0,
                "cancelled": false,
            }),
        );
        let requests = self.call_resolved_node_handler(node, "pinch", &[event])?;
        self.gesture_capture = Some(GestureCapture::Pinch {
            target,
            dx: 0.0,
            dy: 0.0,
            scale: 1.0,
            rotation: 0.0,
        });
        Ok(requests)
    }

    pub(super) fn dispatch_pinch_update(
        &mut self,
        tree: &WidgetNode,
        dx: f32,
        dy: f32,
        scale: f32,
        rotation: f32,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(mut capture) = self.gesture_capture.take() else {
            return Ok(Vec::new());
        };
        let result = match &mut capture {
            GestureCapture::Pinch {
                target,
                dx: total_dx,
                dy: total_dy,
                scale: current_scale,
                rotation: current_rotation,
            } => {
                *total_dx += dx;
                *total_dy += dy;
                *current_scale = scale;
                *current_rotation = rotation;
                let Some((node, event)) = self.gesture_event(tree, target, "pinch") else {
                    return Ok(Vec::new());
                };
                let event = add_event_fields(
                    event,
                    serde_json::json!({
                        "phase": "move",
                        "fingers": target.fingers,
                        "delta": { "x": dx, "y": dy },
                        "total_delta": { "x": *total_dx, "y": *total_dy },
                        "scale": scale,
                        "rotation": rotation,
                        "cancelled": false,
                    }),
                );
                self.call_resolved_node_handler(node, "pinch", &[event])
            }
            _ => Ok(Vec::new()),
        };
        self.gesture_capture = Some(capture);
        result
    }

    pub(super) fn dispatch_pinch_end(
        &mut self,
        tree: &WidgetNode,
        cancelled: bool,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(GestureCapture::Pinch {
            target,
            dx,
            dy,
            scale,
            rotation,
        }) = self.gesture_capture.take()
        else {
            return Ok(Vec::new());
        };
        let Some((node, event)) = self.gesture_event(tree, &target, "pinch") else {
            return Ok(Vec::new());
        };
        let event = add_event_fields(
            event,
            serde_json::json!({
                "phase": "end",
                "fingers": target.fingers,
                "delta": { "x": 0.0, "y": 0.0 },
                "total_delta": { "x": dx, "y": dy },
                "scale": scale,
                "rotation": rotation,
                "cancelled": cancelled,
            }),
        );
        self.call_resolved_node_handler(node, "pinch", &[event])
    }

    pub(super) fn dispatch_hold_begin(
        &mut self,
        tree: &WidgetNode,
        fingers: u32,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(target) = self.capture_gesture_target(tree, "hold", fingers) else {
            self.gesture_capture = None;
            return Ok(Vec::new());
        };
        let Some((node, event)) = self.gesture_event(tree, &target, "hold") else {
            return Ok(Vec::new());
        };
        let event = add_event_fields(
            event,
            serde_json::json!({
                "phase": "start",
                "fingers": fingers,
                "duration": 0.0,
                "cancelled": false,
            }),
        );
        let requests = self.call_resolved_node_handler(node, "hold", &[event])?;
        self.gesture_capture = Some(GestureCapture::Hold { target });
        Ok(requests)
    }

    pub(super) fn dispatch_hold_end(
        &mut self,
        tree: &WidgetNode,
        cancelled: bool,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(GestureCapture::Hold { target }) = self.gesture_capture.take() else {
            return Ok(Vec::new());
        };
        let Some((node, event)) = self.gesture_event(tree, &target, "hold") else {
            return Ok(Vec::new());
        };
        let event = add_event_fields(
            event,
            serde_json::json!({
                "phase": "end",
                "fingers": target.fingers,
                "duration": target.started_at.elapsed().as_secs_f32(),
                "cancelled": cancelled,
            }),
        );
        self.call_resolved_node_handler(node, "hold", &[event])
    }

    fn capture_gesture_target(
        &self,
        tree: &WidgetNode,
        event_name: &str,
        fingers: u32,
    ) -> Option<GestureTargetCapture> {
        let (x, y) = self.hovered_pos;
        let hit = pointer_event_handler_hit(tree, x, y, event_name)?;
        Some(GestureTargetCapture {
            node_key: hit.key.to_string(),
            fingers,
            started_at: Instant::now(),
            pointer: (x, y),
        })
    }

    fn gesture_event<'a>(
        &self,
        tree: &'a WidgetNode,
        target: &GestureTargetCapture,
        event_name: &str,
    ) -> Option<(&'a WidgetNode, serde_json::Value)> {
        let (node, bounds) = find_node_with_bounds_by_key(tree, &target.node_key)?;
        if !node_has_handler(node, event_name) {
            return None;
        }
        let event = self.build_click_event_for(
            tree,
            &target.node_key,
            Some(node),
            bounds,
            target.pointer.0,
            target.pointer.1,
        );
        Some((
            node,
            add_event_fields(event, serde_json::json!({ "type": event_name })),
        ))
    }

    pub(super) fn dispatch_touch_down(
        &mut self,
        tree: &WidgetNode,
        id: i32,
        x: f32,
        y: f32,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let single_touch = self.active_touches.is_empty();
        if !single_touch {
            for touch in self.touch_gestures.values_mut() {
                touch.eligible = false;
            }
        }
        self.active_touches.insert(id, (x, y));
        let Some(node_key) = touch_target_key(tree, x, y) else {
            return Ok(Vec::new());
        };
        self.touch_targets.insert(id, node_key.clone());
        let long_press_enabled = find_node_by_key(tree, &node_key)
            .is_some_and(|node| node_has_handler(node, "longpress"));
        self.touch_gestures.insert(
            id,
            TouchGestureCapture {
                node_key,
                started_at: Instant::now(),
                origin: (x, y),
                point: (x, y),
                eligible: single_touch,
                long_press_enabled,
                long_press_fired: false,
            },
        );
        self.dispatch_touch_to_captured(tree, "touchstart", id, (x, y), false)
    }

    pub(super) fn dispatch_touch_move(
        &mut self,
        tree: &WidgetNode,
        id: i32,
        x: f32,
        y: f32,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        self.active_touches.insert(id, (x, y));
        if let Some(touch) = self.touch_gestures.get_mut(&id) {
            touch.point = (x, y);
            if touch_distance(touch.origin, touch.point) > TOUCH_SLOP {
                touch.eligible = false;
            }
        }
        self.dispatch_touch_to_captured(tree, "touchmove", id, (x, y), false)
    }

    pub(super) fn dispatch_touch_up(
        &mut self,
        tree: &WidgetNode,
        id: i32,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let point = self.active_touches.remove(&id).unwrap_or(self.hovered_pos);
        let mut requests = self.dispatch_touch_to_captured(tree, "touchend", id, point, false)?;
        if let Some(touch) = self.touch_gestures.remove(&id) {
            let duration = touch.started_at.elapsed();
            if touch.eligible
                && touch.long_press_enabled
                && !touch.long_press_fired
                && duration >= LONG_PRESS_DELAY
            {
                requests.extend(self.dispatch_touch_convenience_event(
                    tree,
                    &touch.node_key,
                    "longpress",
                    id,
                    point,
                    duration,
                    1,
                )?);
            } else if touch.eligible && !touch.long_press_fired && duration <= TAP_MAX_DURATION {
                let is_double = self.last_tap.as_ref().is_some_and(|previous| {
                    previous.node_key == touch.node_key
                        && previous.at.elapsed() <= DOUBLE_TAP_DELAY
                        && touch_distance(previous.point, point) <= TOUCH_SLOP
                });
                requests.extend(self.dispatch_touch_convenience_event(
                    tree,
                    &touch.node_key,
                    "tap",
                    id,
                    point,
                    duration,
                    if is_double { 2 } else { 1 },
                )?);
                requests.extend(self.dispatch_synthesized_touch_click(
                    tree,
                    &touch.node_key,
                    point,
                )?);
                if is_double {
                    requests.extend(self.dispatch_touch_convenience_event(
                        tree,
                        &touch.node_key,
                        "doubletap",
                        id,
                        point,
                        duration,
                        2,
                    )?);
                    self.last_tap = None;
                } else {
                    self.last_tap = Some(TapRecord {
                        node_key: touch.node_key,
                        at: Instant::now(),
                        point,
                    });
                }
            }
        }
        self.touch_targets.remove(&id);
        Ok(requests)
    }

    pub(super) fn dispatch_touch_cancel(
        &mut self,
        tree: &WidgetNode,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let changed = self.touch_points_json();
        let mut keys: Vec<String> = self.touch_targets.values().cloned().collect();
        keys.sort();
        keys.dedup();
        self.active_touches.clear();
        self.touch_targets.clear();
        self.touch_gestures.clear();
        self.last_tap = None;

        let mut requests = Vec::new();
        for key in keys {
            let Some((node, bounds)) = find_node_with_bounds_by_key(tree, &key) else {
                continue;
            };
            if !node_has_handler(node, "touchcancel") {
                continue;
            }
            let event = add_event_fields(
                self.build_click_event_for(
                    tree,
                    &key,
                    Some(node),
                    bounds,
                    self.hovered_pos.0,
                    self.hovered_pos.1,
                ),
                serde_json::json!({
                    "type": "touchcancel",
                    "touches": [],
                    "changed_touches": changed,
                    "cancelled": true,
                }),
            );
            requests.extend(self.call_resolved_node_handler(node, "touchcancel", &[event])?);
        }
        Ok(requests)
    }

    pub(in crate::shell::component) fn due_long_presses(
        &mut self,
        now: Instant,
    ) -> Vec<(i32, String, (f32, f32), Duration)> {
        let mut due = Vec::new();
        for (id, touch) in &mut self.touch_gestures {
            let duration = now.saturating_duration_since(touch.started_at);
            if touch.eligible
                && touch.long_press_enabled
                && !touch.long_press_fired
                && duration >= LONG_PRESS_DELAY
            {
                touch.long_press_fired = true;
                due.push((*id, touch.node_key.clone(), touch.point, duration));
            }
        }
        due
    }

    pub(in crate::shell::component) fn dispatch_due_long_presses(
        &mut self,
        tree: &WidgetNode,
        due: Vec<(i32, String, (f32, f32), Duration)>,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let mut requests = Vec::new();
        for (id, node_key, point, duration) in due {
            requests.extend(self.dispatch_touch_convenience_event(
                tree,
                &node_key,
                "longpress",
                id,
                point,
                duration,
                1,
            )?);
        }
        Ok(requests)
    }

    fn dispatch_touch_convenience_event(
        &mut self,
        tree: &WidgetNode,
        node_key: &str,
        event_name: &str,
        id: i32,
        point: (f32, f32),
        duration: Duration,
        tap_count: u8,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some((node, bounds)) = find_node_with_bounds_by_key(tree, node_key) else {
            return Ok(Vec::new());
        };
        if !node_has_handler(node, event_name) {
            return Ok(Vec::new());
        }
        let event = add_event_fields(
            self.build_click_event_for(tree, node_key, Some(node), bounds, point.0, point.1),
            serde_json::json!({
                "type": event_name,
                "touch": { "id": id, "x": point.0, "y": point.1 },
                "duration": duration.as_secs_f32(),
                "tap_count": tap_count,
                "synthesized": true,
            }),
        );
        self.call_resolved_node_handler(node, event_name, &[event])
    }

    fn dispatch_synthesized_touch_click(
        &mut self,
        tree: &WidgetNode,
        node_key: &str,
        point: (f32, f32),
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some((node, bounds)) = find_node_with_bounds_by_key(tree, node_key) else {
            return Ok(Vec::new());
        };
        let event = add_event_fields(
            self.build_click_event_for(tree, node_key, Some(node), bounds, point.0, point.1),
            serde_json::json!({ "synthesized_from": "touch" }),
        );
        if node_is_source(
            node,
            &[
                "menu-item",
                "command-item",
                "preference-row",
                "tab",
                "list-item",
            ],
        ) {
            self.dispatch_resolved_activation_handlers(node, event)
        } else if node_has_handler(node, "click") {
            self.call_resolved_node_handler(node, "click", &[event])
        } else {
            Ok(Vec::new())
        }
    }

    fn dispatch_touch_to_captured(
        &mut self,
        tree: &WidgetNode,
        event_name: &str,
        id: i32,
        point: (f32, f32),
        cancelled: bool,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(key) = self.touch_targets.get(&id).cloned() else {
            return Ok(Vec::new());
        };
        let Some((node, bounds)) = find_node_with_bounds_by_key(tree, &key) else {
            return Ok(Vec::new());
        };
        if !node_has_handler(node, event_name) {
            return Ok(Vec::new());
        }
        let event = add_event_fields(
            self.build_click_event_for(tree, &key, Some(node), bounds, point.0, point.1),
            serde_json::json!({
                "type": event_name,
                "touch": { "id": id, "x": point.0, "y": point.1 },
                "touches": self.touch_points_json(),
                "changed_touches": [{ "id": id, "x": point.0, "y": point.1 }],
                "cancelled": cancelled,
            }),
        );
        self.call_resolved_node_handler(node, event_name, &[event])
    }

    fn touch_points_json(&self) -> Vec<serde_json::Value> {
        let mut touches: Vec<(i32, (f32, f32))> = self
            .active_touches
            .iter()
            .map(|(id, point)| (*id, *point))
            .collect();
        touches.sort_by_key(|(id, _)| *id);
        touches
            .into_iter()
            .map(|(id, (x, y))| serde_json::json!({ "id": id, "x": x, "y": y }))
            .collect()
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
        .filter_map(|child| child.mesh_key().map(str::to_owned))
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

fn rove_aria_menu_focus(tree: &WidgetNode, key: &str, backward: bool) -> Option<String> {
    let path = key_path(tree, key)?;
    let menu = path.iter().rev().skip(1).find_map(|candidate| {
        find_node_by_key(tree, candidate).filter(|node| {
            node.attributes
                .get("role")
                .is_some_and(|role| role == "menu")
        })
    })?;
    let mut candidates = Vec::new();
    collect_aria_menu_item_keys(menu, &mut candidates);
    let index = candidates.iter().position(|candidate| candidate == key)?;
    let next_index = if backward {
        index.checked_sub(1).unwrap_or(candidates.len() - 1)
    } else {
        (index + 1) % candidates.len()
    };
    candidates.get(next_index).cloned()
}

fn collect_aria_menu_item_keys(node: &WidgetNode, keys: &mut Vec<String>) {
    let is_menu_item = node
        .attributes
        .get("role")
        .is_some_and(|role| role.starts_with("menuitem"));
    if is_menu_item
        && !node_disabled(node)
        && let Some(key) = node.mesh_key()
    {
        keys.push(key.to_owned());
    }
    for child in &node.children {
        collect_aria_menu_item_keys(child, keys);
    }
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
        && let Some(key) = node.mesh_key()
    {
        keys.push(key.to_owned());
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
    if let Some(node_key) = node.mesh_key() {
        path.push(node_key.to_owned());
        if node_key == key {
            return true;
        }
    }
    for child in &node.children {
        if collect_key_path(child, key, path) {
            return true;
        }
    }
    if node.has_mesh_key() {
        path.pop();
    }
    false
}

/// Fused press-path lookup: returns the click-target key alongside the
/// focusable-at-point key computed in the same pass. `handle_component_input`
/// used to call `find_focusable_at` a second time immediately after
/// `pointer_event_target_key` (once inside it, once explicitly) to decide the
/// focus target on every press — this returns both from one `find_focusable_at`
/// walk instead of two.
#[cfg(test)]
pub(in crate::shell::component) fn pointer_event_target_with_focus(
    tree: &WidgetNode,
    x: f32,
    y: f32,
) -> (Option<String>, Option<String>) {
    let focusable = mesh_core_interaction::find_focusable_at(tree, x, y);
    let target = focusable.clone().or_else(|| {
        find_node_path_at(tree, x, y).and_then(|path| {
            path.into_iter()
                .rev()
                .find(|key| find_event_handler(tree, key, "click").is_some())
        })
    });
    (target, focusable)
}

pub(in crate::shell::component) fn captured_release_key(
    tree: &WidgetNode,
    pointer_down_key: Option<&str>,
    pointer_down_bounds: Option<(f32, f32, f32, f32)>,
    x: f32,
    y: f32,
) -> Option<String> {
    let down_key = pointer_down_key?;
    if pointer_down_bounds.is_some_and(|bounds| super::point_in_bounds(x, y, bounds)) {
        return Some(down_key.to_owned());
    }
    let release_key = pointer_press_hit(tree, x, y)
        .target
        .map(|target| target.key.to_owned());
    (release_key.as_deref() == Some(down_key)).then(|| down_key.to_owned())
}
